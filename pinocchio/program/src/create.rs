use {
    crate::size::get_account_data_size,
    pinocchio::{cpi::Signer, error::ProgramError, AccountView, Address, ProgramResult},
    pinocchio_associated_token_account_interface::{
        error::AssociatedTokenAccountError, pda::AssociatedTokenPda,
    },
    pinocchio_system_prefund::instructions::CreateAccountAllowPrefund,
    pinocchio_token_2022::{
        instructions::{InitializeAccount3, InitializeImmutableOwner},
        state::{AccountState, StateWithExtensions, TokenAccount},
    },
};

/// Specify when to create the associated token account.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CreateMode {
    /// Always try to create the associated token account.
    Always,
    /// Only try to create the associated token account if non-existent.
    Idempotent,
}

/// Returns `Ok(true)` only when the canonical ATA already exists and idempotent create may
/// treat the instruction as a no-op. `Ok(false)` means the helper could not validate the
/// current account as that preexisting ATA, so the caller must continue its normal checks.
#[inline(always)]
fn is_valid_existing_ata_for_idempotent(
    associated_token_account: &AccountView,
    wallet: &AccountView,
    mint: &AccountView,
    token_program: &AccountView,
) -> Result<bool, ProgramError> {
    // Preexisting ATA must already be owned by the requested token program
    if !associated_token_account.owned_by(token_program.address()) {
        return Ok(false);
    }

    let ata_data = associated_token_account.try_borrow()?;
    // Preexisting ATA must be parsable as a token account
    let Ok(token_account_ext) = StateWithExtensions::<TokenAccount>::from_bytes(&ata_data) else {
        return Ok(false);
    };

    let token_account = token_account_ext.base();
    // Preexisting ATA cannot be in the uninitialized state
    let Ok(AccountState::Initialized | AccountState::Frozen) = token_account.state() else {
        return Ok(false);
    };

    // Now that ATA is confirmed, it must match the wallet and mint supplied
    if token_account.owner() != wallet.address() {
        return Err(AssociatedTokenAccountError::InvalidOwner.into());
    }
    if token_account.mint() != mint.address() {
        return Err(ProgramError::InvalidAccountData);
    }

    Ok(true)
}

#[inline(always)]
pub(crate) fn process_create_associated_token_account(
    program_id: &Address,
    accounts: &mut [AccountView],
    create_mode: CreateMode,
) -> ProgramResult {
    let [payer, associated_token_account, wallet, mint, _system_program, token_program, ..] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    let (associated_token_address, bump_seed) = AssociatedTokenPda::get_address_and_bump_seed(
        program_id,
        wallet.address(),
        token_program.address(),
        mint.address(),
    );
    if associated_token_address != *associated_token_account.address() {
        return Err(ProgramError::InvalidSeeds);
    }

    // For `CreateIdempotent`, if the ATA already exists and is valid, return early
    if create_mode == CreateMode::Idempotent
        && is_valid_existing_ata_for_idempotent(
            associated_token_account,
            wallet,
            mint,
            token_program,
        )?
    {
        return Ok(());
    }

    if !associated_token_account.owned_by(&pinocchio_system::ID) {
        return Err(ProgramError::IllegalOwner);
    }

    let account_len = get_account_data_size(mint, token_program)?;

    // Create the PDA (handles pre-funded accounts)
    let bump_ref = &[bump_seed];
    let seeds = AssociatedTokenPda::signer_seeds(
        wallet.address(),
        token_program.address(),
        mint.address(),
        bump_ref,
    );
    let signer = Signer::from(&seeds);
    CreateAccountAllowPrefund::with_minimum_balance(
        payer,
        associated_token_account,
        account_len,
        token_program.address(),
        None,
    )?
    .invoke_signed(&[signer])?;

    // Lock the owner field (skip for SPL Token)
    if *token_program.address() != pinocchio_token::ID {
        InitializeImmutableOwner {
            account: associated_token_account,
            token_program: token_program.address(),
        }
        .invoke()?;
    }

    // Initialize the token account state
    InitializeAccount3 {
        account: associated_token_account,
        mint,
        owner: wallet.address(),
        token_program: token_program.address(),
    }
    .invoke()
}
