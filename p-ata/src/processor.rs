#![allow(unexpected_cfgs)]

use {
    crate::account::create_pda_account,
    pinocchio::{
        account_info::AccountInfo,
        cpi,
        instruction::{AccountMeta, Instruction, Seed, Signer},
        msg,
        program::{invoke, invoke_signed},
        program_error::ProgramError,
        pubkey::{find_program_address, Pubkey},
        sysvars::{rent::Rent, Sysvar},
        ProgramResult,
    },
    pinocchio_pubkey::derive_address,
    spl_token_interface::state::{
        account::Account as TokenAccount,
        mint::Mint,
        multisig::{Multisig, MAX_SIGNERS},
        Transmutable,
    },
};

#[cfg(target_os = "solana")]
use pinocchio::syscalls::sol_curve_validate_point;

pub const INITIALIZE_ACCOUNT_3_DISCM: u8 = 18;
pub const INITIALIZE_IMMUTABLE_OWNER_DISCM: u8 = 22;
pub const CLOSE_ACCOUNT_DISCM: u8 = 9;
pub const TRANSFER_CHECKED_DISCM: u8 = 12;

// Token-2022 AccountType::Account discriminator value
const ACCOUNTTYPE_ACCOUNT: u8 = 2;

// Compile-time verification that TokenAccount::LEN is >= 109
const _: [(); TokenAccount::LEN - 109] = [(); TokenAccount::LEN - 109];

/// Parsed ATA accounts for create operations
pub struct CreateAccounts<'a> {
    pub payer: &'a AccountInfo,
    pub associated_token_account_to_create: &'a AccountInfo,
    pub wallet: &'a AccountInfo,
    pub mint: &'a AccountInfo,
    pub system_program: &'a AccountInfo,
    pub token_program: &'a AccountInfo,
    pub rent_sysvar: Option<&'a AccountInfo>,
}

/// Parsed Recover accounts for recover operations
pub struct RecoverNestedAccounts<'a> {
    pub nested_associated_token_account: &'a AccountInfo,
    pub nested_mint: &'a AccountInfo,
    pub destination_associated_token_account: &'a AccountInfo,
    pub owner_associated_token_account: &'a AccountInfo,
    pub owner_mint: &'a AccountInfo,
    pub wallet: &'a AccountInfo,
    pub token_program: &'a AccountInfo,
}

/// Derive ATA PDA from wallet, token program, and mint.
/// This is the least efficient derivation method, used when no bump is provided.
/// The address returned is guaranteed to be off-curve and canonical.
///
/// Returns: (address, bump)
#[inline(always)]
fn derive_canoncial_ata_pda(
    wallet: &Pubkey,
    token_program: &Pubkey,
    mint: &Pubkey,
    program_id: &Pubkey,
) -> (Pubkey, u8) {
    find_program_address(
        &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()],
        program_id,
    )
}

/// Check if the given program ID is SPL Token (not Token-2022)
#[inline(always)]
fn is_spl_token_program(program_id: &Pubkey) -> bool {
    const SPL_TOKEN_PROGRAM_ID: Pubkey =
        pinocchio_pubkey::pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
    // SAFETY: Safe because we are comparing the pointers of the
    // program_id and SPL_TOKEN_PROGRAM_ID, which are both const Pubkeys
    unsafe {
        core::slice::from_raw_parts(program_id.as_ref().as_ptr(), 32)
            == core::slice::from_raw_parts(SPL_TOKEN_PROGRAM_ID.as_ref().as_ptr(), 32)
    }
}

/// Check if the given program ID is Token-2022
#[inline(always)]
fn is_token_2022_program(program_id: &Pubkey) -> bool {
    const TOKEN_2022_PROGRAM_ID: Pubkey =
        pinocchio_pubkey::pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
    // SAFETY: Safe because we are comparing the pointers of the
    // program_id and TOKEN_2022_PROGRAM_ID, which are both const Pubkeys
    unsafe {
        core::slice::from_raw_parts(program_id.as_ref().as_ptr(), 32)
            == core::slice::from_raw_parts(TOKEN_2022_PROGRAM_ID.as_ref().as_ptr(), 32)
    }
}

