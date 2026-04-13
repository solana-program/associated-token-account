use {
    crate::{batch::batch_init_and_lock_owner, size::get_account_data_size},
    pinocchio::{
        AccountView, Address, ProgramResult, cpi::Signer, error::ProgramError, instruction::seeds,
    },
    pinocchio_associated_token_account_interface::{
        error::AssociatedTokenAccountError, pda::AssociatedTokenPda,
    },
    pinocchio_system_prefund::instructions::CreateAccountAllowPrefund,
    pinocchio_token::instructions::InitializeAccount3,
    pinocchio_token_2022::state::{AccountState, StateWithExtensions, TokenAccount},
};

/// Specify when to create the associated token account.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CreateMode {
    /// Always try to create the associated token account.
    Always,
    /// Only try to create the associated token account if non-existent.
    Idempotent,
}

#[inline(always)]
pub(crate) fn process_create_associated_token_account(
    program_id: &Address,
    accounts: &mut [AccountView],
    create_mode: CreateMode,
) -> ProgramResult {
    let [
        payer,
        associated_token_account,
        wallet,
        mint,
        _system_program,
        token_program,
        remaining @ ..,
    ] = accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    let rent_sysvar = remaining.first();

    let (associated_token_address, bump_seed) = AssociatedTokenPda::derive_address_and_bump_seed(
        program_id,
        wallet.address(),
        token_program.address(),
        mint.address(),
    );
    if associated_token_address != *associated_token_account.address() {
        return Err(ProgramError::InvalidSeeds);
    }

    // For `CreateIdempotent`, if the ATA already exists and is valid, it's a no-op
    if create_mode == CreateMode::Idempotent
        // Preexisting ATA must already be owned by the requested token program
        && associated_token_account.owned_by(token_program.address())
    {
        let ata_data = associated_token_account.try_borrow()?;
        // Preexisting ATA must be parsable as a token account
        if let Ok(token_account_ext) = StateWithExtensions::<TokenAccount>::from_bytes(&ata_data) {
            let token_account = token_account_ext.base();
            // Preexisting ATA cannot be in the uninitialized state
            if let Ok(account_state) = token_account.state() {
                if account_state != AccountState::Uninitialized {
                    // Now that ATA is confirmed, it must match the wallet and mint supplied
                    if token_account.owner() != wallet.address() {
                        return Err(AssociatedTokenAccountError::InvalidOwner.into());
                    }
                    if token_account.mint() != mint.address() {
                        return Err(ProgramError::InvalidAccountData);
                    }
                    // Confirmed `CreateIdempotent` no-op
                    return Ok(());
                }
            }
        }
    }

    if !associated_token_account.owned_by(&pinocchio_system::ID) {
        return Err(ProgramError::IllegalOwner);
    }

    let account_len = get_account_data_size(mint, token_program)?;

    // Create the PDA (handles pre-funded accounts)
    let bump_ref = &[bump_seed];
    let seeds = seeds!(
        wallet.address().as_ref(),
        token_program.address().as_ref(),
        mint.address().as_ref(),
        bump_ref
    );
    let signer = Signer::from(&seeds);
    CreateAccountAllowPrefund::with_minimum_balance(
        payer,
        associated_token_account,
        account_len,
        token_program.address(),
        rent_sysvar,
    )?
    .invoke_signed(&[signer])?;

    // If token-2022, lock the owner field
    if *token_program.address() != pinocchio_token::ID {
        batch_init_and_lock_owner(
            token_program.address(),
            associated_token_account,
            mint,
            wallet.address(),
        )
    } else {
        // If spl-token, just initialize
        InitializeAccount3::new(associated_token_account, mint, wallet.address()).invoke()
    }
}
