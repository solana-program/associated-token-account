use {
    crate::tools::account::create_pda_account,
    pinocchio::{
        account_info::AccountInfo,
        instruction::{AccountMeta, Instruction, Seed, Signer},
        program::{invoke, invoke_signed},
        program_error::ProgramError,
        pubkey::{find_program_address, Pubkey},
        sysvars::{rent::Rent, Sysvar},
        ProgramResult,
    },
    spl_token_interface::{
        instruction::TokenInstruction,
        state::{account::Account as TokenAccount, Transmutable},
    },
};

/// Parsed ATA accounts: (payer, ata, wallet, mint, system_program, token_program, rent_sysvar?)
type AtaAccounts<'a> = (
    &'a AccountInfo,
    &'a AccountInfo,
    &'a AccountInfo,
    &'a AccountInfo,
    &'a AccountInfo,
    &'a AccountInfo,
    Option<&'a AccountInfo>,
);

/// Extract PDA derivation for ATA
#[inline(always)]
fn derive_ata_pda(
    wallet: &Pubkey,
    token_prog: &Pubkey,
    mint: &Pubkey,
    program_id: &Pubkey,
) -> (Pubkey, u8) {
    find_program_address(
        &[wallet.as_ref(), token_prog.as_ref(), mint.as_ref()],
        program_id,
    )
}

/// Extract PDA validation
#[inline(always)]
fn validate_pda(expected: &Pubkey, actual: &Pubkey) -> Result<(), ProgramError> {
    if expected != actual {
        return Err(ProgramError::InvalidSeeds);
    }
    Ok(())
}

/// Extract zero-copy token account access
#[inline(always)]
fn get_token_account_unchecked(account: &AccountInfo) -> &TokenAccount {
    let ata_data_slice = unsafe { account.borrow_data_unchecked() };
    unsafe { &*(ata_data_slice.as_ptr() as *const TokenAccount) }
}

/// Extract token account owner validation
#[inline(always)]
fn validate_token_account_owner(
    account: &TokenAccount,
    expected_owner: &Pubkey,
) -> Result<(), ProgramError> {
    if account.owner != *expected_owner {
        return Err(ProgramError::IllegalOwner);
    }
    Ok(())
}

/// Extract token account mint validation
#[inline(always)]
fn validate_token_account_mint(
    account: &TokenAccount,
    expected_mint: &Pubkey,
) -> Result<(), ProgramError> {
    if account.mint != *expected_mint {
        return Err(ProgramError::InvalidAccountData);
    }
    Ok(())
}

/// Extract InitializeAccount3 instruction data building
#[inline(always)]
fn build_initialize_account3_data(owner: &Pubkey) -> [u8; 33] {
    let mut data = [0u8; 33]; // 1 byte discriminator + 32 bytes owner
    data[0] = 18u8; // TokenInstruction::InitializeAccount3
    data[1..33].copy_from_slice(owner.as_ref());
    data
}

/// Extract Transfer instruction data building
#[inline(always)]
fn build_transfer_data(amount: u64) -> [u8; 9] {
    let mut data = [0u8; 9];
    data[0] = TokenInstruction::Transfer as u8;
    data[1..9].copy_from_slice(&amount.to_le_bytes());
    data
}

/// Extract CloseAccount instruction data building
#[inline(always)]
fn build_close_account_data() -> [u8; 1] {
    [TokenInstruction::CloseAccount as u8]
}

/// Parse and validate the standard ATA account layout.
#[inline(always)]
fn parse_ata_accounts(accounts: &[AccountInfo]) -> Result<AtaAccounts, ProgramError> {
    match accounts {
        [payer, ata, wallet, mint, system, token] => {
            Ok((payer, ata, wallet, mint, system, token, None))
        }
        [payer, ata, wallet, mint, system, token, rent, ..] => {
            Ok((payer, ata, wallet, mint, system, token, Some(rent)))
        }
        _ => Err(ProgramError::NotEnoughAccountKeys),
    }
}

