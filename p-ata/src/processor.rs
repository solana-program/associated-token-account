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
    crate::{account::create_pda_account, size::get_token_account_size},
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
/// Panics if account_data.len() < 109.
#[inline(always)]
pub(crate) fn is_initialized_account(account_data: &[u8]) -> bool {
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
        return is_initialized_account(account_data);
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
        if is_initialized_account(account_data) {
            return account_data[TokenAccount::LEN] == ACCOUNTTYPE_ACCOUNT;
        }
    }

    false
}

/// Get mint reference from account info
#[inline(always)]
pub(crate) fn get_decimals_from_mint(account: &AccountInfo) -> Result<u8, ProgramError> {
    let mint_data_slice = account.try_borrow_data()?;
    match unsafe { spl_token_interface::state::load_unchecked::<Mint>(&mint_data_slice) } {
        Ok(mint) => Ok(mint.decimals),
        Err(err) => {
            const MINT_BASE_SIZE: usize = core::mem::size_of::<Mint>();
            log!(
                "Error: Mint account data too small. Expected at least {} bytes, found {} bytes",
                MINT_BASE_SIZE,
                mint_data_slice.len()
            );
            Err(err)
        }
    }
}

/// Get token account reference with validation. Fails if a mutable borrow
/// of the account has occurred.
#[inline(always)]
pub(crate) fn load_token_account(account: &AccountInfo) -> Result<&TokenAccount, ProgramError> {
    let account_data = account.try_borrow_data()?;
    if !valid_token_account_data(&account_data) {
        return Err(ProgramError::InvalidAccountData);
    }
    // SAFETY: We've validated the account data structure above
    unsafe { Ok(&*(account_data.as_ptr() as *const TokenAccount)) }
}

