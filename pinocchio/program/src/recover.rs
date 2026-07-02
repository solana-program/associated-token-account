use {
    crate::error::{
        destination_associated_address_mismatch, missing_required_signature,
        nested_associated_address_mismatch, nested_ata_illegal_owner, nested_ata_invalid_owner,
        nested_mint_illegal_owner, not_enough_account_keys, not_enough_multisig_signers,
        owner_associated_address_mismatch, owner_ata_illegal_owner, owner_ata_invalid_owner,
        owner_mint_illegal_owner, uninitialized_account, wallet_missing_required_signature,
    },
    pinocchio::{AccountView, Address, ProgramResult, cpi::Signer, instruction::seeds},
    pinocchio_associated_token_account_interface::pda::AssociatedTokenPda,
    pinocchio_token_2022::{
        instructions::{CloseAccount, MAX_MULTISIG_SIGNERS, TransferChecked},
        state::{Account, Mint, Multisig, StateWithExtensions},
    },
};

/// Recovers tokens stuck in a "nested" ATA (one that was created by mistakenly using an ATA address
/// as the wallet/owner when deriving a new ATA). Since that ATA is a PDA, the tokens would be
/// permanently inaccessible without this instruction.
///
/// The fix: this program can sign for the owner ATA so it transfers all tokens to the wallet's
/// correct ATA and closes the nested account.
///
/// ```text
///                          ┌───────────────┐
///                          │    wallet     │  (signer)
///                          └───┬───────┬───┘
///                              │       │
///                              ▼       ▼
///                   ┌─────────────┐ ┌─────────────┐
///    PDA(wallet,    │  owner_ata  │ │ destination │  PDA(wallet,
///       owner_mint) │  (mint A)   │ │  (mint B)   │      nested_mint)
///                   └─────┬───────┘ └─────────────┘
///                         │              ▲
///                         ▼              │
///                   ┌────────────┐  transfer_checked
///  PDA(owner_ata,   │ nested_ata │───────┘
///      nested_mint) │  (mint B)  │  all tokens
///                   └────────────┘
///                         │
///                   close_account
///                         │
///                   rent ──▶ wallet
/// ```
#[inline(always)]
pub(crate) fn process_recover_nested(
    program_id: &Address,
    accounts: &mut [AccountView],
) -> ProgramResult {
    let [
        nested_ata,
        nested_token_mint,
        destination_ata,
        owner_ata,
        owner_token_mint,
        wallet,
        owner_token_program,
        remaining @ ..,
    ] = accounts
    else {
        return Err(not_enough_account_keys());
    };

    // Optional to specify nested token program if different from owner one
    let nested_token_program = remaining.first().unwrap_or(owner_token_program);

    // `owner_ata` must be the canonical ATA for wallet & `owner_token_mint`
    let (derived_owner_ata, bump_seed) = AssociatedTokenPda::derive_address_and_bump_seed(
        program_id,
        wallet.address(),
        owner_token_program.address(),
        owner_token_mint.address(),
    );
    if derived_owner_ata != *owner_ata.address() {
        return Err(owner_associated_address_mismatch());
    }

    // `nested_ata` must be derived from owner_ata as the "wallet".
    // The `owner_ata` address was mistakenly used where a wallet address should have been.
    let derived_nested_ata = AssociatedTokenPda::derive_address(
        program_id,
        owner_ata.address(),
        nested_token_program.address(),
        nested_token_mint.address(),
    );
    if derived_nested_ata != *nested_ata.address() {
        return Err(nested_associated_address_mismatch());
    }

    // `destination_ata` must be the wallet's correct ATA for the nested mint
    let derived_destination_ata = AssociatedTokenPda::derive_address(
        program_id,
        wallet.address(),
        nested_token_program.address(),
        nested_token_mint.address(),
    );
    if derived_destination_ata != *destination_ata.address() {
        return Err(destination_associated_address_mismatch());
    }

    // Multisig wallets are authorized by their configured signer accounts.
    // Other wallet accounts must sign directly.
    if wallet.data_len() == Multisig::LEN
        && (wallet.owned_by(&pinocchio_token::ID) || wallet.owned_by(&pinocchio_token_2022::ID))
    {
        let wallet_signers = remaining.get(1..).unwrap_or_default();
        validate_multisig_wallet(wallet, wallet_signers)?;
    } else if !wallet.is_signer() {
        return Err(wallet_missing_required_signature());
    }

    // The owner mint must belong to the token program we will CPI into
    if !owner_token_mint.owned_by(owner_token_program.address()) {
        return Err(owner_mint_illegal_owner());
    }

    // The owner ATA must also belong to that token program so it can sign as
    // the nested account authority during the recovery CPIs
    if !owner_ata.owned_by(owner_token_program.address()) {
        return Err(owner_ata_illegal_owner());
    }

    let owner_account_data = owner_ata.try_borrow()?;
    let owner_account = StateWithExtensions::<Account>::from_bytes(&owner_account_data)?;

    // The wallet must actually control this ATA
    if owner_account.base.owner() != wallet.address() {
        return Err(owner_ata_invalid_owner());
    }
    drop(owner_account_data);

    // The nested ATA must belong to the same token program so its balance can be transferred
    if !nested_ata.owned_by(nested_token_program.address()) {
        return Err(nested_ata_illegal_owner());
    }

    let nested_account_data = nested_ata.try_borrow()?;
    let nested_account = StateWithExtensions::<Account>::from_bytes(&nested_account_data)?;

    // Confirming this is genuinely a nested ATA, not an arbitrary token account
    if nested_account.base.owner() != owner_ata.address() {
        return Err(nested_ata_invalid_owner());
    }

    // The nested mint must match the token program
    if !nested_token_mint.owned_by(nested_token_program.address()) {
        return Err(nested_mint_illegal_owner());
    }

    let nested_mint_data = nested_token_mint.try_borrow()?;
    let nested_mint = StateWithExtensions::<Mint>::from_bytes(&nested_mint_data)?;
    let amount = nested_account.base.amount();
    let decimals = nested_mint.base.decimals();
    drop(nested_account_data);

    let bump_ref = &[bump_seed];
    let seeds = seeds!(
        wallet.address().as_ref(),
        owner_token_program.address().as_ref(),
        owner_token_mint.address().as_ref(),
        bump_ref
    );

    // Move all tokens from the nested ATA to the wallet's correct ATA
    TransferChecked {
        from: nested_ata,
        mint: nested_token_mint,
        to: destination_ata,
        authority: owner_ata,
        amount,
        decimals,
        token_program: nested_token_program.address(),
    }
    .invoke_signed(&[Signer::from(&seeds)])?;

    // Close the now-empty nested ATA and return its rent lamports to the wallet
    CloseAccount {
        account: nested_ata,
        destination: wallet,
        authority: owner_ata,
        token_program: nested_token_program.address(),
    }
    .invoke_signed(&[Signer::from(&seeds)])
}

#[cold]
fn validate_multisig_wallet(
    wallet: &AccountView,
    signer_accounts: &[AccountView],
) -> ProgramResult {
    let wallet_data = wallet.try_borrow()?;
    // SAFETY: Function called after wallet data length is confirmed to be
    // `Multisig::LEN` and is owned by SPL Token or Token-2022.
    let multisig = unsafe { Multisig::from_bytes_unchecked(&wallet_data) };
    if !multisig.is_initialized() {
        return Err(uninitialized_account());
    }

    let mut num_signers: u8 = 0;
    let mut matched = [false; MAX_MULTISIG_SIGNERS];

    // Count distinct configured signers that signed
    for signer_account in signer_accounts {
        for (position, signer) in multisig.signers().iter().enumerate() {
            // Match on address, skipping signers already credited
            if signer == signer_account.address() && !matched[position] {
                // A matching account must have signed the transaction
                if !signer_account.is_signer() {
                    return Err(missing_required_signature());
                }
                matched[position] = true;
                num_signers = num_signers.wrapping_add(1);
            }
        }
    }

    // Reject unless the m-of-n threshold is met
    if num_signers < multisig.required_signers() {
        return Err(not_enough_multisig_signers());
    }

    Ok(())
}
