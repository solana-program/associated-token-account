use {
    crate::error::ToProgramError,
    pinocchio::{error::ProgramError, AccountView, Address, ProgramResult},
    pinocchio_associated_token_account_interface::error::AssociatedTokenAccountError,
    pinocchio_token_2022::state::{AccountState, StateWithExtensions, TokenAccount},
};

use pinocchio_associated_token_account_interface::pda::AssociatedTokenPda;

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
        return Err(AssociatedTokenAccountError::InvalidOwner.to_program_err());
    }
    if token_account.mint() != mint.address() {
        return Err(ProgramError::InvalidAccountData);
    }

    Ok(true)
}

#[inline(always)]
pub(crate) fn process_create_associated_token_account(
    program_id: &Address,
    accounts: &[AccountView],
    create_mode: CreateMode,
) -> ProgramResult {
    let [_payer, associated_token_account, wallet, mint, _system_program, token_program, ..] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    let (associated_token_address, _bump_seed) = AssociatedTokenPda::get_address_and_bump_seed(
        program_id,
        wallet.address(),
        token_program.address(),
        mint.address(),
    );
    if associated_token_address != *associated_token_account.address() {
        return Err(ProgramError::InvalidSeeds);
    }

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

    unimplemented!()
}
