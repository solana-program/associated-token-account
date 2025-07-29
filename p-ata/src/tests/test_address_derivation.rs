use {
    crate::processor::is_off_curve, pinocchio::pubkey::Pubkey, solana_keypair::Keypair,
    solana_signer::Signer,
};

#[test]
fn test_is_off_curve_true() {
    let address = Pubkey::from([0u8; 32]);
    let result = is_off_curve(&address);
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
