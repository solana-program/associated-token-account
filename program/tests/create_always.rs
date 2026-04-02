use {
    mollusk_svm::result::Check,
    solana_instruction::AccountMeta,
    solana_program_error::ProgramError,
    solana_pubkey::Pubkey,
    spl_associated_token_account_mollusk_harness::{
        build_create_ata_instruction, AtaTestHarness, CreateAtaInstructionType,
    },
    test_case::test_case,
};

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn create_rejects_existing_ata(token_program_id: Pubkey) {
    let harness = AtaTestHarness::new(&token_program_id).with_wallet_and_mint(1_000_000, 6);
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

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn create_accepts_legacy_implicit_instruction(token_program_id: Pubkey) {
    let mut harness = AtaTestHarness::new(&token_program_id).with_wallet_and_mint(1_000_000, 6);

    harness.create_and_check_ata_with_custom_instruction(
        CreateAtaInstructionType::default(),
        |instruction| {
            instruction.data = vec![];
            instruction
                .accounts
                .push(AccountMeta::new_readonly(solana_sysvar::rent::id(), false));
        },
    );
}
