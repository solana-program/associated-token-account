use {
    crate::{error::ToProgramError, token_account::parse_token_account_mint_and_owner},
    pinocchio::{error::ProgramError, AccountView, Address, ProgramResult},
    pinocchio_associated_token_account_interface::error::AssociatedTokenAccountError,
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

#[inline(always)]
fn is_valid_existing_ata_for_idempotent(
    associated_token_account: &AccountView,
    wallet: &AccountView,
    mint: &AccountView,
    token_program: &AccountView,
) -> Result<bool, ProgramError> {
    if !associated_token_account.owned_by(token_program.address()) {
        return Ok(false);
    }

    let ata_data = associated_token_account.try_borrow()?;
    let Some((ata_mint, ata_owner)) = parse_token_account_mint_and_owner(&ata_data) else {
        return Ok(false);
    };

    if ata_owner != *wallet.address() {
        return Err(AssociatedTokenAccountError::InvalidOwner.to_program_err());
    }
    if ata_mint != *mint.address() {
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
