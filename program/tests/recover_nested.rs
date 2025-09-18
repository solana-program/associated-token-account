use {
    ata_mollusk_harness::{AccountBuilder, AtaTestHarness},
    mollusk_svm::result::Check,
    solana_instruction::AccountMeta,
    solana_keypair::Keypair,
    solana_program_error::ProgramError,
    solana_program_pack::Pack,
    solana_pubkey::Pubkey,
    solana_signer::Signer,
    spl_associated_token_account_interface::{
        address::get_associated_token_address_with_program_id, instruction,
    },
    spl_token_2022_interface::extension::StateWithExtensionsOwned,
};

const TEST_MINT_AMOUNT: u64 = 100;

fn test_recover_nested_same_mint(program_id: &Pubkey) {
    let mut harness = AtaTestHarness::new(program_id)
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();

    // Create nested ATA and mint tokens to it (not to the main, canonical ATA)
    let nested_ata = harness.create_nested_ata(owner_ata);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    // Capture pre-state for lamports transfer validation
    let wallet_pubkey = harness.wallet.as_ref().unwrap().pubkey();
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

    // Validate the recovery worked - tokens should be in the destination ATA (owner_ata)
    let destination_account = harness.get_account(owner_ata);
    let destination_state = if *program_id == spl_token_2022_interface::id() {
        let state = StateWithExtensionsOwned::<spl_token_2022_interface::state::Account>::unpack(
            destination_account.data,
        )
        .unwrap();
        state.base.amount
    } else {
        let state = spl_token_interface::state::Account::unpack(&destination_account.data).unwrap();
        state.amount
    };
    assert_eq!(destination_state, TEST_MINT_AMOUNT);
}

#[test]
fn success_same_mint_2022() {
    test_recover_nested_same_mint(&spl_token_2022_interface::id());
}

#[test]
fn success_same_mint() {
    test_recover_nested_same_mint(&spl_token_interface::id());
}

fn test_recover_nested_different_mints(program_id: &Pubkey) {
    let harness = AtaTestHarness::new(program_id)
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let owner_mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();

    // Create a second mint for the nested token
    let mut harness = harness.with_mint(0);
    let nested_mint = harness.mint.unwrap();

    // Create nested ATA and mint tokens to it
    let nested_ata = harness.create_nested_ata(owner_ata);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    // Create destination ATA for the nested token
    let destination_ata = harness.create_ata_for_owner(harness.wallet.as_ref().unwrap().pubkey());

    // Capture pre-state for lamports transfer validation
    let wallet_pubkey = harness.wallet.as_ref().unwrap().pubkey();
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
    let destination_state = if *program_id == spl_token_2022_interface::id() {
        let state = StateWithExtensionsOwned::<spl_token_2022_interface::state::Account>::unpack(
            destination_account.data,
        )
        .unwrap();
        state.base.amount
    } else {
        let state = spl_token_interface::state::Account::unpack(&destination_account.data).unwrap();
        state.amount
    };
    assert_eq!(destination_state, TEST_MINT_AMOUNT);
}

#[test]
fn success_different_mints() {
    test_recover_nested_different_mints(&spl_token_interface::id());
}

#[test]
fn success_different_mints_2022() {
    test_recover_nested_different_mints(&spl_token_2022_interface::id());
}

#[test]
fn fail_missing_wallet_signature_2022() {
    let mut harness = AtaTestHarness::new(&spl_token_2022_interface::id())
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_nested_ata(owner_ata);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    let mut recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    recover_instruction.accounts[5] =
        AccountMeta::new(harness.wallet.as_ref().unwrap().pubkey(), false);

    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::MissingRequiredSignature)],
    );
}

#[test]
fn fail_missing_wallet_signature() {
    let mut harness = AtaTestHarness::new(&spl_token_interface::id())
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_nested_ata(owner_ata);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    let mut recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    recover_instruction.accounts[5] =
        AccountMeta::new(harness.wallet.as_ref().unwrap().pubkey(), false);

    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::MissingRequiredSignature)],
    );
}

#[test]
fn fail_wrong_signer_2022() {
    let mut harness = AtaTestHarness::new(&spl_token_2022_interface::id())
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_nested_ata(owner_ata);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    // Create wrong wallet and instruction with wrong signer
    let wrong_wallet = Keypair::new();
    harness.create_ata_for_owner(wrong_wallet.pubkey());

    let recover_instruction = instruction::recover_nested(
        &wrong_wallet.pubkey(),
        &mint,
        &mint,
        &spl_token_2022_interface::id(),
    );

    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::IllegalOwner)],
    );
}

