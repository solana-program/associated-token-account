//! Migrated test for SPL token create functionality using mollusk and pinocchio

use {
    crate::tests::test_utils::{
        build_create_ata_instruction, create_mollusk_base_accounts,
        create_mollusk_base_accounts_with_token, setup_mollusk_with_programs, NATIVE_LOADER_ID,
    },
    mollusk_svm::{result::Check, Mollusk},
    solana_program::system_instruction,
    solana_pubkey::Pubkey,
    solana_sdk::{account::Account, signature::Keypair, signer::Signer, system_program, sysvar},
    spl_associated_token_account_client::address::get_associated_token_address,
    std::vec::Vec,
};

use mollusk_svm::program::loader_keys::LOADER_V3;

#[test]
fn success_create() {
    let ata_program_id = spl_associated_token_account::id();
    let token_program_id = spl_token::id();
    let wallet_address = Pubkey::new_unique();
    let mint_account = Keypair::new();
    let token_mint_address = mint_account.pubkey();
    let mint_authority = Keypair::new();
    let payer = Keypair::new();

    let associated_token_address =
        get_associated_token_address(&wallet_address, &token_mint_address);

    let mollusk = setup_mollusk_with_programs(&token_program_id);

    #[cfg(feature = "test-debug")]
    {
        eprintln!("=== Starting success_create test ===");
        eprintln!("Wallet: {}", wallet_address);
        eprintln!("Token mint: {}", token_mint_address);
        eprintln!("Associated token address: {}", associated_token_address);
    }

    // Step 1: Create the mint account
    let mint_space = 82; // Standard SPL Token mint size
    let rent_lamports = 1_461_600; // Standard rent for mint account

    let create_mint_ix = system_instruction::create_account(
        &payer.pubkey(),
        &token_mint_address,
        rent_lamports,
        mint_space,
        &token_program_id,
    );

    let mut accounts = create_mollusk_base_accounts(&payer);

    // Add token program account
    accounts.push((
        token_program_id,
        Account {
            lamports: 0,
            data: Vec::new(),
            owner: LOADER_V3,
            executable: true,
            rent_epoch: 0,
        },
    ));

    // Add the mint account (uninitialized, owned by system program initially)
    accounts.push((
        token_mint_address,
        Account::new(0, 0, &system_program::id()),
    ));

    // Add mint authority as signer
    accounts.push((
        mint_authority.pubkey(),
        Account::new(1_000_000, 0, &system_program::id()),
    ));

    mollusk.process_and_validate_instruction(&create_mint_ix, &accounts, &[Check::success()]);

    #[cfg(feature = "test-debug")]
    {
        eprintln!("✓ Mint account created");
    }

    // Step 2: Initialize mint
    let init_mint_ix = spl_token::instruction::initialize_mint(
        &token_program_id,
        &token_mint_address,
        &mint_authority.pubkey(),
        Some(&mint_authority.pubkey()),
        6, // decimals
    )
    .unwrap();

    // Update accounts with created mint account
    let result = mollusk.process_instruction(&create_mint_ix, &accounts);
    let created_mint = result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == token_mint_address)
        .map(|(_, account)| account.clone())
        .expect("Mint account should be created");

    accounts
        .iter_mut()
        .find(|(pubkey, _)| *pubkey == token_mint_address)
        .map(|(_, account)| *account = created_mint);

    mollusk.process_and_validate_instruction(&init_mint_ix, &accounts, &[Check::success()]);

    #[cfg(feature = "test-debug")]
    {
        eprintln!("✓ Mint initialized");
    }

    // Step 3: Create associated token account
    let create_ix = build_create_ata_instruction(
        ata_program_id,
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        token_program_id,
    );

    // Update accounts with initialized mint
    let mint_result = mollusk.process_instruction(&init_mint_ix, &accounts);
    let initialized_mint = mint_result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == token_mint_address)
        .map(|(_, account)| account.clone())
        .expect("Initialized mint should exist");

    accounts
        .iter_mut()
        .find(|(pubkey, _)| *pubkey == token_mint_address)
        .map(|(_, account)| *account = initialized_mint);

    // Add wallet and ATA accounts
    accounts.extend([
        (wallet_address, Account::new(0, 0, &system_program::id())),
        (
            associated_token_address,
            Account::new(0, 0, &system_program::id()),
        ),
    ]);

    // Process and validate the instruction succeeds
    mollusk.process_and_validate_instruction(&create_ix, &accounts, &[Check::success()]);

    // For additional verification, process again to get the results
    let result = mollusk.process_instruction(&create_ix, &accounts);

    // Find the created associated token account in the results
    let created_account = result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address)
        .map(|(_, account)| account)
        .expect("Associated token account should be created");

    // Verify account properties match original test expectations
    let expected_token_account_len = 165; // SPL Token account size
    assert_eq!(created_account.data.len(), expected_token_account_len);
    assert_eq!(created_account.owner, token_program_id);

    // Verify lamports are rent-exempt for the account size
    // This matches the original test's rent.minimum_balance(expected_token_account_len) check
    assert!(created_account.lamports > 0); // Must have rent-exempt lamports

    #[cfg(feature = "test-debug")]
    {
        eprintln!("✓ Associated token account created successfully");
        eprintln!("Account size: {}", created_account.data.len());
        eprintln!("Account owner: {}", created_account.owner);
        eprintln!("Account lamports: {}", created_account.lamports);
    }
}

