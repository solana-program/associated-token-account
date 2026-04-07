use {
    mollusk_svm::result::Check,
    solana_instruction::AccountMeta,
    solana_program_error::ProgramError,
    solana_pubkey::Pubkey,
    spl_associated_token_account_interface::{
        address::get_associated_token_address_with_program_id, instruction,
    },
    spl_associated_token_account_mollusk_harness::{AccountBuilder, AtaTestHarness},
    spl_token_2022_interface::extension::StateWithExtensionsOwned,
    test_case::test_case,
};

const TEST_MINT_AMOUNT: u64 = 100;

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn fail_not_enough_accounts(token_program_id: Pubkey) {
    let mut harness = AtaTestHarness::new(&token_program_id)
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    harness.create_ata_for_owner(owner_ata, 1_000_000);

    let mut recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    recover_instruction.accounts.truncate(6);

    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::NotEnoughAccountKeys)],
    );
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn fail_wrong_address_derivation_owner(token_program_id: Pubkey) {
    let mut harness = AtaTestHarness::new(&token_program_id)
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_ata_for_owner(owner_ata, 1_000_000);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    let mut recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    let wrong_owner_address = Pubkey::new_unique();
    recover_instruction.accounts[3] = AccountMeta::new_readonly(wrong_owner_address, false);

    harness.ensure_accounts_with_lamports(&[(wrong_owner_address, 1_000_000)]);

    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::InvalidSeeds)],
    );
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn fail_wrong_nested_address_derivation(token_program_id: Pubkey) {
    let mut harness = AtaTestHarness::new(&token_program_id)
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_ata_for_owner(owner_ata, 1_000_000);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    let mut recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    let wrong_nested_address = Pubkey::new_unique();
    recover_instruction.accounts[0] = AccountMeta::new(wrong_nested_address, false);

    harness.ensure_accounts_with_lamports(&[(wrong_nested_address, 1_000_000)]);

    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::InvalidSeeds)],
    );
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn fail_destination_not_wallet_ata(token_program_id: Pubkey) {
    let mut harness = AtaTestHarness::new(&token_program_id)
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_ata_for_owner(owner_ata, 1_000_000);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    // Create wrong destination ATA
    let wrong_wallet = Pubkey::new_unique();
    let wrong_destination_ata = harness.create_ata_for_owner(wrong_wallet, 1_000_000);

    let mut recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    recover_instruction.accounts[2] = AccountMeta::new(wrong_destination_ata, false);

    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::InvalidSeeds)],
    );
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn fail_missing_wallet_signature(token_program_id: Pubkey) {
    let mut harness = AtaTestHarness::new(&token_program_id)
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_ata_for_owner(owner_ata, 1_000_000);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    let mut recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    recover_instruction.accounts[5] = AccountMeta::new(harness.wallet.unwrap(), false);

    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::MissingRequiredSignature)],
    );
}

