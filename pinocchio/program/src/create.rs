use {
    crate::{
        batch::batch_init_and_lock_owner,
        error::{
            illegal_owner, incorrect_program_id, invalid_account_data, invalid_owner,
            invalid_seeds, not_enough_account_keys,
        },
        size::get_token_2022_account_data_size,
    },
    pinocchio::{AccountView, Address, ProgramResult, cpi::Signer, instruction::seeds},
    pinocchio_associated_token_account_interface::{
        instruction::CreateMode, pda::AssociatedTokenPda,
    },
    pinocchio_system::instructions::CreateAccountAllowPrefund,
    pinocchio_token::instructions::{InitializeAccount, InitializeAccount3},
    pinocchio_token_2022::state::{Account, AccountState, StateWithExtensions},
};

#[inline(always)]
pub(crate) fn process_create_associated_token_account(
    program_id: &Address,
    accounts: &mut [AccountView],
    create_mode: CreateMode,
    accept_rent_sysvar: bool,
    bump_hint: Option<u8>,
    account_len_hint: Option<u32>,
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
        return Err(not_enough_account_keys());
    };

    // For `CreateIdempotent`, if the ATA already exists and is valid, it's a no-op
    if create_mode == CreateMode::Idempotent
        // Preexisting ATA must already be owned by the requested token program
        && associated_token_account.owned_by(token_program.address())
    {
        let ata_data = associated_token_account.try_borrow()?;
        // Preexisting ATA must be parsable as a token account
        if let Ok(token_account) = StateWithExtensions::<Account>::from_bytes(&ata_data) {
            // Preexisting ATA cannot be in the uninitialized state
            if let Ok(account_state) = token_account.base.state() {
                if account_state != AccountState::Uninitialized {
                    // Must match the wallet and mint supplied
                    if token_account.base.owner() != wallet.address() {
                        return Err(invalid_owner());
                    }
                    if token_account.base.mint() != mint.address() {
                        return Err(invalid_account_data());
                    }
                    // Validate expected address, using bump hint if provided
                    let derived_ata_addr = if let Some(bump) = bump_hint {
                        // When a `bump` is provided, the address is derived directly without performing
                        // an on-curve check, since the account already exists. An ATA cannot be created
                        // with either a non-canonical bump or an on-curve address.
                        Address::derive_address(
                            &[
                                wallet.address().as_array(),
                                token_program.address().as_array(),
                                mint.address().as_array(),
                            ],
                            Some(bump),
                            program_id,
                        )
                    } else {
                        AssociatedTokenPda::derive_address(
                            program_id,
                            wallet.address(),
                            token_program.address(),
                            mint.address(),
                        )
                    };
                    if derived_ata_addr != *associated_token_account.address() {
                        return Err(invalid_seeds());
                    }
                    // Confirmed `CreateIdempotent` no-op
                    return Ok(());
                }
            }
        }
    }

    let rent_sysvar = if accept_rent_sysvar {
        // `CreateWithArgs` accepts rent as an optional account
        remaining.first()
    } else {
        // `Create` / `CreateIdempotent` ignore trailing accounts
        None
    };

    let (derived_ata_addr, bump_seed) = match bump_hint {
        Some(bump) => (
            AssociatedTokenPda::derive_address_with_bump_hint(
                program_id,
                wallet.address(),
                token_program.address(),
                mint.address(),
                bump,
            )?,
            bump,
        ),
        None => AssociatedTokenPda::derive_address_and_bump_seed(
            program_id,
            wallet.address(),
            token_program.address(),
            mint.address(),
        ),
    };
    if derived_ata_addr != *associated_token_account.address() {
        return Err(invalid_seeds());
    }

    if !associated_token_account.owned_by(&pinocchio_system::ID) {
        return Err(illegal_owner());
    }

    let is_spl_token = *token_program.address() == pinocchio_token::ID;
    let account_len = if is_spl_token {
        Account::BASE_LEN as u64
    } else if *token_program.address() == pinocchio_token_2022::ID {
        // Undersized accounts fail during initialization and excessive sizes fail
        // through rent/system account-size limits.
        if let Some(account_len_hint) = account_len_hint {
            account_len_hint as u64
        } else {
            get_token_2022_account_data_size(mint, token_program)?
        }
    } else {
        return Err(incorrect_program_id());
    };

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
    if !is_spl_token {
        batch_init_and_lock_owner(
            token_program.address(),
            associated_token_account,
            mint,
            wallet,
            rent_sysvar,
        )
    } else if let Some(rent) = rent_sysvar {
        // If rent account was supplied, save CUs by passing it into plain `InitializeAccount`.
        // Performs slightly better than `InitializeAccount2` given we already have owner account.
        InitializeAccount::new(associated_token_account, mint, wallet, rent).invoke()
    } else {
        InitializeAccount3::new(associated_token_account, mint, wallet.address()).invoke()
    }
}
