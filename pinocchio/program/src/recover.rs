use {
    pinocchio::{
        AccountView, Address, ProgramResult, cpi::Signer, error::ProgramError, instruction::seeds,
    },
    pinocchio_associated_token_account_interface::{
        error::AssociatedTokenAccountError, pda::AssociatedTokenPda,
    },
    pinocchio_log::log,
    pinocchio_token_2022::{
        instructions::{CloseAccount, TransferChecked},
        state::{Account, Mint, StateWithExtensions},
    },
    solana_program_pack::Pack,
    spl_token_2022_interface::{instruction::MAX_SIGNERS, state::Multisig},
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
        return Err(ProgramError::NotEnoughAccountKeys);
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
        log!("Error: Owner associated address does not match seed derivation");
        return Err(ProgramError::InvalidSeeds);
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
        log!("Error: Nested associated address does not match seed derivation");
        return Err(ProgramError::InvalidSeeds);
    }

    // `destination_ata` must be the wallet's correct ATA for the nested mint
    let derived_destination_ata = AssociatedTokenPda::derive_address(
        program_id,
        wallet.address(),
        nested_token_program.address(),
        nested_token_mint.address(),
    );
    if derived_destination_ata != *destination_ata.address() {
        log!("Error: Destination associated address does not match seed derivation");
        return Err(ProgramError::InvalidSeeds);
    }

    // Multisig wallets are authorized by their configured signer accounts.
    // Other wallet accounts must sign directly.
    if wallet.owned_by(owner_token_program.address()) && wallet.data_len() == Multisig::LEN {
        let wallet_signers = remaining.get(1..).unwrap_or_default();
        validate_multisig_wallet(wallet, wallet_signers)?;
    } else if !wallet.is_signer() {
        log!("Wallet of the owner associated token account must sign");
        return Err(ProgramError::MissingRequiredSignature);
    }

    // The owner mint must belong to the token program we will CPI into
    if !owner_token_mint.owned_by(owner_token_program.address()) {
        log!("Owner mint not owned by provided token program");
        return Err(ProgramError::IllegalOwner);
    }

    // The owner ATA must also belong to that token program so it can sign as
    // the nested account authority during the recovery CPIs
    if !owner_ata.owned_by(owner_token_program.address()) {
        log!(
            "Owner associated token account not owned by provided token program, recreate the \
             owner associated token account first"
        );
        return Err(ProgramError::IllegalOwner);
    }

    let owner_account_data = owner_ata.try_borrow()?;
    let owner_account = StateWithExtensions::<Account>::from_bytes(&owner_account_data)?;

    // The wallet must actually control this ATA
    if owner_account.base.owner() != wallet.address() {
        log!("Owner associated token account not owned by provided wallet");
        return Err(AssociatedTokenAccountError::InvalidOwner.into());
    }
    drop(owner_account_data);

    // The nested ATA must belong to the same token program so its balance can be transferred
    if !nested_ata.owned_by(nested_token_program.address()) {
        log!("Nested associated token account not owned by provided token program");
        return Err(ProgramError::IllegalOwner);
    }

    let nested_account_data = nested_ata.try_borrow()?;
    let nested_account = StateWithExtensions::<Account>::from_bytes(&nested_account_data)?;

    // Confirming this is genuinely a nested ATA, not an arbitrary token account
    if nested_account.base.owner() != owner_ata.address() {
        log!("Nested associated token account not owned by provided associated token account");
        return Err(AssociatedTokenAccountError::InvalidOwner.into());
    }

    // The nested mint must match the token program
    if !nested_token_mint.owned_by(nested_token_program.address()) {
        log!("Nested mint account not owned by provided token program");
        return Err(ProgramError::IllegalOwner);
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

#[inline(always)]
fn validate_multisig_wallet(
    wallet: &AccountView,
    signer_accounts: &[AccountView],
) -> ProgramResult {
    let wallet_data = wallet.try_borrow()?;
    let multisig = Multisig::unpack(&wallet_data)?;

    let mut num_signers: usize = 0;
    let mut matched = [false; MAX_SIGNERS];
    let signers = &multisig.signers[..usize::from(multisig.n)];

    // Count distinct configured signers that signed
    for signer_account in signer_accounts {
        for (position, signer) in signers.iter().enumerate() {
            // Match on address, skipping signers already credited
            if signer == signer_account.address() && !matched[position] {
                // A matching account must have signed the transaction
                if !signer_account.is_signer() {
                    return Err(ProgramError::MissingRequiredSignature);
                }
                matched[position] = true;
                num_signers = num_signers
                    .checked_add(1)
                    .ok_or(ProgramError::InvalidInstructionData)?;
            }
        }
    }

    // Reject unless the m-of-n threshold is met
    if num_signers < usize::from(multisig.m) {
        log!("Not enough multisig signers for wallet");
        return Err(ProgramError::MissingRequiredSignature);
    }

    Ok(())
}