/// Get token account reference with validation.
/// SAFETY: Caller must ensure no mutable borrows of `account`.
#[inline(always)]
pub(crate) unsafe fn load_token_account_unchecked(
    account: &AccountInfo,
) -> Result<&TokenAccount, ProgramError> {
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
        log!(
            "Error: Token account owner mismatch. Expected: {}, Found: {}",
            expected_owner,
            &account.owner
        );
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
        log!(
            "Error: Token account mint mismatch. Expected: {}, Found: {}",
            expected_mint,
            &account.mint
        );
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
    let [payer, associated_token_account_to_create, wallet, mint, system_program, token_program, maybe_rent_sysvar @ ..] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    Ok(CreateAccounts {
        payer,
        associated_token_account_to_create,
        wallet,
        mint,
        system_program,
        token_program,
        rent_sysvar: maybe_rent_sysvar.first(),
    })
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
///
/// SAFETY: Caller must ensure no mutable borrows of `associated_token_account` have occurred.
#[inline(always)]
pub(crate) unsafe fn check_idempotent_account(
    associated_token_account: &AccountInfo,
    wallet: &AccountInfo,
    mint_account: &AccountInfo,
    token_program: &AccountInfo,
    program_id: &Pubkey,
    expected_bump: Option<u8>,
) -> Result<bool, ProgramError> {
    if associated_token_account.is_owned_by(token_program.key()) {
        // SAFETY: no mutable borrows of the associated_token_account have occurred in this
        // function. Caller ensures that none have occurred in caller scope.
        let ata_state = unsafe { load_token_account_unchecked(associated_token_account)? };

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
                if !is_off_curve(&maybe_canonical_address) {
                    log!(
                        "Error: Invalid bump: bump {} results in on curve address.",
                        bump,
                    );
                    return Err(ProgramError::InvalidSeeds);
                }
                if maybe_canonical_address != *associated_token_account.key() {
                    log!(
                        "Error: Address mismatch: bump {} derives address which does not match provided associated token account address. Expected: {}, Found: {}",
                        bump,
                        &maybe_canonical_address,
                        associated_token_account.key()
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
                    log!(
                        "Error: Address mismatch: derived associated token address does not match provided address. Expected: {}, Found: {}",
                        &canonical_address,
                        associated_token_account.key()
                    );
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
    let bump_slice = &[bump];
    let seeds = pinocchio::seeds!(
        wallet.key().as_ref(),
        token_program.key().as_ref(),
        mint_account.key().as_ref(),
        bump_slice
    );

    create_pda_account(
        payer,
        rent,
        space,
        token_program.key(),
        associated_token_account,
        &seeds,
    )?;

    // Initialize ImmutableOwner for non-SPL Token programs (future compatible)
    if !is_spl_token_program(token_program.key()) {
        let initialize_immutable_owner_metas =
            &[AccountMeta::writable(associated_token_account.key())];
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
        AccountMeta::writable(associated_token_account.key()),
        AccountMeta::readonly(mint_account.key()),
    ];
    let init_ix = Instruction {
        program_id: token_program.key(),
        accounts: initialize_account_metas,
        data: &initialize_account_instr_data,
    };

    cpi::invoke(&init_ix, &[associated_token_account, mint_account])
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
pub fn is_off_curve(address: &Pubkey) -> bool {
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
#[inline(always)]
pub(crate) fn process_create_associated_token_account(
    program_id: &Pubkey,
    create_accounts: &CreateAccounts,
    expected_bump: Option<u8>,
    known_token_account_len: Option<usize>,
) -> ProgramResult {
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

#[cfg(test)]
mod tests {
    use super::*;
    use ata_mollusk_harness::validate_token_account_structure;
    use core::ptr;
    use std::vec::Vec;
    use {
        pinocchio::{program_error::ProgramError, pubkey::Pubkey},
        solana_keypair::Keypair,
        solana_program::pubkey::Pubkey as SolanaPubkey,
        solana_signer::Signer,
        spl_token_interface::state::{
            account::Account as TokenAccount, multisig::Multisig, Transmutable,
        },
        std::{collections::HashSet, vec},
    };

    // Test utility functions
    fn create_token_account_data(mint: &Pubkey, owner: &Pubkey, amount: u64) -> Vec<u8> {
        let mut data = vec![0u8; TokenAccount::LEN];

        // Set mint (bytes 0-31)
        data[0..32].copy_from_slice(mint.as_ref());

        // Set owner (bytes 32-63)
        data[32..64].copy_from_slice(owner.as_ref());

        // Set amount (bytes 64-71)
        data[64..72].copy_from_slice(&amount.to_le_bytes());

        // Set initialized state (byte 108)
        data[108] = 1;

        data
    }

    #[test]
    fn test_is_off_curve_true() {
        let program_id = SolanaPubkey::new_unique();
        let seeds = &[b"test_seed" as &[u8]];
        let (off_curve_address, _) = SolanaPubkey::find_program_address(seeds, &program_id);
        let pinocchio_format = Pubkey::from(off_curve_address.to_bytes());
        let result = is_off_curve(&pinocchio_format);
        assert!(result);
    }

    #[test]
    fn test_is_off_curve_false() {
        // Generate a random address
        let wallet = Keypair::new();
        let address = wallet.pubkey();
        let pinocchio_format = Pubkey::from(address.to_bytes());
        let result = is_off_curve(&pinocchio_format);
        assert!(!result);
    }

    #[test]
    fn test_valid_token_account_data() {
        // Case 1: Regular, initialized account
        let mut data1 = [0u8; TokenAccount::LEN];
        data1[108] = 1; // initialized state
        assert!(
            valid_token_account_data(&data1),
            "Regular initialized account should be valid"
        );

        // Case 2: Uninitialized account
        let mut data2 = [0u8; TokenAccount::LEN];
        data2[108] = 0; // uninitialized state
        assert!(
            !valid_token_account_data(&data2),
            "Uninitialized account should be invalid"
        );

        // Case 3: Data too short
        let data3 = [0u8; TokenAccount::LEN - 1];
        assert!(
            !valid_token_account_data(&data3),
            "Data shorter than TokenAccount::LEN should be invalid"
        );

        // Case 4: Extended, correctly typed account
        let mut data4 = vec![0u8; TokenAccount::LEN + 10];
        data4[108] = 1; // initialized
        data4[TokenAccount::LEN] = 2; // AccountType::Account
        assert!(
            valid_token_account_data(&data4),
            "Extended, correctly typed account should be valid"
        );

        // Case 5: Extended, incorrectly typed account
        let mut data5 = vec![0u8; TokenAccount::LEN + 10];
        data5[108] = 1; // initialized
        data5[TokenAccount::LEN] = 1; // Wrong account type
        assert!(
            !valid_token_account_data(&data5),
            "Extended, incorrectly typed account should be invalid"
        );

        // Case 6: Multisig collision
        let mut data6 = [0u8; Multisig::LEN];
        data6[0] = 2; // valid multisig m
        data6[1] = 3; // valid multisig n
        data6[2] = 1; // initialized
        data6[108] = 1;
        assert!(
            !valid_token_account_data(&data6),
            "Multisig data should be invalid"
        );
    }

    #[test]
    fn test_validate_token_account_owner() {
        let owner1 = Pubkey::from([1u8; 32]);
        let owner2 = Pubkey::from([2u8; 32]);
        let mint = Pubkey::from([3u8; 32]);
        let data = create_token_account_data(&mint, &owner1, 1000);
        let account: &TokenAccount = unsafe { &*(data.as_ptr() as *const TokenAccount) };

        assert!(validate_token_account_owner(account, &owner1).is_ok());
        assert_eq!(
            validate_token_account_owner(account, &owner2).unwrap_err(),
            ProgramError::IllegalOwner
        );
    }

    #[test]
    fn test_validate_token_account_mint() {
        let mint1 = Pubkey::from([1u8; 32]);
        let mint2 = Pubkey::from([2u8; 32]);
        let owner = Pubkey::from([3u8; 32]);
        let data = create_token_account_data(&mint1, &owner, 1000);
        let account: &TokenAccount = unsafe { &*(data.as_ptr() as *const TokenAccount) };

        assert!(validate_token_account_mint(account, &mint1).is_ok());
        assert_eq!(
            validate_token_account_mint(account, &mint2).unwrap_err(),
            ProgramError::InvalidAccountData
        );
    }

    #[test]
    fn test_create_token_account_data_structure() {
        let mint = Pubkey::from([1u8; 32]);
        let owner = Pubkey::from([2u8; 32]);
        let amount = 1000u64;

        let data = create_token_account_data(&mint, &owner, amount);

        assert!(validate_token_account_structure(&data, &mint, &owner));
        assert!(valid_token_account_data(&data));
    }

    #[test]
    fn test_build_initialize_account3_data_basic() {
        let owner = Pubkey::from([1u8; 32]);
        let data = build_initialize_account3_data(&owner);

        assert_eq!(data.len(), 33);
        assert_eq!(data[0], INITIALIZE_ACCOUNT_3_DISCRIMINATOR);
        assert_eq!(&data[1..33], owner.as_ref());
    }

    #[test]
    fn test_build_initialize_account3_data_different_owners() {
        let owner1 = Pubkey::from([1u8; 32]);
        let owner2 = Pubkey::from([2u8; 32]);

        let data1 = build_initialize_account3_data(&owner1);
        let data2 = build_initialize_account3_data(&owner2);

        assert_eq!(data1[0], data2[0]); // Same discriminator
        assert_ne!(&data1[1..], &data2[1..]); // Different owner bytes
    }

    #[test]
    fn test_build_transfer_data_basic() {
        let amount = 1000u64;
        let decimals = 6u8;
        let data = build_transfer_checked_data(amount, decimals);

        assert_eq!(data.len(), 10);
        assert_eq!(data[0], TRANSFER_CHECKED_DISCRIMINATOR);

        let parsed_amount = u64::from_le_bytes([
            data[1], data[2], data[3], data[4], data[5], data[6], data[7], data[8],
        ]);
        assert_eq!(parsed_amount, amount);
        assert_eq!(data[9], decimals);
    }

    #[test]
    fn test_build_transfer_data_edge_cases() {
        // Test zero values
        let data = build_transfer_checked_data(0, 0);
        assert_eq!(data.len(), 10);
        assert_eq!(data[0], TRANSFER_CHECKED_DISCRIMINATOR);
        assert_eq!(
            u64::from_le_bytes([
                data[1], data[2], data[3], data[4], data[5], data[6], data[7], data[8]
            ]),
            0
        );
        assert_eq!(data[9], 0);

        // Test max values
        let data = build_transfer_checked_data(u64::MAX, u8::MAX);
        assert_eq!(data.len(), 10);
        assert_eq!(data[0], TRANSFER_CHECKED_DISCRIMINATOR);
        assert_eq!(
            u64::from_le_bytes([
                data[1], data[2], data[3], data[4], data[5], data[6], data[7], data[8]
            ]),
            u64::MAX
        );
        assert_eq!(data[9], u8::MAX);
    }

    #[test]
    fn test_build_transfer_data_endianness() {
        let amount = 0x0123456789abcdef_u64;
        let decimals = 6u8;
        let data = build_transfer_checked_data(amount, decimals);

        // Verify little-endian encoding
        let expected_bytes = amount.to_le_bytes();
        assert_eq!(&data[1..9], &expected_bytes);
    }

    #[test]
    fn test_instruction_data_deterministic() {
        let owner = Pubkey::from([42u8; 32]);
        let amount = 1000u64;
        let decimals = 6u8;

        // Test that identical inputs produce identical outputs
        let data1 = build_initialize_account3_data(&owner);
        let data2 = build_initialize_account3_data(&owner);
        assert_eq!(data1, data2);

        let transfer1 = build_transfer_checked_data(amount, decimals);
        let transfer2 = build_transfer_checked_data(amount, decimals);
        assert_eq!(transfer1, transfer2);
    }

    #[test]
    fn test_discriminator_uniqueness() {
        use crate::recover::CLOSE_ACCOUNT_DISCRIMINATOR;

        let discriminators = [
            INITIALIZE_ACCOUNT_3_DISCRIMINATOR,
            INITIALIZE_IMMUTABLE_OWNER_DISCRIMINATOR,
            TRANSFER_CHECKED_DISCRIMINATOR,
            CLOSE_ACCOUNT_DISCRIMINATOR,
        ];

        let mut unique_discriminators = HashSet::new();
        for &d in &discriminators {
            unique_discriminators.insert(d);
        }

        assert_eq!(
            discriminators.len(),
            unique_discriminators.len(),
            "All discriminators must be unique"
        );
    }

    // Test utility - AccountLayout structure for creating test accounts
    #[repr(C)]
    struct AccountLayout {
        borrow_state: u8,
        is_signer: u8,
        is_writable: u8,
        executable: u8,
        resize_delta: u32,
        key: Pubkey,
        owner: Pubkey,
        lamports: u64,
        data_len: u64,
    }

    fn with_test_accounts_for_parsing<F, R>(count: usize, test_fn: F) -> R
    where
        F: FnOnce(&[AccountInfo]) -> R,
    {
        let mut account_data: Vec<AccountLayout> = (0..count)
            .map(|i| AccountLayout {
                borrow_state: 0b_1111_1111,
                is_signer: 0,
                is_writable: 0,
                executable: 0,
                resize_delta: 0,
                key: Pubkey::from([i as u8; 32]),
                owner: Pubkey::from([(i as u8).wrapping_add(1); 32]),
                lamports: 0,
                data_len: 0,
            })
            .collect();

        let account_infos: Vec<AccountInfo> = account_data
            .iter_mut()
            .map(|layout| unsafe { std::mem::transmute::<*mut AccountLayout, AccountInfo>(layout) })
            .collect();

        test_fn(&account_infos)
    }

    #[test]
    fn test_parse_create_accounts_success_without_rent() {
        // Exactly 6 accounts – rent sysvar should be `None`.
        with_test_accounts_for_parsing(6, |accounts| {
            let parsed = parse_create_accounts(accounts).unwrap();

            assert!(ptr::eq(parsed.payer, &accounts[0]));
            assert_eq!(parsed.payer.key(), accounts[0].key());
            assert!(ptr::eq(
                parsed.associated_token_account_to_create,
                &accounts[1]
            ));
            assert_eq!(
                parsed.associated_token_account_to_create.key(),
                accounts[1].key()
            );
            assert!(ptr::eq(parsed.wallet, &accounts[2]));
            assert_eq!(parsed.wallet.key(), accounts[2].key());
            assert!(ptr::eq(parsed.mint, &accounts[3]));
            assert_eq!(parsed.mint.key(), accounts[3].key());
            assert!(ptr::eq(parsed.system_program, &accounts[4]));
            assert_eq!(parsed.system_program.key(), accounts[4].key());
            assert!(ptr::eq(parsed.token_program, &accounts[5]));
            assert_eq!(parsed.token_program.key(), accounts[5].key());
            assert!(parsed.rent_sysvar.is_none());
        });
    }

    #[test]
    fn test_parse_create_accounts_success_with_rent() {
        // 7 accounts – index 6 is rent sysvar.
        with_test_accounts_for_parsing(7, |accounts| {
            assert_eq!(accounts.len(), 7);

            let parsed = parse_create_accounts(accounts).unwrap();

            assert!(parsed.rent_sysvar.is_some());
            assert!(ptr::eq(parsed.rent_sysvar.unwrap(), &accounts[6]));
            assert_eq!(parsed.rent_sysvar.unwrap().key(), accounts[6].key());
        });
    }

    #[test]
    fn test_parse_create_accounts_error_insufficient() {
        with_test_accounts_for_parsing(5, |accounts| {
            assert!(matches!(
                parse_create_accounts(accounts),
                Err(ProgramError::NotEnoughAccountKeys)
            ));
        });
    }

    #[test]
    fn test_fn_is_spl_token_program() {
        assert!(is_spl_token_program(&spl_token_interface::program::id()));

        let token_2022_id =
            pinocchio_pubkey::pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
        assert!(!is_spl_token_program(&token_2022_id));
    }
}
