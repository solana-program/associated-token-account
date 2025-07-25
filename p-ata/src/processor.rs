#![allow(unexpected_cfgs)]

use {
    crate::account::create_pda_account,
    core::mem::MaybeUninit,
    pinocchio::{
        account_info::AccountInfo,
        cpi,
        instruction::{AccountMeta, Instruction, Seed, Signer},
        msg,
        program::invoke_signed,
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
pub const GET_ACCOUNT_DATA_SIZE_DISCM: u8 = 21;
pub const MINT_BASE_SIZE: usize = 82;
pub const MINT_WITH_TYPE_SIZE: usize = MINT_BASE_SIZE + 1;

// Token-2022 AccountType::Account discriminator value
const ACCOUNTTYPE_ACCOUNT: u8 = 2;

pub const INITIALIZE_IMMUTABLE_OWNER_DATA: [u8; 1] = [INITIALIZE_IMMUTABLE_OWNER_DISCM];
pub const CLOSE_ACCOUNT_DATA: [u8; 1] = [CLOSE_ACCOUNT_DISCM];

// Compile-time verifications
const _: () = assert!(
    TokenAccount::LEN == 165,
    "TokenAccount size changed unexpectedly"
);
const _: () = assert!(Multisig::LEN == 355, "Multisig size changed unexpectedly");

/// Parsed ATA accounts for create operations
pub struct CreateAccounts<'a> {
    pub payer: &'a AccountInfo,
    pub associated_token_account_to_create: &'a AccountInfo,
    pub wallet: &'a AccountInfo,
    pub mint: &'a AccountInfo,
    #[allow(dead_code)]
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
pub(crate) fn derive_canonical_ata_pda(
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
pub(crate) fn is_spl_token_program(program_id: &Pubkey) -> bool {
    const SPL_TOKEN_PROGRAM_ID: Pubkey =
        pinocchio_pubkey::pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
    // SAFETY: Safe because we are comparing the pointers of the
    // program_id and SPL_TOKEN_PROGRAM_ID, which are both const Pubkeys
    unsafe {
        core::slice::from_raw_parts(program_id.as_ref().as_ptr(), 32)
            == core::slice::from_raw_parts(SPL_TOKEN_PROGRAM_ID.as_ref().as_ptr(), 32)
    }
}

/// Calculate token account size by parsing mint extension data inline.
/// This avoids the expensive CPI call to GetAccountDataSize for most cases.
/// Returns None if unknown/variable-length extensions are found.
#[inline(always)]
pub(crate) fn account_size_from_mint_inline(mint_data: &[u8]) -> Option<usize> {
    const TOKEN_ACCOUNT_LEN: usize = 165;
    const ACCOUNT_TYPE_OFFSET: usize = 165; // Account type discriminator position

    // Check if this mint has extensions (must be larger than base + account type)
    if mint_data.len() <= ACCOUNT_TYPE_OFFSET {
        // No mint extensions, but Token-2022 ATAs still need account type discriminator + ImmutableOwner
        return Some(TOKEN_ACCOUNT_LEN + 1 + 4); // +1 for account type, +4 for ImmutableOwner TLV
    }

    let mut account_extensions_size = 0usize;
    let mut cursor = ACCOUNT_TYPE_OFFSET + 1; // Start after account type discriminator

    while cursor + 4 <= mint_data.len() {
        // Parse TLV header manually: 2 bytes type + 2 bytes length
        let extension_type = u16::from_le_bytes([mint_data[cursor], mint_data[cursor + 1]]);
        let length = u16::from_le_bytes([mint_data[cursor + 2], mint_data[cursor + 3]]);

        if extension_type == 0 {
            break;
        } // TypeEnd/Uninitialized

        // Based on token-2022's get_required_init_account_extensions:
        // Only specific mint extensions require account-side data
        #[allow(clippy::manual_range_patterns)]
        #[allow(clippy::identity_op)]
        match extension_type {
            1 => {
                // TransferFeeConfig → requires TransferFeeAmount (8 bytes + 4 TLV overhead)
                account_extensions_size += 4 + 8; // TLV overhead + data
            }
            9 => {
                // NonTransferable → requires NonTransferableAccount (0 bytes)
                // (ImmutableOwner is already accounted for globally)
                account_extensions_size += 4 + 0; // NonTransferableAccount: TLV overhead + data
            }
            14 => {
                // TransferHook → requires TransferHookAccount (1 byte + 4 TLV overhead)
                account_extensions_size += 4 + 1; // TLV overhead + data
            }
            26 => {
                // Pausable → requires PausableAccount (0 bytes + 4 TLV overhead)
                account_extensions_size += 4 + 0; // TLV overhead + data
            }
            // Known simple mint-only extensions (don't affect account size)
            2 | 3 | 6 | 7 | 8 | 10 | 11 | 12 | 13 | 15 | 17 | 18 | 20 | 24 | 25 | 27 => {
                // These are simple mint extensions that don't require account data
            }
            // Complex extensions - fall back to CPI for safety
            4 | 5 | 16 => {
                // ConfidentialTransferMint, ConfidentialTransferAccount, ConfidentialTransferFeeConfig
                return None; // Complex confidential transfer extensions, fall back to CPI
            }
            19 => {
                // TokenMetadata is variable-length (proven by sized() -> false)
                return None; // Variable-length, fall back to CPI
            }
            21 | 22 | 23 => {
                // TokenGroup, GroupMemberPointer, TokenGroupMember
                return None; // Complex group extensions, fall back to CPI
            }
            // Unknown or variable-length extensions → fall back to CPI
            _ => return None,
        }
        cursor += 4 + length as usize;
    }

    // For Token-2022 ATAs, we ALWAYS include:
    // - Account type discriminator (+1 byte)
    // - ImmutableOwner extension (+4 bytes TLV overhead + 0 bytes data = 4 bytes)
    // - Any additional extensions derived from mint extensions
    Some(TOKEN_ACCOUNT_LEN + 1 + 4 + account_extensions_size)
}

/// Get the required account size for a mint using inline parsing first,
/// falling back to GetAccountDataSize CPI only when necessary.
/// Returns the account size in bytes.
#[inline(always)]
pub(crate) fn get_token_account_size(
    mint_account: &AccountInfo,
    token_program: &AccountInfo,
) -> Result<usize, ProgramError> {
    if is_spl_token_program(token_program.key()) {
        return Ok(TokenAccount::LEN);
    }

    // Token mint has no extensions other than ImmutableOwner
    // (this assumes any future token program has ImmutableOwner)
    if !token_mint_has_extensions(mint_account) {
        return Ok(TokenAccount::LEN + 5);
    }

    // Try inline parsing first
    let mint_data = unsafe { mint_account.borrow_data_unchecked() };
    if let Some(size) = account_size_from_mint_inline(mint_data) {
        return Ok(size);
    }

    // Fallback to CPI for unknown/variable-length extensions
    // ImmutableOwner extension is required for Token-2022 Associated Token Accounts
    let instruction_data = [GET_ACCOUNT_DATA_SIZE_DISCM, 7u8, 0u8]; // [7, 0] = ImmutableOwner as u16

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

    cpi::invoke(&get_size_ix, &[mint_account])?;
    let return_data = cpi::get_return_data().ok_or(ProgramError::InvalidAccountData)?;

    // `try_into` as this could be an unknown token program;
    // it must error if it doesn't give us [u8; 8]
    Ok(u64::from_le_bytes(
        return_data
            .as_slice()
            .try_into()
            .map_err(|_| ProgramError::InvalidAccountData)?,
    ) as usize)
}

/// Check if a Token-2022 mint has extensions by examining its data length
#[inline(always)]
pub(crate) fn token_mint_has_extensions(mint_account: &AccountInfo) -> bool {
    // If mint data is larger than base + type, it has extensions
    mint_account.data_len() > MINT_WITH_TYPE_SIZE
}

/// Check if account data represents an initialized token account.
/// Mimics p-token's is_initialized_account check.
///
/// Safety: caller must ensure account_data.len() >= 109.
#[inline(always)]
pub(crate) unsafe fn is_initialized_account(account_data: &[u8]) -> bool {
    // Token account state is at offset 108 (after mint, owner, amount, delegate fields)
    // State: 0 = Uninitialized, 1 = Initialized, 2 = Frozen
    account_data[108] != 0
}

/// Validate that account data represents a valid token account.
/// Replicates Token-2022's GenericTokenAccount::valid_account_data checks.
#[inline(always)]
pub(crate) fn valid_token_account_data(account_data: &[u8]) -> bool {
    // Regular Token account: exact length match and initialized
    if account_data.len() == TokenAccount::LEN {
        // SAFETY: TokenAccount::LEN is compile-ensured to be == 165
        return unsafe { is_initialized_account(account_data) };
    }

    // Token-2022's GenericTokenAccount::valid_account_data assumes Multisig
    // if account_data length is Multisig::LEN. Collisions are prevented by
    // adding a byte if a token account happens to have the same length as
    // Multisig::LEN.
    if account_data.len() > TokenAccount::LEN {
        if account_data.len() == Multisig::LEN {
            return false;
        }
        // SAFETY: TokenAccount::LEN is compile-ensured to be == 165, and in
        // this branch account_data.len > TokenAccount::LEN
        if unsafe { is_initialized_account(account_data) } {
            return account_data[TokenAccount::LEN] == ACCOUNTTYPE_ACCOUNT;
        }
    }

    false
}

/// Get zero-copy mint reference from account info
#[inline(always)]
pub(crate) unsafe fn get_mint_unchecked(account: &AccountInfo) -> &Mint {
    let mint_data_slice = account.borrow_data_unchecked();
    &*(mint_data_slice.as_ptr() as *const Mint)
}

/// Get token account reference with validation
#[inline(always)]
pub(crate) fn get_token_account(account: &AccountInfo) -> Result<&TokenAccount, ProgramError> {
    let account_data = unsafe { account.borrow_data_unchecked() };

    if !valid_token_account_data(account_data) {
        return Err(ProgramError::InvalidAccountData);
    }

    // SAFETY: We've validated the account data structure above
    unsafe { Ok(&*(account_data.as_ptr() as *const TokenAccount)) }
}

/// Validate token account owner matches expected owner
#[inline(always)]
pub(crate) fn validate_token_account_owner(
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
pub(crate) fn validate_token_account_mint(
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
pub(crate) fn build_initialize_account3_data(owner: &Pubkey) -> [u8; 33] {
    let mut data = MaybeUninit::<[u8; 33]>::uninit();
    let data_ptr = data.as_mut_ptr() as *mut u8;
    // SAFETY: We initialize all 33 bytes before calling assume_init()
    unsafe {
        *data_ptr = INITIALIZE_ACCOUNT_3_DISCM;
        core::ptr::copy_nonoverlapping(owner.as_ref().as_ptr(), data_ptr.add(1), 32);
        data.assume_init()
    }
}

/// Build TransferChecked instruction data
#[inline(always)]
pub(crate) fn build_transfer_data(amount: u64, decimals: u8) -> [u8; 10] {
    let mut data = MaybeUninit::<[u8; 10]>::uninit();
    let data_ptr = data.as_mut_ptr() as *mut u8;
    // SAFETY: We initialize all 10 bytes before calling assume_init()
    unsafe {
        *data_ptr = TRANSFER_CHECKED_DISCM;
        core::ptr::copy_nonoverlapping(amount.to_le_bytes().as_ptr(), data_ptr.add(1), 8);
        *data_ptr.add(9) = decimals;
        data.assume_init()
    }
}

/// Parse and validate the standard Recover account layout.
#[inline(always)]
pub(crate) fn parse_recover_accounts(
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
pub(crate) fn parse_create_accounts(
    accounts: &[AccountInfo],
) -> Result<CreateAccounts, ProgramError> {
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
pub(crate) fn check_idempotent_account(
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
        let (canonical_address, _bump) = derive_canonical_ata_pda(
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

/// Compute the required space (in bytes) for the associated token account.
#[inline(always)]
pub(crate) fn resolve_token_account_space(
    token_program: &AccountInfo,
    mint_account: &AccountInfo,
    maybe_token_account_len: Option<usize>,
) -> Result<usize, ProgramError> {
    match maybe_token_account_len {
        Some(len) => Ok(len),
        None => get_token_account_size(mint_account, token_program),
    }
}

/// Create and initialize an ATA account with the given bump seed.
#[allow(clippy::too_many_arguments)]
#[inline(always)]
pub(crate) fn create_and_initialize_ata(
    payer: &AccountInfo,
    associated_token_account: &AccountInfo,
    wallet: &AccountInfo,
    mint_account: &AccountInfo,
    token_program: &AccountInfo,
    rent: &Rent,
    bump: u8,
    space: usize,
) -> ProgramResult {
    let seeds: &[&[u8]] = &[
        wallet.key().as_ref(),
        token_program.key().as_ref(),
        mint_account.key().as_ref(),
        &[bump],
    ];

    create_pda_account(
        payer,
        rent,
        space,
        token_program.key(),
        associated_token_account,
        seeds,
    )?;

    // Initialize ImmutableOwner for non-SPL Token programs (future compatible)
    if !is_spl_token_program(token_program.key()) {
        let initialize_immutable_owner_metas = &[AccountMeta {
            pubkey: associated_token_account.key(),
            is_writable: true,
            is_signer: false,
        }];
        let init_immutable_owner_ix = Instruction {
            program_id: token_program.key(),
            accounts: initialize_immutable_owner_metas,
            data: &INITIALIZE_IMMUTABLE_OWNER_DATA,
        };
        cpi::invoke(&init_immutable_owner_ix, &[associated_token_account])?;
    }

    // Initialize account via InitializeAccount3.
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

    cpi::invoke(&init_ix, &[associated_token_account, mint_account])?;
    Ok(())
}

/// Check if a given address is off-curve using the sol_curve_validate_point syscall.
/// Returns true if the address is off-curve (invalid as an Ed25519 point).
#[inline(always)]
pub(crate) fn is_off_curve(_address: &Pubkey) -> bool {
    #[cfg(target_os = "solana")]
    {
        const ED25519_CURVE_ID: u64 = 0;

        let point_addr = _address.as_ref().as_ptr();

        // SAFETY: We're passing valid pointers to the syscall
        // The syscall directly returns the validation result:
        // - 0 means point is ON the curve (valid)
        // - 1 means point is OFF the curve (invalid)
        // - any other value means error
        let syscall_result = unsafe {
            sol_curve_validate_point(ED25519_CURVE_ID, point_addr, core::ptr::null_mut())
        };

        syscall_result == 1
    }
    #[cfg(all(not(target_os = "solana"), test))]
    {
        // Host build (tests, benches): replicate the on-chain `sol_curve_validate_point` logic
        // using curve25519-dalek. A pubkey is "off-curve" if it cannot be decompressed into
        // an Edwards point **or** it decomposes to a small-order point
        // (matches Solana’s runtime rules).

        use curve25519_dalek::edwards::CompressedEdwardsY;

        let mut bytes = MaybeUninit::<[u8; 32]>::uninit();
        let bytes_ptr = bytes.as_mut_ptr() as *mut u8;
        // SAFETY: We initialize all 32 bytes before calling assume_init()
        let bytes = unsafe {
            core::ptr::copy_nonoverlapping(_address.as_ref().as_ptr(), bytes_ptr, 32);
            bytes.assume_init()
        };
        let compressed = CompressedEdwardsY(bytes);

        match compressed.decompress() {
            None => true,                    // invalid encoding → off-curve
            Some(pt) => pt.is_small_order(), // small-order = off-curve, otherwise on-curve
        }
    }
    #[cfg(all(not(target_os = "solana"), not(test)))]
    {
        false
    }
}

/// Given a hint bump, return a guaranteed canonical bump.
/// The bump is not guaranteed to be off-curve, but it is guaranteed that
/// no better (greater) off-curve bump exists. This prevents callers
/// from creating non-canonical associated token accounts by passing in
/// sub-optimal bumps.
#[inline(always)]
pub(crate) fn ensure_no_better_canonical_address_and_bump(
    seeds: &[&[u8]; 3],
    program_id: &Pubkey,
    hint_bump: u8,
) -> (Option<Pubkey>, u8) {
    // Optimization: Only verify no better bump exists, don't require hint_bump to be off-curve
    // This saves significant compute units while still preventing non-canonical addresses
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
pub(crate) fn process_create_associated_token_account(
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
            let (address, computed_bump) = derive_canonical_ata_pda(
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
        msg!("Error: Canonical address does not match provided address. Use correct owner and optimal bump.");
        return Err(ProgramError::InvalidInstructionData);
    }

    let space = resolve_token_account_space(
        create_accounts.token_program,
        create_accounts.mint,
        maybe_token_account_len,
    )?;

    match create_accounts.rent_sysvar {
        Some(rent_account) => {
            let rent_ref = unsafe { Rent::from_account_info_unchecked(rent_account) }?;
            create_and_initialize_ata(
                create_accounts.payer,
                create_accounts.associated_token_account_to_create,
                create_accounts.wallet,
                create_accounts.mint,
                create_accounts.token_program,
                rent_ref,
                bump,
                space,
            )
        }
        None => {
            let rent = Rent::get()?;
            create_and_initialize_ata(
                create_accounts.payer,
                create_accounts.associated_token_account_to_create,
                create_accounts.wallet,
                create_accounts.mint,
                create_accounts.token_program,
                &rent,
                bump,
                space,
            )
        }
    }
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
pub(crate) fn process_recover_nested(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let recover_accounts = parse_recover_accounts(accounts)?;

    let (owner_associated_token_address, bump) = derive_canonical_ata_pda(
        recover_accounts.wallet.key(),
        recover_accounts.token_program.key(),
        recover_accounts.owner_mint.key(),
        program_id,
    );

    if owner_associated_token_address != *recover_accounts.owner_associated_token_account.key() {
        msg!("Error: Owner associated address does not match seed derivation");
        return Err(ProgramError::InvalidSeeds);
    }

    let (nested_associated_token_address, _) = derive_canonical_ata_pda(
        recover_accounts.owner_associated_token_account.key(),
        recover_accounts.token_program.key(),
        recover_accounts.nested_mint.key(),
        program_id,
    );
    if nested_associated_token_address != *recover_accounts.nested_associated_token_account.key() {
        msg!("Error: Nested associated address does not match seed derivation");
        return Err(ProgramError::InvalidSeeds);
    }

    let (destination_associated_token_address, _) = derive_canonical_ata_pda(
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
    ];

    let ix_close = Instruction {
        program_id: recover_accounts.token_program.key(),
        accounts: close_metas,
        data: &CLOSE_ACCOUNT_DATA,
    };

    invoke_signed(
        &ix_close,
        &[
            recover_accounts.nested_associated_token_account,
            recover_accounts.wallet,
            recover_accounts.owner_associated_token_account,
        ],
        &[pda_signer],
    )?;
    Ok(())
}
