use {
    mollusk_svm::result::Check,
    solana_program_error::ProgramError,
    solana_program_pack::Pack,
    solana_pubkey::Pubkey,
    spl_associated_token_account_interface::address::get_associated_token_address_with_program_id,
    spl_associated_token_account_mollusk_harness::{
        AccountBuilder, AtaTestHarness, CreateAtaInstructionType, build_create_ata_instruction,
        token_account_rent_exempt_balance,
    },
    spl_token_interface::state::AccountState,
    test_case::test_case,
};

const TOKEN_ACCOUNT_STATE_OFFSET: usize = 108;

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn idempotent_rejects_non_token_owned_canonical_ata(token_program_id: Pubkey) {
    let harness = AtaTestHarness::new(&token_program_id).with_wallet_and_mint(1_000_000, 6);
    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let ata_address =
        get_associated_token_address_with_program_id(&wallet, &mint, &token_program_id);

    let mut non_token_account = AccountBuilder::system_account(token_account_rent_exempt_balance());
    non_token_account.owner = Pubkey::new_unique();
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
        token_program_id,
        CreateAtaInstructionType::CreateIdempotent {
            bump: None,
            rent_sysvar_via_account: false,
        },
    );

    harness
        .ctx
        .process_and_validate_instruction(&instruction, &[Check::err(ProgramError::IllegalOwner)]);
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn idempotent_rejects_uninitialized_token_owned_canonical_ata(token_program_id: Pubkey) {
    let harness = AtaTestHarness::new(&token_program_id).with_wallet_and_mint(1_000_000, 6);
    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let ata_address =
        get_associated_token_address_with_program_id(&wallet, &mint, &token_program_id);

    let mut uninitialized_token_account =
        AccountBuilder::system_account(token_account_rent_exempt_balance());
    uninitialized_token_account.owner = token_program_id;
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
        token_program_id,
        CreateAtaInstructionType::CreateIdempotent {
            bump: None,
            rent_sysvar_via_account: false,
        },
    );

    harness
        .ctx
        .process_and_validate_instruction(&instruction, &[Check::err(ProgramError::IllegalOwner)]);
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn idempotent_rejects_invalid_data_token_owned_canonical_ata(token_program_id: Pubkey) {
    let harness = AtaTestHarness::new(&token_program_id).with_wallet_and_mint(1_000_000, 6);
    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let ata_address =
        get_associated_token_address_with_program_id(&wallet, &mint, &token_program_id);

    let mut invalid_data_token_account =
        AccountBuilder::system_account(token_account_rent_exempt_balance());
    invalid_data_token_account.owner = token_program_id;
    invalid_data_token_account.data = vec![0];
    harness
        .ctx
        .account_store
        .borrow_mut()
        .insert(ata_address, invalid_data_token_account);

    let instruction = build_create_ata_instruction(
        spl_associated_token_account_interface::program::id(),
        harness.payer,
        ata_address,
        wallet,
        mint,
        token_program_id,
        CreateAtaInstructionType::CreateIdempotent {
            bump: None,
            rent_sysvar_via_account: false,
        },
    );

    harness
        .ctx
        .process_and_validate_instruction(&instruction, &[Check::err(ProgramError::IllegalOwner)]);
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn idempotent_rejects_packed_uninitialized_token_owned_canonical_ata(token_program_id: Pubkey) {
    let harness = AtaTestHarness::new(&token_program_id).with_wallet_and_mint(1_000_000, 6);
    let wallet = harness.wallet.unwrap();
    let ata_address = harness.insert_token_account_at_ata_address(wallet);

    harness
        .ctx
        .account_store
        .borrow_mut()
        .get_mut(&ata_address)
        .unwrap()
        .data[TOKEN_ACCOUNT_STATE_OFFSET] = AccountState::Uninitialized as u8;

    let instruction = build_create_ata_instruction(
        spl_associated_token_account_interface::program::id(),
        harness.payer,
        ata_address,
        wallet,
        harness.mint.unwrap(),
        token_program_id,
        CreateAtaInstructionType::CreateIdempotent {
            bump: None,
            rent_sysvar_via_account: false,
        },
    );

    // Zeroing the account state byte defeats the idempotent fast-path (unpack
    // fails), so the processor falls through to the ownership check and returns
    // `IllegalOwner` because the account is still token-owned, not system-owned.
    // Note: This is the current behavior of the deployed program.
    harness
        .ctx
        .process_and_validate_instruction(&instruction, &[Check::err(ProgramError::IllegalOwner)]);
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn idempotent_rejects_wrong_owner(token_program_id: Pubkey) {
    let harness = AtaTestHarness::new(&token_program_id).with_wallet_and_mint(1_000_000, 6);
    let wrong_owner = Pubkey::new_unique();
    let ata_address = harness.insert_token_account_at_ata_address(wrong_owner);

    let instruction = build_create_ata_instruction(
        spl_associated_token_account_interface::program::id(),
        harness.payer,
        ata_address,
        harness.wallet.unwrap(),
        harness.mint.unwrap(),
        token_program_id,
        CreateAtaInstructionType::CreateIdempotent {
            bump: None,
            rent_sysvar_via_account: false,
        },
    );

    harness
        .ctx
        .process_and_validate_instruction(&instruction, &[Check::err(ProgramError::Custom(0))]);
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn idempotent_rejects_wrong_mint(token_program_id: Pubkey) {
    let harness = AtaTestHarness::new(&token_program_id).with_wallet_and_mint(1_000_000, 6);
    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let wrong_mint = Pubkey::new_unique();
    let ata_address =
        get_associated_token_address_with_program_id(&wallet, &mint, &token_program_id);

    harness.ctx.account_store.borrow_mut().insert(
        ata_address,
        AccountBuilder::token_account(&wrong_mint, &wallet, 0, &token_program_id),
    );

    let instruction = build_create_ata_instruction(
        spl_associated_token_account_interface::program::id(),
        harness.payer,
        ata_address,
        wallet,
        mint,
        token_program_id,
        CreateAtaInstructionType::CreateIdempotent {
            bump: None,
            rent_sysvar_via_account: false,
        },
    );

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[Check::err(ProgramError::InvalidAccountData)],
    );
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn idempotent_accepts_preexisting_valid_ata(token_program_id: Pubkey) {
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
        CreateAtaInstructionType::CreateIdempotent {
            bump: None,
            rent_sysvar_via_account: false,
        },
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

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn idempotent_accepts_frozen_ata(token_program_id: Pubkey) {
    let harness = AtaTestHarness::new(&token_program_id).with_wallet_and_mint(1_000_000, 6);
    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let ata_address = harness.insert_token_account_at_ata_address(wallet);

    // Set the token account state to Frozen (byte at offset 108)
    harness
        .ctx
        .account_store
        .borrow_mut()
        .get_mut(&ata_address)
        .unwrap()
        .data[TOKEN_ACCOUNT_STATE_OFFSET] = AccountState::Frozen as u8;

    let instruction = build_create_ata_instruction(
        spl_associated_token_account_interface::program::id(),
        harness.payer,
        ata_address,
        wallet,
        mint,
        token_program_id,
        CreateAtaInstructionType::CreateIdempotent {
            bump: None,
            rent_sysvar_via_account: false,
        },
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
