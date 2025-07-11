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
    spl_token_interface::state::{account::Account as TokenAccount, Transmutable},
};

pub const INITIALIZE_ACCOUNT_3_DISCM: u8 = 18;
pub const INITIALIZE_IMMUTABLE_OWNER_DISCM: u8 = 22;
pub const CLOSE_ACCOUNT_DISCM: u8 = 9;
pub const TRANSFER_DISCM: u8 = 3;

/// Parsed ATA accounts for create operations
pub struct CreateAccounts<'a> {
    pub payer: &'a AccountInfo,
    pub ata: &'a AccountInfo,
    pub wallet: &'a AccountInfo,
    pub mint: &'a AccountInfo,
    pub system_program: &'a AccountInfo,
    pub token_program: &'a AccountInfo,
    pub rent_sysvar: Option<&'a AccountInfo>,
}

/// Parsed Recover accounts for recover operations
pub struct RecoverAccounts<'a> {
    pub nested_ata: &'a AccountInfo,
    #[allow(dead_code)] // TBD on use
    pub nested_mint: &'a AccountInfo,
    pub dest_ata: &'a AccountInfo,
    pub owner_ata: &'a AccountInfo,
    pub owner_mint: &'a AccountInfo,
    pub wallet: &'a AccountInfo,
    pub token_prog: &'a AccountInfo,
}

/// Derive ATA PDA from wallet, token program, and mint
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

/// Check if the given program ID is Token-2022
#[inline(always)]
fn is_token_2022_program(program_id: &Pubkey) -> bool {
    const TOKEN_2022_PROGRAM_ID: Pubkey =
        pinocchio_pubkey::pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
    // This hurts 2-3 CUs on create paths, but saves almost 60 on create_token2022
    // SAFETY: Safe because we are comparing the pointers of the
    // program_id and TOKEN_2022_PROGRAM_ID, which are both const Pubkeys
    unsafe {
        core::ptr::eq(
            program_id.as_ref().as_ptr(),
            TOKEN_2022_PROGRAM_ID.as_ref().as_ptr(),
        ) || core::slice::from_raw_parts(program_id.as_ref().as_ptr(), 32)
            == core::slice::from_raw_parts(TOKEN_2022_PROGRAM_ID.as_ref().as_ptr(), 32)
    }
}

/// Get zero-copy token account reference from account info
#[inline(always)]
fn get_token_account_unchecked(account: &AccountInfo) -> &TokenAccount {
    let ata_data_slice = unsafe { account.borrow_data_unchecked() };
    unsafe { &*(ata_data_slice.as_ptr() as *const TokenAccount) }
}

/// Validate token account owner matches expected owner
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

/// Validate token account mint matches expected mint
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

/// Build InitializeAccount3 instruction data
#[inline(always)]
fn build_initialize_account3_data(owner: &Pubkey) -> [u8; 33] {
    let mut data = [0u8; 33]; // 1 byte discriminator + 32 bytes owner
    data[0] = INITIALIZE_ACCOUNT_3_DISCM;
    // unsafe variants here do not reduce CUs in benching
    data[1..33].copy_from_slice(owner.as_ref());
    data
}

/// Build InitializeImmutableOwner instruction data
#[inline(always)]
fn build_initialize_immutable_owner_data() -> [u8; 1] {
    [INITIALIZE_IMMUTABLE_OWNER_DISCM]
}

/// Build Transfer instruction data
#[inline(always)]
fn build_transfer_data(amount: u64) -> [u8; 9] {
    let mut data = [0u8; 9];
    data[0] = TRANSFER_DISCM;
    data[1..9].copy_from_slice(&amount.to_le_bytes());
    data
}

/// Build CloseAccount instruction data
#[inline(always)]
fn build_close_account_data() -> [u8; 1] {
    [CLOSE_ACCOUNT_DISCM]
}

