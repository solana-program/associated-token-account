use {
    mollusk_svm::result::Check,
    solana_address::Address,
    solana_instruction::AccountMeta,
    solana_program_error::ProgramError,
    spl_associated_token_account_interface::address::get_associated_token_address_with_program_id,
    spl_associated_token_account_mollusk_harness::{
        build_create_ata_instruction, AtaProgramUnderTest, AtaTestHarness, CreateAtaInstructionType,
    },
    test_case::test_matrix,
};

#[test_matrix(
    [AtaProgramUnderTest::Legacy, AtaProgramUnderTest::Pinocchio],
    [spl_token_interface::id(), spl_token_2022_interface::id()]
)]
fn create_rejects_too_few_accounts(ata_program: AtaProgramUnderTest, token_program_id: Address) {
    let harness =
        AtaTestHarness::new_for(ata_program, &token_program_id).with_wallet_and_mint(1_000_000, 6);
    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let ata_address =
        get_associated_token_address_with_program_id(&wallet, &mint, &token_program_id);

    let mut instruction = build_create_ata_instruction(
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
    instruction.accounts.truncate(5);

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[Check::err(ProgramError::NotEnoughAccountKeys)],
    );
}

#[test_matrix(
    [AtaProgramUnderTest::Legacy, AtaProgramUnderTest::Pinocchio],
    [spl_token_interface::id(), spl_token_2022_interface::id()]
)]
fn create_account_mismatch(ata_program: AtaProgramUnderTest, token_program_id: Address) {
    let harness =
        AtaTestHarness::new_for(ata_program, &token_program_id).with_wallet_and_mint(1_000_000, 6);

    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let ata_address =
        get_associated_token_address_with_program_id(&wallet, &mint, &token_program_id);

    for account_idx in [1, 2, 3, 5] {
        let mut instruction = build_create_ata_instruction(
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

        instruction.accounts[account_idx] = if account_idx == 1 {
            AccountMeta::new(Address::default(), false)
        } else {
            AccountMeta::new_readonly(Address::default(), false)
        };

        harness.ctx.process_and_validate_instruction(
            &instruction,
            &[Check::err(ProgramError::InvalidSeeds)],
        );
    }
}
