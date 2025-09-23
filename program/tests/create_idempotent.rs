use {
    ata_mollusk_harness::{
        build_create_ata_instruction, token_2022_immutable_owner_account_len,
        token_2022_immutable_owner_rent_exempt_balance, AtaTestHarness, CreateAtaInstructionType,
    },
    mollusk_svm::result::Check,
    solana_program_error::ProgramError,
    solana_pubkey::Pubkey,
};

#[test]
fn success_account_exists() {
    let mut harness =
        AtaTestHarness::new(&spl_token_2022_interface::id()).with_wallet_and_mint(1_000_000, 6);
    // CreateIdempotent will create the ATA if it doesn't exist
    let ata_address = harness.create_ata(CreateAtaInstructionType::CreateIdempotent { bump: None });
    let associated_account = harness
        .ctx
        .account_store
        .borrow()
        .get(&ata_address)
        .cloned()
        .unwrap();

    // Failure case: try to Create when ATA already exists as token account
    harness
        .ctx
        .account_store
        .borrow_mut()
        .insert(ata_address, associated_account.clone());
    let instruction = build_create_ata_instruction(
        spl_associated_token_account::id(),
        harness.payer,
        ata_address,
        harness.wallet.unwrap(),
        harness.mint.unwrap(),
        spl_token_2022_interface::id(),
        CreateAtaInstructionType::default(),
    );
    harness
        .ctx
        .process_and_validate_instruction(&instruction, &[Check::err(ProgramError::IllegalOwner)]);

    // But CreateIdempotent should succeed when account exists
    let instruction = build_create_ata_instruction(
        spl_associated_token_account::id(),
        harness.payer,
        ata_address,
        harness.wallet.unwrap(),
        harness.mint.unwrap(),
        spl_token_2022_interface::id(),
        CreateAtaInstructionType::CreateIdempotent { bump: None },
    );
    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[
            Check::success(),
            Check::account(&ata_address)
                .space(token_2022_immutable_owner_account_len())
                .owner(&spl_token_2022_interface::id())
                .lamports(token_2022_immutable_owner_rent_exempt_balance())
                .build(),
        ],
    );
}

#[test]
fn fail_account_exists_with_wrong_owner() {
    let harness =
        AtaTestHarness::new(&spl_token_2022_interface::id()).with_wallet_and_mint(1_000_000, 6);
    let wrong_owner = Pubkey::new_unique();
    let ata_address = harness.insert_wrong_owner_token_account(wrong_owner);
    let instruction = build_create_ata_instruction(
        spl_associated_token_account::id(),
        harness.payer,
        ata_address,
        harness.wallet.unwrap(),
        harness.mint.unwrap(),
        spl_token_2022_interface::id(),
        CreateAtaInstructionType::CreateIdempotent { bump: None },
    );
    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[Check::err(ProgramError::Custom(
            spl_associated_token_account::error::AssociatedTokenAccountError::InvalidOwner as u32,
        ))],
    );
}

#[test]
fn fail_non_ata() {
    let harness =
        AtaTestHarness::new(&spl_token_2022_interface::id()).with_wallet_and_mint(1_000_000, 6);
    let wrong_account = Pubkey::new_unique();
    harness.execute_with_wrong_account_address(wrong_account, ProgramError::InvalidSeeds);
}