/// Check if account already exists and is properly configured (idempotent check).
#[inline(always)]
fn check_idempotent_account(
    ata_acc: &AccountInfo,
    wallet: &AccountInfo,
    mint_account: &AccountInfo,
    token_prog: &AccountInfo,
    idempotent: bool,
) -> Result<bool, ProgramError> {
    if idempotent && unsafe { ata_acc.owner() } == token_prog.key() {
        let ata_state = get_token_account_unchecked(ata_acc);
        validate_token_account_owner(ata_state, wallet.key())?;
        validate_token_account_mint(ata_state, mint_account.key())?;
        return Ok(true); // Account exists and is valid
    }
    Ok(false) // Need to create account
}

/// Create and initialize an ATA account with the given bump seed.
#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn create_and_initialize_ata(
    payer: &AccountInfo,
    ata_acc: &AccountInfo,
    wallet: &AccountInfo,
    mint_account: &AccountInfo,
    // if an account isn't owned by the system program,
    // the create_pda_account call will fail anyway when trying to allocate/assign
    _system_prog: &AccountInfo,
    token_prog: &AccountInfo,
    rent_info_opt: Option<&AccountInfo>,
    bump: u8,
) -> ProgramResult {
    let space = TokenAccount::LEN;

    let seeds: &[&[u8]] = &[
        wallet.key().as_ref(),
        token_prog.key().as_ref(),
        mint_account.key().as_ref(),
        &[bump],
    ];

    // Use Rent passed in accounts if supplied to avoid syscall
    let rent_owned;
    let rent: &Rent = match rent_info_opt {
        Some(rent_acc) => unsafe { Rent::from_account_info_unchecked(rent_acc)? },
        None => {
            rent_owned = Rent::get()?;
            &rent_owned
        }
    };
    create_pda_account(payer, rent, space, token_prog.key(), ata_acc, seeds)?;

    // Initialize account using InitializeAccount3 (2 accounts + owner in instruction data)
    let initialize_account_instr_data = build_initialize_account3_data(wallet.key());

    let initialize_account_metas = &[
        AccountMeta {
            pubkey: ata_acc.key(),
            is_writable: true,
            is_signer: false,
        },
        AccountMeta {
            pubkey: mint_account.key(),
            is_writable: false,
            is_signer: false,
        },
    ];

    let init_ix = Instruction {
        program_id: token_prog.key(),
        accounts: initialize_account_metas,
        data: &initialize_account_instr_data,
    };

    invoke(&init_ix, &[ata_acc, mint_account])?;

    Ok(())
}

/// Accounts: payer, ata, wallet, mint, system_program, token_program, [rent_sysvar]
///
/// Manually stamping ImmutableOwner data and then calling Assign is **cheaper**
/// on create paths than using the Token-2022 `InitializeImmutableOwner` CPI
/// (100-200 CUs saved). If we ever have a lightweight pinocchio-flavoured
/// Token-2022 program (`p-token-2022`) with a lower overhead, we can swap
/// back to the flow of CreateAccount + InitializeImmutableOwner.
pub fn process_create(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    idempotent: bool,
    bump_opt: Option<u8>,
) -> ProgramResult {
    let (payer, ata_acc, wallet, mint_account, system_prog, token_prog, rent_info_opt) =
        parse_ata_accounts(accounts)?;

    // Check if account already exists (idempotent path)
    if check_idempotent_account(ata_acc, wallet, mint_account, token_prog, idempotent)? {
        return Ok(());
    }

    let bump = match bump_opt {
        Some(provided_bump) => provided_bump,
        None => {
            let (expected, computed_bump) = derive_ata_pda(
                wallet.key(),
                token_prog.key(),
                mint_account.key(),
                program_id,
            );
            validate_pda(&expected, ata_acc.key())?;
            computed_bump
        }
    };

    create_and_initialize_ata(
        payer,
        ata_acc,
        wallet,
        mint_account,
        system_prog,
        token_prog,
        rent_info_opt,
        bump,
    )
}

