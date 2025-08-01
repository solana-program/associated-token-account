use curve25519_dalek::edwards::CompressedEdwardsY;
use solana_pubkey::Pubkey;

use crate::tests::benches::common::{AccountTypeId, AtaVariant, TestBankId};

/// Generate a structured pubkey from 4-byte coordinate system
/// [variant, test_bank, test_number, account_type].
/// Avoids some issues with test cross-contamination by using predictable
/// but different keys for different tests.
pub fn structured_pk(
    variant: &AtaVariant,
    test_bank: TestBankId,
    test_number: u8,
    account_type: AccountTypeId,
) -> Pubkey {
    // For proper byte-for-byte comparison between implementations,
    // use consistent addresses for wallet/owner and mint accounts
    let effective_variant = match account_type {
        AccountTypeId::Wallet
        | AccountTypeId::Mint
        | AccountTypeId::OwnerMint
        | AccountTypeId::NestedMint => &AtaVariant::SplAta, // Always use Original for consistency
        _ => variant, // Use actual variant for other account types (Payer, ATA addresses, etc.)
    };

    let mut bytes = [0u8; 32];
    bytes[0] = variant_to_byte(effective_variant);
    bytes[1] = test_bank as u8;
    bytes[2] = test_number;
    bytes[3] = account_type as u8;

    Pubkey::new_from_array(bytes)
}

/// Generate multiple structured pubkeys at once.
/// Avoids some issues with test cross-contamination by using predictable
/// but different keys for different tests.
#[allow(dead_code)]
pub fn structured_pk_multi<const N: usize>(
    variant: &AtaVariant,
    test_bank: TestBankId,
    test_number: u8,
    account_types: [AccountTypeId; N],
) -> [Pubkey; N] {
    account_types.map(|account_type| structured_pk(variant, test_bank, test_number, account_type))
}

/// Generate a random pubkey for benchmark testing
///
/// Creates a random wallet address with some deterministic seed for test reproducibility
/// but without optimal bump hunting. This provides truly random compute unit results.
///
/// # Arguments
/// * `variant` - The ATA variant to use for seeding
/// * `test_bank` - The test bank ID for seeding
/// * `test_number` - The test number for seeding  
/// * `account_type` - The account type for seeding
/// * `iteration` - Current iteration number for additional randomness
/// * `run_entropy` - A run-specific entropy value to use for seeding
///
/// # Returns
/// A random pubkey seeded by the test parameters and current iteration
pub fn random_seeded_pk(
    variant: &AtaVariant,
    test_bank: TestBankId,
    test_number: u8,
    account_type: AccountTypeId,
    iteration: usize,
    run_entropy: u64,
) -> Pubkey {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // Create a deterministic but random-looking seed from test parameters
    let mut hasher = DefaultHasher::new();
    variant_to_byte(variant).hash(&mut hasher);
    (test_bank as u8).hash(&mut hasher);
    test_number.hash(&mut hasher);
    (account_type as u8).hash(&mut hasher);
    iteration.hash(&mut hasher);

    // Add run-specific entropy so single runs vary between executions
    // This run_entropy should be the same for P-ATA and SPL ATA within a single test
    run_entropy.hash(&mut hasher);

    let hash = hasher.finish();

    // Convert hash to 32-byte array for pubkey
    let mut bytes = [0u8; 32];
    bytes[0..8].copy_from_slice(&hash.to_le_bytes());
    bytes[8..16].copy_from_slice(&(hash.wrapping_mul(0x9E3779B9)).to_le_bytes());
    bytes[16..24].copy_from_slice(&(hash.wrapping_mul(0x85EBCA6B)).to_le_bytes());
    bytes[24..32].copy_from_slice(&(hash.wrapping_mul(0xC2B2AE35)).to_le_bytes());

    Pubkey::new_from_array(bytes)
}