/// Get the required account size for a Token-2022 mint using GetAccountDataSize CPI
/// Returns the account size in bytes
#[inline(always)]
fn get_token_2022_account_size_via_cpi(
    mint_account: &AccountInfo,
    token_program: &AccountInfo,
) -> Result<usize, ProgramError> {
    // Build GetAccountDataSize instruction (discriminator 21, no extension_types)
    let instruction_data = [21u8]; // Just the discriminator, no additional extension types

    let get_size_metas = &[AccountMeta {
        pubkey: mint_account.key(),
        is_writable: false,
        is_signer: false,
    }];

    let get_size_ix = Instruction {
        program_id: token_program.key(),
        accounts: get_size_metas,
        data: &instruction_data,
    };

    // Execute the CPI to get account data size
    cpi::invoke(&get_size_ix, &[mint_account])?;

    // Get return data from the CPI
    let return_data = cpi::get_return_data().ok_or(ProgramError::InvalidAccountData)?;

    if return_data.as_slice().len() != 8 {
        return Err(ProgramError::InvalidAccountData);
    }

    // Deserialize as little-endian u64
    let mut size_bytes = [0u8; 8];
    size_bytes.copy_from_slice(return_data.as_slice());
    let size = u64::from_le_bytes(size_bytes) as usize;

    Ok(size)
}

/// Check if a Token-2022 mint has extensions by examining its data length
#[inline(always)]
fn token_2022_mint_has_extensions(mint_account: &AccountInfo) -> bool {
    const MINT_BASE_SIZE: usize = 82; // Base Mint size
    const MINT_WITH_TYPE_SIZE: usize = MINT_BASE_SIZE + 1; // Base + AccountType

    // If mint data is larger than base + type, it has extensions
    mint_account.data_len() > MINT_WITH_TYPE_SIZE
}

/// Check if account data represents an initialized token account.
/// Mimics p-token's is_initialized_account check.
///
/// Safety: caller must ensure account_data.len() >= 109.
#[inline(always)]
unsafe fn is_initialized_account(account_data: &[u8]) -> bool {
    // Token account state is at offset 108 (after mint, owner, amount, delegate fields)
    // State: 0 = Uninitialized, 1 = Initialized, 2 = Frozen
    account_data[108] != 0
}

/// Validate that account data represents a valid token account.
/// Replicates Token-2022's GenericTokenAccount::valid_account_data checks.
#[inline(always)]
fn valid_token_account_data(account_data: &[u8]) -> bool {
    // Regular Token account: exact length match and initialized
    if account_data.len() == TokenAccount::LEN {
        // SAFETY: TokenAccount::LEN is compile-ensured to be >= 109
        return unsafe { is_initialized_account(account_data) };
    }

    // Token-2022 account with extensions
    if account_data.len() > TokenAccount::LEN
        // TODO: validate we need this!
        && account_data.len() != Multisig::LEN  // Avoid confusion with multisig
        // SAFETY: TokenAccount::LEN is compile-ensured to be >= 109
        && unsafe { is_initialized_account(account_data) }
    {
        // Check AccountType discriminator at position TokenAccount::LEN
        return account_data[TokenAccount::LEN] == ACCOUNTTYPE_ACCOUNT;
    }

    false
}

/// Get zero-copy mint reference from account info
#[inline(always)]
unsafe fn get_mint_unchecked(account: &AccountInfo) -> &Mint {
    let mint_data_slice = account.borrow_data_unchecked();
    &*(mint_data_slice.as_ptr() as *const Mint)
}

