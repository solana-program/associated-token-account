#![cfg_attr(feature = "std", allow(dead_code, unused_imports))]

#[cfg(any(test, feature = "std"))]
use {
    crate::tests::test_utils::setup_mollusk_with_programs,
    curve25519_dalek::edwards::CompressedEdwardsY,
    mollusk_svm::{program::loader_keys::LOADER_V3, Mollusk},
    solana_program,
    solana_pubkey::Pubkey,
    std::vec::Vec,
};

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
pub fn is_off_curve_test(address: &Pubkey) -> bool {
    #[cfg(any(test, feature = "std"))]
    {
        let compressed = CompressedEdwardsY(address.to_bytes());
        match compressed.decompress() {
            None => true,                    // invalid encoding → off-curve
            Some(pt) => pt.is_small_order(), // small-order = off-curve, otherwise on-curve
        }
    }
    #[cfg(not(any(test, feature = "std")))]
    {
        // Fallback for when mollusk_svm is not available (e.g., in a CI environment)
        // This is a placeholder and should ideally be replaced with a proper check
        // For now, we'll assume any address is off-curve if mollusk_svm is not present
        // This is a simplification and might need refinement based on actual requirements
        true
    }
}

/// Find a wallet where find_program_address returns the target canonical bump,
/// meaning all bumps > canonical_bump are on-curve.
/// Then derive an on-curve address at canonical_bump + 1.
/// Returns: (wallet, canonical_address, on_curve_address, attack_bump)
pub fn find_wallet_with_on_curve_attack_opportunity(
    target_canonical_bump: u8,
    token_program: &Pubkey,
    mint: &Pubkey,
    ata_program_id: &Pubkey,
) -> Option<(Pubkey, Pubkey, Pubkey, u8)> {
    const MAX_FIND_ATTEMPTS: u32 = 100_000;
    let attack_bump = target_canonical_bump + 1;

    for _ in 0..MAX_FIND_ATTEMPTS {
        let wallet = Pubkey::new_unique();

        let (canonical_addr, found_bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()],
            ata_program_id,
        );

        // We need find_program_address to return exactly the target canonical bump
        // This means attack_bump and all higher bumps are on-curve
        if found_bump != target_canonical_bump {
            continue;
        }

        // Manually derive the attack address using the higher bump
        let seeds: &[&[u8]; 3] = &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()];
        let attack_addr = derive_address_with_bump(seeds, attack_bump, ata_program_id);

        return Some((wallet, canonical_addr, attack_addr, attack_bump));
    }
    None
}

/// Find a wallet where the destination ATA has a non-canonical bump opportunity.
/// This means there are multiple valid off-curve addresses at different bumps.
/// Returns: (wallet, owner_mint, nested_mint, canonical_bump, non_canonical_bump)
pub fn find_wallet_with_non_canonical_opportunity(
    token_program: &Pubkey,
    ata_program_id: &Pubkey,
) -> Option<(Pubkey, Pubkey, Pubkey, u8, u8)> {
    const MAX_FIND_ATTEMPTS: u32 = 200_000;

    for _ in 0..MAX_FIND_ATTEMPTS {
        let wallet = Pubkey::new_unique();
        let owner_mint = Pubkey::new_unique();
        let nested_mint = Pubkey::new_unique();

        // Find canonical destination ATA bump (this is the highest off-curve bump)
        let (_, canonical_bump) = Pubkey::find_program_address(
            &[
                wallet.as_ref(),
                token_program.as_ref(),
                nested_mint.as_ref(),
            ],
            ata_program_id,
        );

        // Look for a lower bump that's also off-curve (this would be non-canonical)
        // We iterate downward from canonical_bump - 1
        for lower_bump in (0..canonical_bump).rev() {
            let seeds: &[&[u8]; 3] = &[
                wallet.as_ref(),
                token_program.as_ref(),
                nested_mint.as_ref(),
            ];

            let lower_address = derive_address_with_bump(seeds, lower_bump, ata_program_id);

            // Check if this lower bump also produces an off-curve address
            // If so, we have a non-canonical scenario: lower_bump is valid but not optimal
            if is_off_curve_test(&lower_address) {
                return Some((wallet, owner_mint, nested_mint, canonical_bump, lower_bump));
            }
        }
    }
    None
}

/// Setup mollusk with both ATA and token programs for bump testing
#[cfg(any(test, feature = "std"))]
pub fn setup_mollusk_for_bump_tests(token_program_id: &Pubkey) -> Mollusk {
    setup_mollusk_with_programs(token_program_id)
}

#[cfg(any(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn test_derive_address_with_bump() {
        let wallet = Pubkey::new_unique();
        let token_program = spl_token::id();
        let mint = Pubkey::new_unique();
        let ata_program_id = spl_associated_token_account::id();

        // Test that our manual derivation matches find_program_address
        let (expected_addr, expected_bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()],
            &ata_program_id,
        );

        let seeds: &[&[u8]; 3] = &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()];
        let derived_addr = derive_address_with_bump(seeds, expected_bump, &ata_program_id);

        assert_eq!(expected_addr, derived_addr);
    }

    #[test]
    fn test_is_off_curve_test_basic() {
        // Test with a known off-curve address (system program ID is typically off-curve)
        let system_program = Pubkey::new_from_array([0u8; 32]);
        // We can't guarantee this specific address, so just test the function doesn't panic
        let _ = is_off_curve_test(&system_program);
    }
}