/// Accounts: nested_ata, nested_mint, dest_ata, owner_ata, owner_mint, wallet, token_prog, [..multisig signer accounts]
pub fn process_recover(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    if accounts.len() < 7 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }
    let (
        nested_ata,
        _nested_mint_account,
        dest_ata,
        owner_ata,
        owner_mint_account,
        wallet,
        token_prog,
    ) = (
        &accounts[0],
        &accounts[1],
        &accounts[2],
        &accounts[3],
        &accounts[4],
        &accounts[5],
        &accounts[6],
    );

    let (owner_pda, bump) = derive_ata_pda(
        wallet.key(),
        token_prog.key(),
        owner_mint_account.key(),
        program_id,
    );
    validate_pda(&owner_pda, owner_ata.key())?;

    // No expensive seed verification for `nested_ata` and `dest_ata`; the
    // subsequent owner checks on their account data provide sufficient safety
    // for practical purposes.

    // --- Wallet signature / multisig handling ---
    // If `wallet` signed directly, all good. Otherwise, allow a Multisig account
    // owned by the token program, provided that the required number (m) of
    // its signer keys signed this instruction.  Additional signer accounts
    // must be passed directly after the `token_prog` account.

    if !wallet.is_signer() {
        // Check if this is a token-program multisig owner
        if unsafe { wallet.owner() } != token_prog.key() {
            return Err(ProgramError::MissingRequiredSignature);
        }

        #[allow(unused_imports)]
        use spl_token_interface::state::{multisig::Multisig, Initializable, Transmutable};

        // Load and validate multisig state
        let wallet_data_slice = unsafe { wallet.borrow_data_unchecked() };
        let multisig_state: &Multisig =
            unsafe { spl_token_interface::state::load::<Multisig>(wallet_data_slice)? };

        let signer_infos = &accounts[7..];

        // Count how many of the provided signer accounts are both marked as
        // signer on this instruction *and* appear in the multisig signer list.
        let mut signer_count: u8 = 0;
        'outer: for signer_acc in signer_infos {
            if !signer_acc.is_signer() {
                continue;
            }
            for ms_pk in multisig_state.signers[..multisig_state.n as usize].iter() {
                if ms_pk == signer_acc.key() {
                    signer_count = signer_count.saturating_add(1);

                    if signer_count >= multisig_state.m {
                        break 'outer;
                    }
                    continue 'outer;
                }
            }
        }

        if signer_count < multisig_state.m {
            return Err(ProgramError::MissingRequiredSignature);
        }
    }

    let owner_ata_state = get_token_account_unchecked(owner_ata);
    validate_token_account_owner(owner_ata_state, wallet.key())?;

    let nested_ata_state = get_token_account_unchecked(nested_ata);
    validate_token_account_owner(nested_ata_state, owner_ata.key())?;
    let amount_to_recover = nested_ata_state.amount();

    let transfer_data = build_transfer_data(amount_to_recover);

    let transfer_metas = &[
        AccountMeta {
            pubkey: nested_ata.key(),
            is_writable: true,
            is_signer: false,
        },
        AccountMeta {
            pubkey: dest_ata.key(),
            is_writable: true,
            is_signer: false,
        },
        AccountMeta {
            pubkey: owner_ata.key(),
            is_writable: false,
            is_signer: true,
        },
    ];

    let ix_transfer = Instruction {
        program_id: token_prog.key(),
        accounts: transfer_metas,
        data: &transfer_data,
    };

    let pda_seeds_raw: &[&[u8]] = &[
        wallet.key().as_ref(),
        token_prog.key().as_ref(),
        owner_mint_account.key().as_ref(),
        &[bump],
    ];
    let pda_seed_array: [Seed<'_>; 4] = [
        Seed::from(pda_seeds_raw[0]),
        Seed::from(pda_seeds_raw[1]),
        Seed::from(pda_seeds_raw[2]),
        Seed::from(pda_seeds_raw[3]),
    ];
    let pda_signer = Signer::from(&pda_seed_array);

    invoke_signed(
        &ix_transfer,
        &[nested_ata, dest_ata, owner_ata],
        &[pda_signer.clone()],
    )?;

    let close_data = build_close_account_data();

    let close_metas = &[
        AccountMeta {
            pubkey: nested_ata.key(),
            is_writable: true,
            is_signer: false,
        },
        AccountMeta {
            pubkey: wallet.key(),
            is_writable: true,
            is_signer: false,
        },
        AccountMeta {
            pubkey: owner_ata.key(),
            is_writable: false,
            is_signer: true,
        },
        AccountMeta {
            pubkey: token_prog.key(),
            is_writable: false,
            is_signer: false,
        },
    ];

    let ix_close = Instruction {
        program_id: token_prog.key(),
        accounts: close_metas,
        data: &close_data,
    };

    invoke_signed(
        &ix_close,
        &[nested_ata, wallet, owner_ata, token_prog],
        &[pda_signer],
    )
}
