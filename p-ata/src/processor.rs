//! Core ATA creation and validation logic.
//!
//! This module handles the main Associated Token Account creation flow, including:
//! - Account parsing and validation
//! - Idempotent operation support
//! - Bump seed validation and canonicalization
//! - Account initialization for both Token and Token-2022 programs
//! - Cross-program compatibility checks
//!
//! The processor supports optimization hints (bump seeds and account lengths) while
//! maintaining strict security guarantees through canonical address validation.

#![allow(unexpected_cfgs)]

use {
    crate::{
        account::create_pda_account,
        size::{get_token_account_size, MINT_BASE_SIZE},
    },
    core::mem::MaybeUninit,
    pinocchio::{
        account_info::AccountInfo,
        cpi,
        instruction::{AccountMeta, Instruction},
        program_error::ProgramError,
        pubkey::{find_program_address, Pubkey},
        sysvars::{rent::Rent, Sysvar},
        ProgramResult,
    },
    pinocchio_log::log,
    pinocchio_pubkey::derive_address,
    spl_token_interface::state::{
        account::Account as TokenAccount, mint::Mint, multisig::Multisig, Transmutable,
    },
};

#[cfg(target_os = "solana")]
use pinocchio::syscalls::sol_curve_validate_point;

pub const INITIALIZE_ACCOUNT_3_DISCRIMINATOR: u8 = 18;
pub const INITIALIZE_IMMUTABLE_OWNER_DISCRIMINATOR: u8 = 22;
pub const TRANSFER_CHECKED_DISCRIMINATOR: u8 = 12;

// Token-2022 AccountType::Account discriminator value
const ACCOUNTTYPE_ACCOUNT: u8 = 2;

pub const INITIALIZE_IMMUTABLE_OWNER_DATA: [u8; 1] = [INITIALIZE_IMMUTABLE_OWNER_DISCRIMINATOR];

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

/// Derive canonical ATA PDA from wallet, token program, and mint.
///
/// This is the least efficient derivation method, as it searches from bump
/// 255 downward until an off-curve address is found. Use only when no bump hint
/// is available in the instruction data.
///
/// An alternative was considered that used a loop with `derive_address` +
/// `is_off_curve` instead of `find_program_address`, but though it saved ~30 CUs
/// (1%) when bump happened to be `255`, it added more CUs on average.
///
/// ## Returns
///
/// `(address, bump)` - The canonical PDA address and its bump seed
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
    *program_id == SPL_TOKEN_PROGRAM_ID
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

/// Get mint reference from account info
#[inline(always)]
pub(crate) fn get_mint(account: &AccountInfo) -> Result<&Mint, ProgramError> {
    let mint_data_slice = account.try_borrow_data()?;
    if mint_data_slice.len() < MINT_BASE_SIZE {
        return Err(ProgramError::InvalidAccountData);
    }
    // SAFETY: We've validated the account length above
    Ok(unsafe { &*(mint_data_slice.as_ptr() as *const Mint) })
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
        *data_ptr = INITIALIZE_ACCOUNT_3_DISCRIMINATOR;
        core::ptr::copy_nonoverlapping(owner.as_ref().as_ptr(), data_ptr.add(1), 32);
        data.assume_init()
    }
}