/// Find a wallet that produces bump 255 for ALL given mints
///
/// Modular function that searches for a wallet that when used in find_program_address
/// with [wallet, token_program, mint] produces bump 255 for EVERY mint in the array.
///
/// # Arguments
/// * `token_program` - Token program ID for ATA derivation
/// * `mints` - Array of mint addresses that must ALL produce bump 255  
/// * `ata_program` - ATA program ID for derivation
/// * `base_entropy` - Base entropy for deterministic starting point
///
/// # Returns
/// A wallet pubkey that produces bump 255 for [wallet, token_program, mint] for ALL mints
///
/// # Usage
/// - Create operations: `find_optimal_wallet_for_mints(&[mint])`
/// - Recover operations: `find_optimal_wallet_for_mints(&[owner_mint, nested_mint])`
pub fn find_optimal_wallet_for_mints(
    token_program: &Pubkey,
    mints: &[Pubkey],
    ata_programs: &[Pubkey],
    base_entropy: u64,
) -> Pubkey {
    let mut modifier = base_entropy;

    loop {
        // Generate candidate wallet from modifier
        let mut wallet_bytes = [0u8; 32];
        wallet_bytes[0..8].copy_from_slice(&modifier.to_le_bytes());
        wallet_bytes[8..16].copy_from_slice(&(modifier.wrapping_mul(0x9E3779B9)).to_le_bytes());
        wallet_bytes[16..24].copy_from_slice(&(modifier.wrapping_mul(0x85EBCA6B)).to_le_bytes());
        wallet_bytes[24..32].copy_from_slice(&(modifier.wrapping_mul(0xC2B2AE35)).to_le_bytes());

        let candidate_wallet = Pubkey::new_from_array(wallet_bytes);

        // Check if this wallet produces bump 255 for ALL mints across ALL ATA programs
        let all_optimal = mints.iter().all(|mint| {
            ata_programs.iter().all(|ata_program| {
                let (_, bump) = Pubkey::find_program_address(
                    &[
                        candidate_wallet.as_ref(),
                        token_program.as_ref(),
                        mint.as_ref(),
                    ],
                    ata_program,
                );
                bump == 255
            })
        });

        if all_optimal {
            return candidate_wallet;
        }

        modifier = modifier.wrapping_add(1);
    }
}

/// Manual PDA derivation using the same logic as pinocchio_pubkey::derive_address
/// This replicates the exact address derivation process for testing purposes
pub fn derive_address_with_bump(seeds: &[&[u8]], bump: u8, program_id: &Pubkey) -> Pubkey {
    let mut hasher = solana_program::hash::Hasher::default();
    for seed in seeds {
        hasher.hash(seed);
    }
    hasher.hash(&[bump]);
    hasher.hash(program_id.as_ref());
    hasher.hash(b"ProgramDerivedAddress");

    let hash = hasher.result();
    Pubkey::from(hash.to_bytes())
}

/// Simple off-curve check for testing that mirrors the logic in processor.rs
/// Returns true if the address is off-curve (valid PDA), false if on-curve (invalid PDA)
pub fn is_off_curve(address: &Pubkey) -> bool {
    let compressed = CompressedEdwardsY(address.to_bytes());
    match compressed.decompress() {
        None => true,                    // invalid encoding → off-curve
        Some(pt) => pt.is_small_order(), // small-order = off-curve, otherwise on-curve
    }
}

/// Generate a pubkey with optimal bump (255) for consistent single-iteration benchmarking
///
/// When benchmarking with iterations=1, this ensures predictable results by finding
/// wallets that produce bump=255, which is optimal for ATA derivation performance.
/// Falls back to random generation for multiple iterations to maintain test variety.
///
/// # Arguments
/// * `variant` - The ATA variant to use for seeding
/// * `test_bank` - The test bank ID for seeding
/// * `test_number` - The test number for seeding  
/// * `account_type` - The account type for seeding
/// * `iteration` - Current iteration number
/// * `run_entropy` - A run-specific entropy value to use for seeding
/// * `token_program_id` - Token program ID for ATA derivation
/// * `ata_program_id` - ATA program ID for derivation
/// * `mint` - Mint address for ATA derivation
/// * `max_iterations` - Total number of benchmark iterations (to detect single-iteration mode)
///
/// # Returns
/// A pubkey that produces optimal bump when used as wallet for ATA derivation
pub fn const_pk_with_optimal_bump(
    variant: &AtaVariant,
    test_bank: TestBankId,
    test_number: u8,
    account_type: AccountTypeId,
    iteration: usize,
    run_entropy: u64,
    token_program_id: &Pubkey,
    ata_program_ids: &[Pubkey],
    mint: &Pubkey,
    max_iterations: usize,
) -> Pubkey {
    // For multiple iterations or non-wallet account types, use random generation
    if max_iterations > 1 || account_type != AccountTypeId::Wallet {
        return random_seeded_pk(
            variant,
            test_bank,
            test_number,
            account_type,
            iteration,
            run_entropy,
        );
    }

    // For single iterations on wallet generation, find optimal bump (255)
    let search_entropy = run_entropy
        .wrapping_add(test_number as u64)
        .wrapping_add(iteration as u64);

    find_optimal_wallet_for_mints(token_program_id, &[*mint], ata_program_ids, search_entropy)
}

/// Convert AtaVariant to byte value
fn variant_to_byte(variant: &AtaVariant) -> u8 {
    match variant {
        AtaVariant::PAtaLegacy => 1, // avoid system program ID
        AtaVariant::PAtaPrefunded => 2,
        AtaVariant::SplAta => 3,
    }
}