#[test_case(spl_token_2022_interface::id(), spl_token_interface::id())]
#[test_case(spl_token_interface::id(), spl_token_2022_interface::id())]
fn fail_wrong_token_program(setup_program_id: Pubkey, instruction_program_id: Pubkey) {
    let mut harness = AtaTestHarness::new(&setup_program_id)
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_ata_for_owner(owner_ata, 1_000_000);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    // Use wrong program in instruction
    let recover_instruction = instruction::recover_nested(
        &harness.wallet.unwrap(),
        &mint,
        &mint,
        &instruction_program_id,
    );

    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::IllegalOwner)],
    );
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn fail_owner_account_does_not_exist(token_program_id: Pubkey) {
    let mut harness = AtaTestHarness::new(&token_program_id)
        .with_wallet(1_000_000)
        .with_mint(0);
    // Note: deliberately NOT calling .with_ata() - owner ATA should not exist

    let mint = harness.mint.unwrap();
    let wallet_pubkey = harness.wallet.unwrap();
    let owner_ata_address =
        get_associated_token_address_with_program_id(&wallet_pubkey, &mint, &token_program_id);

    // Create nested ATA using non-existent owner ATA address
    let nested_ata = harness.create_ata_for_owner(owner_ata_address, 0);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    let recover_instruction =
        instruction::recover_nested(&wallet_pubkey, &mint, &mint, &token_program_id);

    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::IllegalOwner)],
    );
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn fail_owner_ata_invalid_data(token_program_id: Pubkey) {
    let mut harness = AtaTestHarness::new(&token_program_id)
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_ata_for_owner(owner_ata, 1_000_000);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    harness
        .ctx
        .account_store
        .borrow_mut()
        .get_mut(&owner_ata)
        .unwrap()
        .data = vec![0_u8; 3];

    let recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::InvalidAccountData)],
    );
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn fail_owner_ata_wrong_internal_owner(token_program_id: Pubkey) {
    let mut harness = AtaTestHarness::new(&token_program_id)
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_ata_for_owner(owner_ata, 1_000_000);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    // Overwrite owner ATA with a token account whose internal .owner != wallet
    let wrong_owner = Pubkey::new_unique();
    let tampered = AccountBuilder::token_account(&mint, &wrong_owner, 0, &token_program_id);
    harness
        .ctx
        .account_store
        .borrow_mut()
        .insert(owner_ata, tampered);

    let recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        // AssociatedTokenAccountError::InvalidOwner == Custom(0)
        &[Check::err(ProgramError::Custom(0))],
    );
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn fail_nested_ata_wrong_program_owner(token_program_id: Pubkey) {
    let mut harness = AtaTestHarness::new(&token_program_id)
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_ata_for_owner(owner_ata, 1_000_000);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    harness
        .ctx
        .account_store
        .borrow_mut()
        .insert(nested_ata, AccountBuilder::system_account(1_000_000));

    let recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::IllegalOwner)],
    );
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn fail_nested_ata_invalid_data(token_program_id: Pubkey) {
    let mut harness = AtaTestHarness::new(&token_program_id)
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_ata_for_owner(owner_ata, 1_000_000);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    harness
        .ctx
        .account_store
        .borrow_mut()
        .get_mut(&nested_ata)
        .unwrap()
        .data = vec![0_u8; 3];

    let recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::InvalidAccountData)],
    );
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn fail_nested_ata_wrong_internal_owner(token_program_id: Pubkey) {
    let mut harness = AtaTestHarness::new(&token_program_id)
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_ata_for_owner(owner_ata, 1_000_000);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    // Overwrite nested ATA with a token account whose internal .owner != owner_ata
    let wrong_owner = Pubkey::new_unique();
    let tampered =
        AccountBuilder::token_account(&mint, &wrong_owner, TEST_MINT_AMOUNT, &token_program_id);
    harness
        .ctx
        .account_store
        .borrow_mut()
        .insert(nested_ata, tampered);

    let recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        // AssociatedTokenAccountError::InvalidOwner == Custom(0)
        &[Check::err(ProgramError::Custom(0))],
    );
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn fail_nested_mint_wrong_program_owner(token_program_id: Pubkey) {
    // Must use different mints so that tampering nested_mint (accounts[1])
    // does not also tamper owner_mint (accounts[4]).
    let harness = AtaTestHarness::new(&token_program_id)
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let owner_mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();

    let mut harness = harness.with_mint(0);
    let nested_mint = harness.mint.unwrap();

    let nested_ata = harness.create_ata_for_owner(owner_ata, 1_000_000);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);
    harness.create_ata_for_owner(harness.wallet.unwrap(), 1_000_000);

    // Change the nested mint's Solana account owner away from token program
    harness
        .ctx
        .account_store
        .borrow_mut()
        .get_mut(&nested_mint)
        .unwrap()
        .owner = Pubkey::new_unique();

    let recover_instruction = harness.build_recover_nested_instruction(owner_mint, nested_mint);
    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::IllegalOwner)],
    );
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn fail_nested_mint_invalid_data(token_program_id: Pubkey) {
    let mut harness = AtaTestHarness::new(&token_program_id)
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_ata_for_owner(owner_ata, 1_000_000);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    harness
        .ctx
        .account_store
        .borrow_mut()
        .get_mut(&mint)
        .unwrap()
        .data = vec![0_u8; 3];

    let recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::InvalidAccountData)],
    );
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn success_same_mint(token_program_id: Pubkey) {
    let mut harness = AtaTestHarness::new(&token_program_id)
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();

    // Create nested ATA and mint tokens to it (not to the main, canonical ATA)
    let nested_ata = harness.create_ata_for_owner(owner_ata, 1_000_000);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    // Capture pre-state for lamports transfer validation
    let wallet_pubkey = harness.wallet.unwrap();
    let pre_wallet_lamports = {
        let store = harness.ctx.account_store.borrow();
        store.get(&wallet_pubkey).unwrap().lamports
    };
    let nested_lamports = harness.get_account(nested_ata).lamports;

    // Build and execute recover instruction
    let recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[
            Check::success(),
            // Wallet received nested account lamports
            Check::account(&wallet_pubkey)
                .lamports(pre_wallet_lamports.checked_add(nested_lamports).unwrap())
                .build(),
            // Nested account has no lamports
            Check::account(&nested_ata).lamports(0).build(),
            // Nested account is closed
            Check::account(&nested_ata).closed().build(),
        ],
    );

    // Validate the recovery worked. Tokens should be in the destination ATA (owner_ata).
    let destination_account = harness.get_account(owner_ata);
    let destination_amount =
        StateWithExtensionsOwned::<spl_token_2022_interface::state::Account>::unpack(
            destination_account.data,
        )
        .unwrap()
        .base
        .amount;
    assert_eq!(destination_amount, TEST_MINT_AMOUNT);
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn success_zero_amount(program_id: Pubkey) {
    let mut harness = AtaTestHarness::new(&program_id)
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_ata_for_owner(owner_ata, 1_000_000);

    let wallet_pubkey = harness.wallet.unwrap();
    let pre_wallet_lamports = {
        let store = harness.ctx.account_store.borrow();
        store.get(&wallet_pubkey).unwrap().lamports
    };
    let nested_lamports = harness.get_account(nested_ata).lamports;

    let recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[
            Check::success(),
            Check::account(&wallet_pubkey)
                .lamports(pre_wallet_lamports.checked_add(nested_lamports).unwrap())
                .build(),
            Check::account(&nested_ata).lamports(0).build(),
            Check::account(&nested_ata).closed().build(),
        ],
    );

    let destination_account = harness.get_account(owner_ata);
    let destination_amount =
        StateWithExtensionsOwned::<spl_token_2022_interface::state::Account>::unpack(
            destination_account.data,
        )
        .unwrap()
        .base
        .amount;
    assert_eq!(destination_amount, 0);
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn success_different_mints(token_program_id: Pubkey) {
    let harness = AtaTestHarness::new(&token_program_id)
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let owner_mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();

    // Create a second mint for the nested token
    let mut harness = harness.with_mint(0);
    let nested_mint = harness.mint.unwrap();

    // Create nested ATA and mint tokens to it
    let nested_ata = harness.create_ata_for_owner(owner_ata, 1_000_000);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    // Create destination ATA for the nested token
    let destination_ata = harness.create_ata_for_owner(harness.wallet.unwrap(), 1_000_000);

    // Capture pre-state for lamports transfer validation
    let wallet_pubkey = harness.wallet.unwrap();
    let pre_wallet_lamports = {
        let store = harness.ctx.account_store.borrow();
        store.get(&wallet_pubkey).unwrap().lamports
    };
    let nested_lamports = harness.get_account(nested_ata).lamports;

    // Build and execute recover instruction
    let recover_instruction = harness.build_recover_nested_instruction(owner_mint, nested_mint);
    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[
            Check::success(),
            // Wallet received nested account lamports
            Check::account(&wallet_pubkey)
                .lamports(pre_wallet_lamports.checked_add(nested_lamports).unwrap())
                .build(),
            // Nested account has no lamports
            Check::account(&nested_ata).lamports(0).build(),
            // Nested account is closed
            Check::account(&nested_ata).closed().build(),
        ],
    );

    // Validate the recovery worked - tokens should be in the destination ATA
    let destination_account = harness.get_account(destination_ata);
    let destination_amount =
        StateWithExtensionsOwned::<spl_token_2022_interface::state::Account>::unpack(
            destination_account.data,
        )
        .unwrap()
        .base
        .amount;
    assert_eq!(destination_amount, TEST_MINT_AMOUNT);
}