/// Build TransferChecked instruction data
#[inline(always)]
pub(crate) fn build_transfer_checked_data(amount: u64, decimals: u8) -> [u8; 10] {
    let mut data = MaybeUninit::<[u8; 10]>::uninit();
    let data_ptr = data.as_mut_ptr() as *mut u8;
    // SAFETY: We initialize all 10 bytes before calling assume_init()
    unsafe {
        *data_ptr = TRANSFER_CHECKED_DISCRIMINATOR;
        core::ptr::copy_nonoverlapping(amount.to_le_bytes().as_ptr(), data_ptr.add(1), 8);
        *data_ptr.add(9) = decimals;
        data.assume_init()
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
///
/// This function validates that an existing ATA account:
/// 1. Is owned by the correct token program
/// 2. Has the correct owner (wallet)
/// 3. Has the correct mint
/// 4. Uses the canonical PDA address, even when `expected_bump` is provided
///
/// ## Returns
///
/// * `Ok(true)` - Account exists and is properly configured (safe to skip creation)
/// * `Ok(false)` - Account doesn't exist or is not token program owned (needs creation)  
/// * `Err(_)` - Account exists but has invalid configuration (error condition)
#[inline(always)]
pub(crate) fn check_idempotent_account(
    associated_token_account: &AccountInfo,
    wallet: &AccountInfo,
    mint_account: &AccountInfo,
    token_program: &AccountInfo,
    idempotent: bool,
    program_id: &Pubkey,
    expected_bump: Option<u8>,
) -> Result<bool, ProgramError> {
    if idempotent && associated_token_account.is_owned_by(token_program.key()) {
        let ata_state = get_token_account(associated_token_account)?;

        validate_token_account_owner(ata_state, wallet.key())?;
        validate_token_account_mint(ata_state, mint_account.key())?;

        match expected_bump {
            Some(bump) => {
                let seeds: &[&[u8]; 3] = &[
                    wallet.key().as_ref(),
                    token_program.key().as_ref(),
                    mint_account.key().as_ref(),
                ];

                // Check if a better canonical bump exists
                reject_if_better_valid_bump_exists(seeds, program_id, bump)?;

                let maybe_canonical_address = derive_address::<3>(seeds, Some(bump), program_id);

                // We must check that the actual derived address is off-curve,
                // since it will not fail downstream as in Create paths.
                // Potential problem if skipping this is demonstrated in
                // tests/bump/test_idemp_oncurve_attack.rs
                if !is_off_curve(&maybe_canonical_address)
                    || maybe_canonical_address != *associated_token_account.key()
                {
                    log!(
                        "Error: Provided `expected_bump` {} is on curve and non-canonical.",
                        bump
                    );
                    return Err(ProgramError::InvalidSeeds);
                }
            }
            None => {
                let (canonical_address, _bump) = derive_canonical_ata_pda(
                    wallet.key(),
                    token_program.key(),
                    mint_account.key(),
                    program_id,
                );

                if canonical_address != *associated_token_account.key() {
                    return Err(ProgramError::InvalidSeeds);
                }
            }
        }

        return Ok(true);
    }
    Ok(false)
}

/// Determine the required space (in bytes) for the associated token account.
///
/// Either uses a pre-computed account length hint or calls into the size
/// calculation logic to determine the space needed. For extended accounts,
/// passing in length can save significant compute units.
///
/// ## Arguments
///
/// * `known_token_account_len` - Optional pre-computed account length
///
/// ## Returns
///
/// The account size in bytes, or an error if size calculation fails.
#[inline(always)]
pub(crate) fn resolve_token_account_space(
    token_program: &AccountInfo,
    mint_account: &AccountInfo,
    known_token_account_len: Option<usize>,
) -> Result<usize, ProgramError> {
    match known_token_account_len {
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

/// Check if a given address is off-curve (not a valid Ed25519 curve point).
///
/// Returns `true` if the address is off-curve, `false` if on-curve.
///
/// - **On-chain (Solana)**: Uses `sol_curve_validate_point` syscall
/// - **Tests**: Uses curve25519-dalek to replicate on-chain behavior  
/// - **Other builds**: Returns `false`
#[inline(always)]
#[allow(unused_variables)]
pub(crate) fn is_off_curve(address: &Pubkey) -> bool {
    #[cfg(target_os = "solana")]
    {
        const ED25519_CURVE_ID: u64 = 0;

        let point_addr = address.as_ref().as_ptr();

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
        // an Edwards point.

        curve25519_dalek::edwards::CompressedEdwardsY(*address)
            .decompress()
            .is_none()
    }
    #[cfg(all(not(target_os = "solana"), not(test)))]
    {
        false
    }
}

/// Validate an expected bump and ensure no better canonical bump exists.
///
/// Given an expected bump, this function verifies that no higher bump value produces
/// a valid (off-curve) PDA address. This prevents creation of non-canonical ATAs
/// by rejecting sub-optimal bump seeds.
///
/// ## Arguments
///
/// * `seeds` - The PDA derivation seeds (wallet, token_program, mint)
/// * `program_id` - The program ID for PDA derivation  
/// * `expected_bump` - The bump value provided by the caller
///
/// ## Returns
///
/// * `None` - No better bump exists, the expected_bump is canonical
/// * `Some((address, bump))` - A better bump was found, returns the canonical address and bump
///
/// ## Security
///
/// This function is critical for preventing PDA canonicality attacks. It ensures
/// that only the highest valid bump can be used, maintaining deterministic
/// address derivation across all clients.
#[inline(always)]
pub(crate) fn reject_if_better_valid_bump_exists(
    seeds: &[&[u8]; 3],
    program_id: &Pubkey,
    expected_bump: u8,
) -> Result<(), ProgramError> {
    // Optimization: Only verify no better bump exists. Don't require expected_bump to
    // yield an off-curve address. This saves significant compute units while still
    // preventing non-canonical addresses.
    //
    // Caller must ensure that `expected_bump` is off-curve, either via downstream failure
    // (i.e. syscalls that will fail) or by calling `is_off_curve`.
    let mut better_bump = 255;
    while better_bump > expected_bump {
        let maybe_better_address = derive_address::<3>(seeds, Some(better_bump), program_id);
        if is_off_curve(&maybe_better_address) {
            log!("Canonical address does not match provided address. Canonical bump is {}, with address {}.", better_bump, &maybe_better_address);
            return Err(ProgramError::InvalidInstructionData);
        }
        better_bump -= 1;
    }
    Ok(())
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
    expected_bump: Option<u8>,
    known_token_account_len: Option<usize>,
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
        expected_bump,
    )? {
        return Ok(());
    }

    let bump = match expected_bump {
        Some(provided_bump) => {
            // Check if a better canonical bump exists
            reject_if_better_valid_bump_exists(
                &[
                    create_accounts.wallet.key().as_ref(),
                    create_accounts.token_program.key().as_ref(),
                    create_accounts.mint.key().as_ref(),
                ],
                program_id,
                provided_bump,
            )?;
            provided_bump
        }
        None => {
            let (_address, computed_bump) = derive_canonical_ata_pda(
                create_accounts.wallet.key(),
                create_accounts.token_program.key(),
                create_accounts.mint.key(),
                program_id,
            );
            computed_bump
        }
    };

    let space = resolve_token_account_space(
        create_accounts.token_program,
        create_accounts.mint,
        known_token_account_len,
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