/// Resolve rent from sysvar account or syscall
#[inline(always)]
fn resolve_rent(rent_info_opt: Option<&AccountInfo>) -> Result<Rent, ProgramError> {
    match rent_info_opt {
        Some(rent_acc) => unsafe { Rent::from_account_info_unchecked(rent_acc) }.cloned(),
        None => Rent::get(),
    }
}

/// Parse and validate the standard Recover account layout.
#[inline(always)]
fn parse_recover_accounts(accounts: &[AccountInfo]) -> Result<RecoverAccounts, ProgramError> {
    if accounts.len() < 7 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    // SAFETY: account len already checked
    unsafe {
        Ok(RecoverAccounts {
            nested_ata: accounts.get_unchecked(0),
            nested_mint: accounts.get_unchecked(1),
            dest_ata: accounts.get_unchecked(2),
            owner_ata: accounts.get_unchecked(3),
            owner_mint: accounts.get_unchecked(4),
            wallet: accounts.get_unchecked(5),
            token_prog: accounts.get_unchecked(6),
        })
    }
}

/// Parse and validate the standard Create account layout.
#[inline(always)]
fn parse_create_accounts(accounts: &[AccountInfo]) -> Result<CreateAccounts, ProgramError> {
    let rent_info = match accounts.len() {
        len if len >= 7 => Some(unsafe { accounts.get_unchecked(6) }),
        6 => None,
        _ => return Err(ProgramError::NotEnoughAccountKeys),
    };

    // SAFETY: account info already checked
    unsafe {
        Ok(CreateAccounts {
            payer: accounts.get_unchecked(0),
            ata: accounts.get_unchecked(1),
            wallet: accounts.get_unchecked(2),
            mint: accounts.get_unchecked(3),
            system_program: accounts.get_unchecked(4),
            token_program: accounts.get_unchecked(5),
            rent_sysvar: rent_info,
        })
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
        // validation is more or less the point of CreateIdempotent,
        // so TBD on these staying or going
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
    account_len_opt: Option<usize>,
) -> ProgramResult {
    // Use provided account length if available, otherwise calculate based on token program
    let space = match account_len_opt {
        Some(len) => len,
        None => {
            // Calculate correct space: 165 for base TokenAccount, +5 for ImmutableOwner extension
            if is_token_2022_program(token_prog.key()) {
                TokenAccount::LEN + 5 // 170 bytes total for Token-2022 with ImmutableOwner
            } else {
                TokenAccount::LEN // 165 bytes for regular Token
            }
        }
    };

    let seeds: &[&[u8]] = &[
        wallet.key().as_ref(),
        token_prog.key().as_ref(),
        mint_account.key().as_ref(),
        &[bump],
    ];

    // Use Rent passed in accounts if supplied to avoid syscall
    let rent = resolve_rent(rent_info_opt)?;
    create_pda_account(payer, &rent, space, token_prog.key(), ata_acc, seeds)?;

    // For Token-2022, initialize ImmutableOwner extension first
    if is_token_2022_program(token_prog.key()) {
        let initialize_immutable_owner_data = build_initialize_immutable_owner_data();

        let initialize_immutable_owner_metas = &[AccountMeta {
            pubkey: ata_acc.key(),
            is_writable: true,
            is_signer: false,
        }];

        let init_immutable_owner_ix = Instruction {
            program_id: token_prog.key(),
            accounts: initialize_immutable_owner_metas,
            data: &initialize_immutable_owner_data,
        };

        invoke(&init_immutable_owner_ix, &[ata_acc])?;
    }

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
/// For Token-2022 accounts, we create the account with the correct size (170 bytes)
/// and call InitializeImmutableOwner followed by InitializeAccount3.
pub fn process_create(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    idempotent: bool,
    bump_opt: Option<u8>,
    account_len_opt: Option<usize>,
) -> ProgramResult {
    let create_accounts = parse_create_accounts(accounts)?;

    // Check if account already exists (idempotent path)
    if check_idempotent_account(
        create_accounts.ata,
        create_accounts.wallet,
        create_accounts.mint,
        create_accounts.token_program,
        idempotent,
    )? {
        return Ok(());
    }

    let bump = match bump_opt {
        Some(provided_bump) => provided_bump,
        None => {
            let (_, computed_bump) = derive_ata_pda(
                create_accounts.wallet.key(),
                create_accounts.token_program.key(),
                create_accounts.mint.key(),
                program_id,
            );
            computed_bump
        }
    };

    create_and_initialize_ata(
        create_accounts.payer,
        create_accounts.ata,
        create_accounts.wallet,
        create_accounts.mint,
        create_accounts.system_program,
        create_accounts.token_program,
        create_accounts.rent_sysvar,
        bump,
        account_len_opt,
    )
}

/// Accounts: nested_ata, nested_mint, dest_ata, owner_ata, owner_mint, wallet, token_prog, [..multisig signer accounts]
pub fn process_recover(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    bump_opt: Option<u8>,
) -> ProgramResult {
    // SAFETY: Accounts bounded by runtime
    let recover_accounts = parse_recover_accounts(accounts)?;

    let bump = match bump_opt {
        Some(provided_bump) => provided_bump,
        None => {
            let (_, computed_bump) = derive_ata_pda(
                recover_accounts.wallet.key(),
                recover_accounts.token_prog.key(),
                recover_accounts.owner_mint.key(),
                program_id,
            );
            computed_bump
        }
    };

    if !recover_accounts.wallet.is_signer() {
        // Check if this is a token-program multisig owner
        if unsafe { recover_accounts.wallet.owner() } != recover_accounts.token_prog.key() {
            return Err(ProgramError::MissingRequiredSignature);
        }

        #[allow(unused_imports)]
        use spl_token_interface::state::{multisig::Multisig, Initializable, Transmutable};

        // Load and validate multisig state
        let wallet_data_slice = unsafe { recover_accounts.wallet.borrow_data_unchecked() };
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

    // Owner_ata and nested_ata validation no longer performed here.
    let amount_to_recover = get_token_account_unchecked(recover_accounts.nested_ata).amount();

    let transfer_data = build_transfer_data(amount_to_recover);

    let transfer_metas = &[
        AccountMeta {
            pubkey: recover_accounts.nested_ata.key(),
            is_writable: true,
            is_signer: false,
        },
        AccountMeta {
            pubkey: recover_accounts.dest_ata.key(),
            is_writable: true,
            is_signer: false,
        },
        AccountMeta {
            pubkey: recover_accounts.owner_ata.key(),
            is_writable: false,
            is_signer: true,
        },
    ];

    let ix_transfer = Instruction {
        program_id: recover_accounts.token_prog.key(),
        accounts: transfer_metas,
        data: &transfer_data,
    };

    let pda_seeds_raw: &[&[u8]] = &[
        recover_accounts.wallet.key().as_ref(),
        recover_accounts.token_prog.key().as_ref(),
        recover_accounts.owner_mint.key().as_ref(),
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
        &[
            recover_accounts.nested_ata,
            recover_accounts.dest_ata,
            recover_accounts.owner_ata,
        ],
        &[pda_signer.clone()],
    )?;

    let close_data = build_close_account_data();

    let close_metas = &[
        AccountMeta {
            pubkey: recover_accounts.nested_ata.key(),
            is_writable: true,
            is_signer: false,
        },
        AccountMeta {
            pubkey: recover_accounts.wallet.key(),
            is_writable: true,
            is_signer: false,
        },
        AccountMeta {
            pubkey: recover_accounts.owner_ata.key(),
            is_writable: false,
            is_signer: true,
        },
        AccountMeta {
            pubkey: recover_accounts.token_prog.key(),
            is_writable: false,
            is_signer: false,
        },
    ];

    let ix_close = Instruction {
        program_id: recover_accounts.token_prog.key(),
        accounts: close_metas,
        data: &close_data,
    };

    invoke_signed(
        &ix_close,
        &[
            recover_accounts.nested_ata,
            recover_accounts.wallet,
            recover_accounts.owner_ata,
            recover_accounts.token_prog,
        ],
        &[pda_signer],
    )?;
    Ok(())
}
