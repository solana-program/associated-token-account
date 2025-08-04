#![cfg_attr(feature = "std", allow(dead_code, unused_imports))]

use pinocchio_ata_program::{
    processor::is_off_curve, test_helpers::address_gen::derive_address_with_bump,
};
#[cfg(any(test, feature = "std"))]
use {
    mollusk_svm::Mollusk, pinocchio::pubkey::Pubkey,
    pinocchio_ata_program::test_utils::setup_mollusk_with_programs,
    solana_pubkey::Pubkey as SolanaPubkey,
};

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
        let wallet = SolanaPubkey::new_unique();

        let (canonical_addr, found_bump) = SolanaPubkey::find_program_address(
            &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()],
            &SolanaPubkey::new_from_array(*ata_program_id),
        );

        // We need find_program_address to return exactly the target canonical bump
        // This means attack_bump and all higher bumps are on-curve
        if found_bump != target_canonical_bump {
            continue;
        }

        // Manually derive the attack address using the higher bump
        let seeds: &[&[u8]; 3] = &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()];
        let attack_addr = derive_address_with_bump(
            seeds,
            attack_bump,
            &SolanaPubkey::new_from_array(*ata_program_id),
        );

        return Some((
            wallet.to_bytes(),
            canonical_addr.to_bytes(),
            attack_addr.to_bytes(),
            attack_bump,
        ));
    }
    None
}

/// Setup mollusk with both ATA and token programs for bump testing
#[cfg(any(test, feature = "std"))]
pub fn setup_mollusk_for_bump_tests(token_program_id: &Pubkey) -> Mollusk {
    setup_mollusk_with_programs(&SolanaPubkey::new_from_array(*token_program_id))
}

#[cfg(any(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn test_derive_address_with_bump() {
        use std::eprintln;
        let wallet = SolanaPubkey::new_unique();
        let token_program = spl_token::id();
        let mint = SolanaPubkey::new_unique();
        let ata_program_id = spl_associated_token_account::id();

        // Test that our manual derivation matches find_program_address
        let (expected_addr, expected_bump) = SolanaPubkey::find_program_address(
            &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()],
            &ata_program_id,
        );
        eprintln!("expected_addr: {:?}", expected_addr);
        eprintln!("expected_bump: {:?}", expected_bump);
        let seeds: &[&[u8]; 3] = &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()];
        let derived_addr = derive_address_with_bump(seeds, expected_bump, &ata_program_id);
        eprintln!("derived_addr: {:?}", derived_addr);
        assert_eq!(expected_addr, derived_addr);
    }
}
