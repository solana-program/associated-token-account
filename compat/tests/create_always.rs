use {
    mollusk_svm::result::Check,
    solana_address::Address,
    solana_program_error::ProgramError,
    spl_associated_token_account_mollusk_harness::{
        build_create_ata_instruction, AtaProgramUnderTest, AtaTestHarness, CreateAtaInstructionType,
    },
    test_case::test_matrix,
};

#[test_matrix(
    [AtaProgramUnderTest::Legacy, AtaProgramUnderTest::Pinocchio],
    [spl_token_interface::id(), spl_token_2022_interface::id()]
)]
fn create_rejects_existing_ata(ata_program: AtaProgramUnderTest, token_program_id: Address) {
    let harness =
        AtaTestHarness::new_for(ata_program, &token_program_id).with_wallet_and_mint(1_000_000, 6);
    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let ata_address = harness.insert_token_account_at_ata_address(wallet);

    let instruction = build_create_ata_instruction(
        spl_associated_token_account_interface::program::id(),
        harness.payer,
        ata_address,
        wallet,
        mint,
        token_program_id,
        CreateAtaInstructionType::Create {
            bump: None,
            account_len: None,
        },
    );

    harness
        .ctx
        .process_and_validate_instruction(&instruction, &[Check::err(ProgramError::IllegalOwner)]);
}
