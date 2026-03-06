use {
    mollusk_svm::result::Check,
    solana_address::Address,
    solana_program_error::ProgramError,
    solana_program_pack::Pack,
    spl_associated_token_account_interface::address::get_associated_token_address_with_program_id,
    spl_associated_token_account_mollusk_harness::{
        build_create_ata_instruction, token_account_rent_exempt_balance, AccountBuilder,
        AtaProgramUnderTest, AtaTestHarness, CreateAtaInstructionType,
    },
    test_case::{test_case, test_matrix},
};

#[test_case(AtaProgramUnderTest::Legacy)]
#[test_case(AtaProgramUnderTest::Pinocchio)]
fn idempotent_rejects_non_token_owned_canonical_ata(ata_program: AtaProgramUnderTest) {
    let harness = AtaTestHarness::new_for(ata_program, &spl_token_2022_interface::id())
        .with_wallet_and_mint(1_000_000, 6);
    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let ata_address = get_associated_token_address_with_program_id(
        &wallet,
        &mint,
        &spl_token_2022_interface::id(),
    );

    let mut non_token_account = AccountBuilder::system_account(token_account_rent_exempt_balance());
    non_token_account.owner = Address::new_unique();
    harness
        .ctx
        .account_store
        .borrow_mut()
        .insert(ata_address, non_token_account);

    let instruction = build_create_ata_instruction(
        spl_associated_token_account_interface::program::id(),
        harness.payer,
        ata_address,
        wallet,
        mint,
        spl_token_2022_interface::id(),
        CreateAtaInstructionType::CreateIdempotent { bump: None },
    );

    harness
        .ctx
        .process_and_validate_instruction(&instruction, &[Check::err(ProgramError::IllegalOwner)]);
}

#[test_case(AtaProgramUnderTest::Legacy)]
#[test_case(AtaProgramUnderTest::Pinocchio)]
fn idempotent_rejects_uninitialized_token_owned_canonical_ata(ata_program: AtaProgramUnderTest) {
    let harness = AtaTestHarness::new_for(ata_program, &spl_token_2022_interface::id())
        .with_wallet_and_mint(1_000_000, 6);
    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let ata_address = get_associated_token_address_with_program_id(
        &wallet,
        &mint,
        &spl_token_2022_interface::id(),
    );

    let mut uninitialized_token_account =
        AccountBuilder::system_account(token_account_rent_exempt_balance());
    uninitialized_token_account.owner = spl_token_2022_interface::id();
    uninitialized_token_account.data = vec![0; spl_token_interface::state::Account::LEN];
    harness
        .ctx
        .account_store
        .borrow_mut()
        .insert(ata_address, uninitialized_token_account);

    let instruction = build_create_ata_instruction(
        spl_associated_token_account_interface::program::id(),
        harness.payer,
        ata_address,
        wallet,
        mint,
        spl_token_2022_interface::id(),
        CreateAtaInstructionType::CreateIdempotent { bump: None },
    );

    harness
        .ctx
        .process_and_validate_instruction(&instruction, &[Check::err(ProgramError::IllegalOwner)]);
}

#[test_case(AtaProgramUnderTest::Legacy)]
#[test_case(AtaProgramUnderTest::Pinocchio)]
fn idempotent_rejects_malformed_token_owned_canonical_ata(ata_program: AtaProgramUnderTest) {
    let harness = AtaTestHarness::new_for(ata_program, &spl_token_2022_interface::id())
        .with_wallet_and_mint(1_000_000, 6);
    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let ata_address = get_associated_token_address_with_program_id(
        &wallet,
        &mint,
        &spl_token_2022_interface::id(),
    );

    let mut malformed_token_account =
        AccountBuilder::system_account(token_account_rent_exempt_balance());
    malformed_token_account.owner = spl_token_2022_interface::id();
    malformed_token_account.data = vec![0];
    harness
        .ctx
        .account_store
        .borrow_mut()
        .insert(ata_address, malformed_token_account);

    let instruction = build_create_ata_instruction(
        spl_associated_token_account_interface::program::id(),
        harness.payer,
        ata_address,
        wallet,
        mint,
        spl_token_2022_interface::id(),
        CreateAtaInstructionType::CreateIdempotent { bump: None },
    );

    harness
        .ctx
        .process_and_validate_instruction(&instruction, &[Check::err(ProgramError::IllegalOwner)]);
}

#[test_case(AtaProgramUnderTest::Legacy)]
#[test_case(AtaProgramUnderTest::Pinocchio)]
fn idempotent_rejects_wrong_owner(ata_program: AtaProgramUnderTest) {
    let harness = AtaTestHarness::new_for(ata_program, &spl_token_2022_interface::id())
        .with_wallet_and_mint(1_000_000, 6);
    let wrong_owner = Address::new_unique();
    let ata_address = harness.insert_token_account_at_ata_address(wrong_owner);

    let instruction = build_create_ata_instruction(
        spl_associated_token_account_interface::program::id(),
        harness.payer,
        ata_address,
        harness.wallet.unwrap(),
        harness.mint.unwrap(),
        spl_token_2022_interface::id(),
        CreateAtaInstructionType::CreateIdempotent { bump: None },
    );

    harness
        .ctx
        .process_and_validate_instruction(&instruction, &[Check::err(ProgramError::Custom(0))]);
}

#[test_case(AtaProgramUnderTest::Legacy)]
#[test_case(AtaProgramUnderTest::Pinocchio)]
fn idempotent_rejects_wrong_mint(ata_program: AtaProgramUnderTest) {
    let harness = AtaTestHarness::new_for(ata_program, &spl_token_2022_interface::id())
        .with_wallet_and_mint(1_000_000, 6);
    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let wrong_mint = Address::new_unique();
    let ata_address = get_associated_token_address_with_program_id(
        &wallet,
        &mint,
        &spl_token_2022_interface::id(),
    );

    harness.ctx.account_store.borrow_mut().insert(
        ata_address,
        AccountBuilder::token_account(&wrong_mint, &wallet, 0, &spl_token_2022_interface::id()),
    );

    let instruction = build_create_ata_instruction(
        spl_associated_token_account_interface::program::id(),
        harness.payer,
        ata_address,
        wallet,
        mint,
        spl_token_2022_interface::id(),
        CreateAtaInstructionType::CreateIdempotent { bump: None },
    );

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[Check::err(ProgramError::InvalidAccountData)],
    );
}

#[test_matrix(
    [AtaProgramUnderTest::Legacy, AtaProgramUnderTest::Pinocchio],
    [spl_token_interface::id(), spl_token_2022_interface::id()]
)]
fn idempotent_accepts_preexisting_valid_ata(
    ata_program: AtaProgramUnderTest,
    token_program_id: Address,
) {
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
        CreateAtaInstructionType::CreateIdempotent { bump: None },
    );

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[
            Check::success(),
            Check::account(&ata_address)
                .space(spl_token_interface::state::Account::LEN)
                .owner(&token_program_id)
                .lamports(token_account_rent_exempt_balance())
                .build(),
        ],
    );
}