#[test]
fn fail_wrong_signer() {
    let mut harness = AtaTestHarness::new(&spl_token_interface::id())
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_nested_ata(owner_ata);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    // Test-specific logic: create wrong wallet and instruction with wrong signer
    let wrong_wallet = Keypair::new();
    harness.create_ata_for_owner(wrong_wallet.pubkey());

    let recover_instruction = instruction::recover_nested(
        &wrong_wallet.pubkey(),
        &mint,
        &mint,
        &spl_token_interface::id(),
    );

    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::IllegalOwner)],
    );
}

#[test]
fn fail_not_nested_2022() {
    let mut harness = AtaTestHarness::new(&spl_token_2022_interface::id())
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let wrong_wallet = Pubkey::new_unique();

    // Create nested ATA under wrong wallet instead of owner ATA
    let nested_ata = harness.create_ata_for_owner(wrong_wallet);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    let recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::IllegalOwner)],
    );
}

#[test]
fn fail_not_nested() {
    let mut harness = AtaTestHarness::new(&spl_token_interface::id())
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let wrong_wallet = Pubkey::new_unique();

    // Create nested ATA under wrong wallet instead of owner ATA
    let nested_ata = harness.create_ata_for_owner(wrong_wallet);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    let recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::IllegalOwner)],
    );
}
#[test]
fn fail_wrong_address_derivation_owner_2022() {
    let mut harness = AtaTestHarness::new(&spl_token_2022_interface::id())
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_nested_ata(owner_ata);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    let mut recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    let wrong_owner_address = Pubkey::new_unique();
    recover_instruction.accounts[3] = AccountMeta::new_readonly(wrong_owner_address, false);

    harness
        .ctx
        .account_store
        .borrow_mut()
        .insert(wrong_owner_address, AccountBuilder::system_account(0));

    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::InvalidSeeds)],
    );
}

#[test]
fn fail_wrong_address_derivation_owner() {
    let mut harness = AtaTestHarness::new(&spl_token_interface::id())
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_nested_ata(owner_ata);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    let mut recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    let wrong_owner_address = Pubkey::new_unique();
    recover_instruction.accounts[3] = AccountMeta::new_readonly(wrong_owner_address, false);

    harness
        .ctx
        .account_store
        .borrow_mut()
        .insert(wrong_owner_address, AccountBuilder::system_account(0));

    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::InvalidSeeds)],
    );
}

#[test]
fn fail_owner_account_does_not_exist() {
    let mut harness = AtaTestHarness::new(&spl_token_2022_interface::id())
        .with_wallet(1_000_000)
        .with_mint(0);
    // Note: deliberately NOT calling .with_ata() - owner ATA should not exist

    let mint = harness.mint.unwrap();
    let wallet_pubkey = harness.wallet.as_ref().unwrap().pubkey();
    let owner_ata_address = get_associated_token_address_with_program_id(
        &wallet_pubkey,
        &mint,
        &spl_token_2022_interface::id(),
    );

    // Create nested ATA using non-existent owner ATA address
    let nested_ata = harness.create_nested_ata(owner_ata_address);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    let recover_instruction = instruction::recover_nested(
        &wallet_pubkey,
        &mint,
        &mint,
        &spl_token_2022_interface::id(),
    );

    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::IllegalOwner)],
    );
}

#[test]
fn fail_wrong_spl_token_program() {
    let mut harness = AtaTestHarness::new(&spl_token_2022_interface::id())
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_nested_ata(owner_ata);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    // Use wrong program in instruction
    let recover_instruction = instruction::recover_nested(
        &harness.wallet.as_ref().unwrap().pubkey(),
        &mint,
        &mint,
        &spl_token_interface::id(), // Wrong program ID
    );

    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::IllegalOwner)],
    );
}

#[test]
fn fail_destination_not_wallet_ata() {
    let mut harness = AtaTestHarness::new(&spl_token_2022_interface::id())
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_nested_ata(owner_ata);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    // Create wrong destination ATA
    let wrong_wallet = Pubkey::new_unique();
    let wrong_destination_ata = harness.create_ata_for_owner(wrong_wallet);

    let mut recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    recover_instruction.accounts[2] = AccountMeta::new(wrong_destination_ata, false);

    harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::InvalidSeeds)],
    );
}