/// Get token account reference with validation
#[inline(always)]
fn get_token_account(account: &AccountInfo) -> Result<&TokenAccount, ProgramError> {
    let account_data = unsafe { account.borrow_data_unchecked() };

    if !valid_token_account_data(account_data) {
        return Err(ProgramError::InvalidAccountData);
    }

    // SAFETY: We've validated the account data structure above
    unsafe { Ok(&*(account_data.as_ptr() as *const TokenAccount)) }
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

/// Build TransferChecked instruction data
#[inline(always)]
fn build_transfer_data(amount: u64, decimals: u8) -> [u8; 10] {
    let mut data = [0u8; 10];
    data[0] = TRANSFER_CHECKED_DISCM;
    data[1..9].copy_from_slice(&amount.to_le_bytes());
    data[9] = decimals;
    data
}

/// Build CloseAccount instruction data
#[inline(always)]
fn build_close_account_data() -> [u8; 1] {
    [CLOSE_ACCOUNT_DISCM]
}

/// Resolve rent from sysvar account or syscall
#[inline(always)]
fn resolve_rent(maybe_rent_info: Option<&AccountInfo>) -> Result<Rent, ProgramError> {
    match maybe_rent_info {
        Some(rent_account) => unsafe { Rent::from_account_info_unchecked(rent_account) }.cloned(),
        None => Rent::get(),
    }
}

/// Parse and validate the standard Recover account layout.
#[inline(always)]
pub fn parse_recover_accounts(
    accounts: &[AccountInfo],
) -> Result<RecoverNestedAccounts, ProgramError> {
    if accounts.len() < 7 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    // SAFETY: account len already checked
    unsafe {
        Ok(RecoverNestedAccounts {
            nested_associated_token_account: accounts.get_unchecked(0),
            nested_mint: accounts.get_unchecked(1),
            destination_associated_token_account: accounts.get_unchecked(2),
            owner_associated_token_account: accounts.get_unchecked(3),
            owner_mint: accounts.get_unchecked(4),
            wallet: accounts.get_unchecked(5),
            token_program: accounts.get_unchecked(6),
        })
    }
}

/// Parse and validate the standard Create account layout.
#[inline(always)]
pub fn parse_create_accounts(accounts: &[AccountInfo]) -> Result<CreateAccounts, ProgramError> {
    let rent_info = match accounts.len() {
        len if len >= 7 => Some(unsafe { accounts.get_unchecked(6) }),
        6 => None,
        _ => return Err(ProgramError::NotEnoughAccountKeys),
    };

    // SAFETY: account len already checked
    unsafe {
        Ok(CreateAccounts {
            payer: accounts.get_unchecked(0),
            associated_token_account_to_create: accounts.get_unchecked(1),
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
pub fn check_idempotent_account(
    associated_token_account: &AccountInfo,
    wallet: &AccountInfo,
    mint_account: &AccountInfo,
    token_program: &AccountInfo,
    idempotent: bool,
    program_id: &Pubkey,
) -> Result<bool, ProgramError> {
    if idempotent && associated_token_account.is_owned_by(token_program.key()) {
        let ata_state = get_token_account(associated_token_account)?;

        // validation is the point of CreateIdempotent,
        // so these checks should not be optimized out
        validate_token_account_owner(ata_state, wallet.key())?;
        validate_token_account_mint(ata_state, mint_account.key())?;

        // Also validate that the account is at the canonical ATA address
        // Prevents idempotent operations from succeeding with non-canonical addresses
        let (canonical_address, _bump) = derive_canoncial_ata_pda(
            wallet.key(),
            token_program.key(),
            mint_account.key(),
            program_id,
        );

        if canonical_address != *associated_token_account.key() {
            return Err(ProgramError::InvalidSeeds);
        }

        return Ok(true); // Account exists and is valid
    }
    Ok(false) // Need to create account
}

/// Create and initialize an ATA account with the given bump seed.
#[allow(clippy::too_many_arguments)]
#[inline(always)]
pub fn create_and_initialize_ata(
    payer: &AccountInfo,
    associated_token_account: &AccountInfo,
    wallet: &AccountInfo,
    mint_account: &AccountInfo,
    // if an account isn't owned by the system program,
    // the create_pda_account call will fail anyway when trying to allocate/assign
    _system_program: &AccountInfo,
    token_program: &AccountInfo,
    maybe_rent_info: Option<&AccountInfo>,
    bump: u8,
    maybe_token_account_len: Option<usize>,
) -> ProgramResult {
    // Use provided account length if available, otherwise calculate based on token program
    let space = match maybe_token_account_len {
        Some(len) => len,
        None => {
            if is_spl_token_program(token_program.key()) {
                TokenAccount::LEN // 165 bytes for SPL Token
            } else if is_token_2022_program(token_program.key()) {
                // For Token-2022, check if mint has extensions
                if token_2022_mint_has_extensions(mint_account) {
                    // Mint has extensions, use CPI to get correct account size
                    get_token_2022_account_size_via_cpi(mint_account, token_program)?
                } else {
                    // No extensions, use base account (165 bytes) + ImmutableOwner (5 bytes) = 170 bytes
                    TokenAccount::LEN + 5
                }
            } else {
                // Unknown token program, default to 165 bytes
                TokenAccount::LEN
            }
        }
    };

    let seeds: &[&[u8]] = &[
        wallet.key().as_ref(),
        token_program.key().as_ref(),
        mint_account.key().as_ref(),
        &[bump],
    ];

    // Use Rent passed in accounts if supplied to avoid syscall
    let rent = resolve_rent(maybe_rent_info)?;
    create_pda_account(
        payer,
        &rent,
        space,
        token_program.key(),
        associated_token_account,
        seeds,
    )?;

    // Initialize ImmutableOwner extension for non-SPL Token programs.
    // Note this requires that any future Token programs support ImmutableOwner in order
    // for this p-ata program to support them.
    if !is_spl_token_program(token_program.key()) {
        let initialize_immutable_owner_data = build_initialize_immutable_owner_data();

        let initialize_immutable_owner_metas = &[AccountMeta {
            pubkey: associated_token_account.key(),
            is_writable: true,
            is_signer: false,
        }];

        let init_immutable_owner_ix = Instruction {
            program_id: token_program.key(),
            accounts: initialize_immutable_owner_metas,
            data: &initialize_immutable_owner_data,
        };

        invoke(&init_immutable_owner_ix, &[associated_token_account])?;
    }

    // Initialize account using InitializeAccount3 (2 accounts + owner in instruction data)
    let initialize_account_instr_data = build_initialize_account3_data(wallet.key());

    let initialize_account_metas = &[
        AccountMeta {
            pubkey: associated_token_account.key(),
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
        program_id: token_program.key(),
        accounts: initialize_account_metas,
        data: &initialize_account_instr_data,
    };

    invoke(&init_ix, &[associated_token_account, mint_account])?;

    Ok(())
}

/// Check if a given address is off-curve using the sol_curve_validate_point syscall.
/// Returns true if the address is off-curve (invalid as an Ed25519 point).
#[inline(always)]
fn is_off_curve(_address: &Pubkey) -> bool {
    #[cfg(target_os = "solana")]
    {
        const ED25519_CURVE_ID: u64 = 0;

        let mut result: u8 = 0;
        let point_addr = _address.as_ref().as_ptr();
        let result_addr = &mut result as *mut u8;

        // SAFETY: We're passing valid pointers to the syscall
        let syscall_result =
            unsafe { sol_curve_validate_point(ED25519_CURVE_ID, point_addr, result_addr) };

        // If syscall fails (non-zero return), assume off-curve for safety
        // If syscall succeeds (zero return), check the result:
        // - result == 1 means point is ON the curve
        // - result == 0 means point is OFF the curve
        syscall_result != 0 || result == 0
    }
    #[cfg(not(target_os = "solana"))]
    {
        // TODO: hand-roll something here, just for testing
        false
    }
}

/// Given a hint bump, return a guaranteed canonical bump.
/// The bump is not guaranteed to be off-curve, but it is guaranteed that
/// no better (greater) off-curve bump exists. This prevents callers
/// from creating non-canonical associated token accounts by passing in
/// sub-optimal bumps.
#[inline(always)]
fn ensure_no_better_canonical_address_and_bump(
    seeds: &[&[u8]; 3],
    program_id: &Pubkey,
    hint_bump: u8,
) -> (Option<Pubkey>, u8) {
    // note: this complexity (rather than just find_program_address) is because we are en route
    // to benching a solution that verifies there is no better canonical address, but does not
    // guarantee that the found address, IF it matches the provided bump, is off-curve.
    // This saves an AVERAGE of 76% compute, with the only downside being that the client
    // will encounter an error if the bump they provide is not off-curve.
    // However, this is not implemented quite yet, since we will bench actual savings
    // in a later commit vs this one.
    let mut better_bump = 255;
    while better_bump > hint_bump {
        let maybe_better_address = derive_address::<3>(seeds, better_bump, program_id);
        if is_off_curve(&maybe_better_address) {
            return (Some(maybe_better_address), better_bump);
        }
        better_bump -= 1;
    }
    (None, hint_bump)
}

/// Accounts:
/// [0] payer
/// [1] associated_token_account_to_create
/// [2] wallet
/// [3] mint
/// [4] system_program
/// [5] token_program
/// [6] rent_sysvar
///
/// For Token-2022 accounts, create the account with the correct size
/// and call InitializeImmutableOwner followed by InitializeAccount3.
pub fn process_create_associated_token_account(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    idempotent: bool,
    maybe_bump: Option<u8>,
    maybe_token_account_len: Option<usize>,
) -> ProgramResult {
    let create_accounts = parse_create_accounts(accounts)?;

    // Check if account already exists (idempotent path)
    if check_idempotent_account(
        create_accounts.associated_token_account_to_create,
        create_accounts.wallet,
        create_accounts.mint,
        create_accounts.token_program,
        idempotent,
        program_id,
    )? {
        return Ok(());
    }

    // reenable this if determining that it is better than ata validation
    // if !create_accounts.payer.is_signer() {
    //     return Err(ProgramError::MissingRequiredSignature);
    // }

    let (verified_associated_token_account_to_create, bump) = match maybe_bump {
        Some(provided_bump) => ensure_no_better_canonical_address_and_bump(
            &[
                create_accounts.wallet.key().as_ref(),
                create_accounts.token_program.key().as_ref(),
                create_accounts.mint.key().as_ref(),
            ],
            program_id,
            provided_bump,
        ),
        None => {
            let (address, computed_bump) = derive_canoncial_ata_pda(
                create_accounts.wallet.key(),
                create_accounts.token_program.key(),
                create_accounts.mint.key(),
                program_id,
            );
            (Some(address), computed_bump)
        }
    };

    // Error if there is a canonical address with a better bump than provided
    if verified_associated_token_account_to_create
        .is_some_and(|address| &address != create_accounts.associated_token_account_to_create.key())
    {
        msg!("Canonical address does not match provided address. Use correct owner and optimal bump.");
        return Err(ProgramError::InvalidInstructionData);
    }

    create_and_initialize_ata(
        create_accounts.payer,
        create_accounts.associated_token_account_to_create,
        create_accounts.wallet,
        create_accounts.mint,
        create_accounts.system_program,
        create_accounts.token_program,
        create_accounts.rent_sysvar,
        bump,
        maybe_token_account_len,
    )
}

/// Accounts:
/// [0] nested_associated_token_account
/// [1] nested_mint
/// [2] destination_associated_token_account
/// [3] owner_associated_token_account
/// [4] owner_mint
/// [5] wallet
/// [6] token_program
/// [7..] multisig signer accounts
pub fn process_recover_nested(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let recover_accounts = parse_recover_accounts(accounts)?;

    // Verify owner address derivation
    let (owner_associated_token_address, bump) = derive_canoncial_ata_pda(
        recover_accounts.wallet.key(),
        recover_accounts.token_program.key(),
        recover_accounts.owner_mint.key(),
        program_id,
    );

    if owner_associated_token_address != *recover_accounts.owner_associated_token_account.key() {
        msg!("Error: Owner associated address does not match seed derivation");
        return Err(ProgramError::InvalidSeeds);
    }

    // Verify nested address derivation
    let (nested_associated_token_address, _) = derive_canoncial_ata_pda(
        recover_accounts.owner_associated_token_account.key(),
        recover_accounts.token_program.key(),
        recover_accounts.nested_mint.key(),
        program_id,
    );
    if nested_associated_token_address != *recover_accounts.nested_associated_token_account.key() {
        msg!("Error: Nested associated address does not match seed derivation");
        return Err(ProgramError::InvalidSeeds);
    }

    // Verify destination address derivation
    let (destination_associated_token_address, _) = derive_canoncial_ata_pda(
        recover_accounts.wallet.key(),
        recover_accounts.token_program.key(),
        recover_accounts.nested_mint.key(),
        program_id,
    );
    if destination_associated_token_address
        != *recover_accounts.destination_associated_token_account.key()
    {
        msg!("Error: Destination associated address does not match seed derivation");
        return Err(ProgramError::InvalidSeeds);
    }

    // Handle multisig case
    if !recover_accounts.wallet.is_signer() {
        // Multisig case: must be token-program owned
        if !recover_accounts
            .wallet
            .is_owned_by(recover_accounts.token_program.key())
        {
            return Err(ProgramError::MissingRequiredSignature);
        }

        // Load and validate multisig state
        let wallet_data_slice = unsafe { recover_accounts.wallet.borrow_data_unchecked() };
        let multisig_state: &Multisig =
            unsafe { spl_token_interface::state::load::<Multisig>(wallet_data_slice)? };

        let signer_infos = &accounts[7..];

        let mut num_signers = 0;
        let mut matched = [false; MAX_SIGNERS as usize];

        for signer in signer_infos.iter() {
            for (position, key) in multisig_state.signers[0..multisig_state.n as usize]
                .iter()
                .enumerate()
            {
                if key == signer.key() && !matched[position] {
                    if !signer.is_signer() {
                        return Err(ProgramError::MissingRequiredSignature);
                    }
                    matched[position] = true;
                    num_signers += 1;
                }
            }
        }

        if num_signers < multisig_state.m {
            return Err(ProgramError::MissingRequiredSignature);
        }
    }

    let amount_to_recover =
        get_token_account(recover_accounts.nested_associated_token_account)?.amount();

    let nested_mint_decimals = unsafe { get_mint_unchecked(recover_accounts.nested_mint).decimals };

    let transfer_data = build_transfer_data(amount_to_recover, nested_mint_decimals);

    let transfer_metas = &[
        AccountMeta {
            pubkey: recover_accounts.nested_associated_token_account.key(),
            is_writable: true,
            is_signer: false,
        },
        AccountMeta {
            pubkey: recover_accounts.nested_mint.key(),
            is_writable: false,
            is_signer: false,
        },
        AccountMeta {
            pubkey: recover_accounts.destination_associated_token_account.key(),
            is_writable: true,
            is_signer: false,
        },
        AccountMeta {
            pubkey: recover_accounts.owner_associated_token_account.key(),
            is_writable: false,
            is_signer: true,
        },
    ];

    let ix_transfer = Instruction {
        program_id: recover_accounts.token_program.key(),
        accounts: transfer_metas,
        data: &transfer_data,
    };

    let pda_seeds_raw: &[&[u8]] = &[
        recover_accounts.wallet.key().as_ref(),
        recover_accounts.token_program.key().as_ref(),
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
            recover_accounts.nested_associated_token_account,
            recover_accounts.nested_mint,
            recover_accounts.destination_associated_token_account,
            recover_accounts.owner_associated_token_account,
        ],
        &[pda_signer.clone()],
    )?;

    let close_data = build_close_account_data();

    let close_metas = &[
        AccountMeta {
            pubkey: recover_accounts.nested_associated_token_account.key(),
            is_writable: true,
            is_signer: false,
        },
        AccountMeta {
            pubkey: recover_accounts.wallet.key(),
            is_writable: true,
            is_signer: false,
        },
        AccountMeta {
            pubkey: recover_accounts.owner_associated_token_account.key(),
            is_writable: false,
            is_signer: true,
        },
        AccountMeta {
            pubkey: recover_accounts.token_program.key(),
            is_writable: false,
            is_signer: false,
        },
    ];

    let ix_close = Instruction {
        program_id: recover_accounts.token_program.key(),
        accounts: close_metas,
        data: &close_data,
    };

    invoke_signed(
        &ix_close,
        &[
            recover_accounts.nested_associated_token_account,
            recover_accounts.wallet,
            recover_accounts.owner_associated_token_account,
            recover_accounts.token_program,
        ],
        &[pda_signer],
    )?;
    Ok(())
}
