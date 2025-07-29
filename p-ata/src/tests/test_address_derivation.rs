use {
    crate::processor::is_off_curve, pinocchio::pubkey::Pubkey, solana_keypair::Keypair,
    solana_program::pubkey::Pubkey as SolanaPubkey, solana_signer::Signer,
};

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