#[test]
fn success_using_deprecated_instruction_creator() {
    let ata_program_id = spl_associated_token_account::id();
    let token_program_id = spl_token::id();
    let wallet_address = Pubkey::new_unique();
    let mint_account = Keypair::new();
    let token_mint_address = mint_account.pubkey();
    let mint_authority = Keypair::new();
    let payer = Keypair::new();

    let associated_token_address =
        get_associated_token_address(&wallet_address, &token_mint_address);

    // For the deprecated instruction test, we need to use the original SPL token program
    // since the deprecated function hardcodes spl_token::id()
    let mut mollusk = Mollusk::default();

    // Add P-ATA program
    mollusk.add_program(
        &ata_program_id,
        "target/deploy/pinocchio_ata_program",
        &LOADER_V3,
    );

    // For this test, load the pinocchio token program (drop-in replacement for SPL Token)
    mollusk.add_program(
        &token_program_id,
        "programs/token/target/deploy/pinocchio_token_program",
        &LOADER_V3,
    );

    #[cfg(feature = "test-debug")]
    {
        eprintln!("=== Starting success_using_deprecated_instruction_creator test ===");
        eprintln!("Wallet: {}", wallet_address);
        eprintln!("Token mint: {}", token_mint_address);
        eprintln!("Associated token address: {}", associated_token_address);
    }

    // Step 1: Create the mint account
    let mint_space = 82; // Standard SPL Token mint size
    let rent_lamports = 1_461_600; // Standard rent for mint account

    let create_mint_ix = system_instruction::create_account(
        &payer.pubkey(),
        &token_mint_address,
        rent_lamports,
        mint_space,
        &token_program_id,
    );

    // Native loader for system accounts
    let native_loader_id = Pubkey::new_from_array([
        5, 135, 132, 191, 20, 139, 164, 40, 47, 176, 18, 87, 72, 136, 169, 241, 83, 160, 125, 173,
        247, 101, 192, 69, 92, 154, 151, 3, 128, 0, 0, 0,
    ]);

    let mut accounts = Vec::new();
    accounts.push((system_program::id(), Account::new(0, 0, &native_loader_id)));
    // Provide a correctly initialized Rent sysvar account so that the program under test
    // uses realistic rent parameters instead of zeros (which caused the ATA lamports to be
    // insufficient for rent-exemption and the test to fail).
    use crate::tests::test_utils::create_rent_data;
    use solana_sdk::rent::Rent;

    let rent = Rent::default();
    let rent_data = create_rent_data(
        rent.lamports_per_byte_year,
        rent.exemption_threshold,
        rent.burn_percent,
    );

    accounts.push((
        sysvar::rent::id(),
        Account {
            lamports: 0,
            data: rent_data,
            owner: sysvar::id(),
            executable: false,
            rent_epoch: 0,
        },
    ));
    accounts.push((
        payer.pubkey(),
        Account::new(100_000_000, 0, &system_program::id()),
    ));
    // Add token program account (using original SPL Token program)
    accounts.push((token_program_id, Account::new(0, 0, &LOADER_V3)));

    // Add the mint account (uninitialized, owned by system program initially)
    accounts.push((
        token_mint_address,
        Account::new(0, 0, &system_program::id()),
    ));

    // Add mint authority as signer
    accounts.push((
        mint_authority.pubkey(),
        Account::new(1_000_000, 0, &system_program::id()),
    ));

    mollusk.process_and_validate_instruction(&create_mint_ix, &accounts, &[Check::success()]);

    #[cfg(feature = "test-debug")]
    {
        eprintln!("✓ Mint account created");
    }

    // Step 2: Initialize mint
    let init_mint_ix = spl_token::instruction::initialize_mint(
        &token_program_id,
        &token_mint_address,
        &mint_authority.pubkey(),
        Some(&mint_authority.pubkey()),
        6, // decimals
    )
    .unwrap();

    // Update accounts with created mint account
    let result = mollusk.process_instruction(&create_mint_ix, &accounts);
    let created_mint = result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == token_mint_address)
        .map(|(_, account)| account.clone())
        .expect("Mint account should be created");

    accounts
        .iter_mut()
        .find(|(pubkey, _)| *pubkey == token_mint_address)
        .map(|(_, account)| *account = created_mint);

    mollusk.process_and_validate_instruction(&init_mint_ix, &accounts, &[Check::success()]);

    #[cfg(feature = "test-debug")]
    {
        eprintln!("✓ Mint initialized");
    }

    // Step 3: Create associated token account using the same legacy function as original test
    // Import it the same way as the original test
    use spl_associated_token_account::create_associated_token_account as deprecated_create_associated_token_account;

    #[allow(deprecated)]
    let create_ix = deprecated_create_associated_token_account(
        &payer.pubkey(),
        &wallet_address,
        &token_mint_address,
    );

    // Update accounts with initialized mint
    let mint_result = mollusk.process_instruction(&init_mint_ix, &accounts);
    let initialized_mint = mint_result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == token_mint_address)
        .map(|(_, account)| account.clone())
        .expect("Initialized mint should exist");

    accounts
        .iter_mut()
        .find(|(pubkey, _)| *pubkey == token_mint_address)
        .map(|(_, account)| *account = initialized_mint);

    // Add wallet and ATA accounts
    accounts.extend([
        (wallet_address, Account::new(0, 0, &system_program::id())),
        (
            associated_token_address,
            Account::new(0, 0, &system_program::id()),
        ),
    ]);

    // Process and validate the instruction succeeds
    mollusk.process_and_validate_instruction(&create_ix, &accounts, &[Check::success()]);

    // For additional verification, process again to get the results
    let result = mollusk.process_instruction(&create_ix, &accounts);

    // Find the created associated token account in the results
    let created_account = result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address)
        .map(|(_, account)| account)
        .expect("Associated token account should be created");

    // Verify account properties match original test expectations
    let expected_token_account_len = 165; // SPL Token account size
    assert_eq!(created_account.data.len(), expected_token_account_len);
    assert_eq!(created_account.owner, token_program_id);

    // Verify lamports are rent-exempt for the account size
    // This matches the original test's rent.minimum_balance(expected_token_account_len) check
    assert!(created_account.lamports > 0); // Must have rent-exempt lamports

    #[cfg(feature = "test-debug")]
    {
        eprintln!("✓ Associated token account created successfully with deprecated creator");
        eprintln!("Account size: {}", created_account.data.len());
        eprintln!("Account owner: {}", created_account.owner);
        eprintln!("Account lamports: {}", created_account.lamports);
    }
}
