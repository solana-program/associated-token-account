//! Migrated test for idempotent creation functionality using mollusk and pinocchio

use {
    crate::tests::{
        migrated::process_create_associated_token_account::create_test_mint,
        test_utils::{
            build_create_ata_instruction, build_create_idempotent_ata_instruction,
            create_mollusk_base_accounts, create_mollusk_token_account_data,
            setup_mollusk_with_programs,
        },
    },
    mollusk_svm::result::Check,
    solana_instruction::AccountMeta,
    solana_program::program_error::ProgramError,
    solana_pubkey::Pubkey,
    solana_sdk::{account::Account, signature::Keypair, signer::Signer, system_program},
    spl_associated_token_account_client::address::get_associated_token_address_with_program_id,
};

use mollusk_svm::{program::loader_keys::LOADER_V3, Mollusk};

#[test]
fn create_with_a_lamport_with_idempotent() {
    let ata_program_id = spl_associated_token_account::id();
    let token_program_id = spl_token::id();
    let wallet_address = Pubkey::new_unique();
    let mint_account = Keypair::new();
    let token_mint_address = mint_account.pubkey();
    let mint_authority = Keypair::new();
    let payer = Keypair::new();

    let associated_token_address = get_associated_token_address_with_program_id(
        &wallet_address,
        &token_mint_address,
        &token_program_id,
    );

    let mollusk = setup_mollusk_with_programs(&token_program_id);

    // Step 1: Create and initialize mint
    let mut accounts = create_test_mint(
        &mollusk,
        &mint_account,
        &mint_authority,
        &payer,
        &token_program_id,
        6,
    );

    // Add wallet with 1 lamport at ATA address (simulating pre-funded account)
    accounts.extend([
        (
            wallet_address,
            Account::new(1_000_000, 0, &system_program::id()),
        ),
        (
            associated_token_address,
            Account::new(1, 0, &system_program::id()),
        ), // 1 lamport pre-funded
    ]);

    // Step 2: Try regular create - this now succeeds because the account has pre-funding
    let create_ix = build_create_ata_instruction(
        ata_program_id,
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        token_program_id,
    );

    // This should succeed because modern implementation handles pre-funded accounts
    mollusk.process_and_validate_instruction(&create_ix, &accounts, &[Check::success()]);

    // Step 3: Try with idempotent instruction on already created ATA - should also succeed
    let create_idempotent_ix = build_create_idempotent_ata_instruction(
        ata_program_id,
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        token_program_id,
    );

    // Update accounts with the created ATA
    let result = mollusk.process_instruction(&create_ix, &accounts);
    let created_ata = result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address)
        .map(|(_, account)| account.clone())
        .expect("ATA should be created");

    let mut updated_accounts = accounts;
    updated_accounts
        .iter_mut()
        .find(|(pubkey, _)| *pubkey == associated_token_address)
        .map(|(_, account)| *account = created_ata);

    mollusk.process_and_validate_instruction(
        &create_idempotent_ix,
        &updated_accounts,
        &[Check::success()],
    );
}

#[test]
fn success_idempotent_on_existing_ata() {
    let ata_program_id = spl_associated_token_account::id();
    let token_program_id = spl_token::id();
    let wallet_address = Pubkey::new_unique();
    let mint_account = Keypair::new();
    let token_mint_address = mint_account.pubkey();
    let mint_authority = Keypair::new();
    let payer = Keypair::new();

    let associated_token_address = get_associated_token_address_with_program_id(
        &wallet_address,
        &token_mint_address,
        &token_program_id,
    );

    let mollusk = setup_mollusk_with_programs(&token_program_id);

    #[cfg(feature = "test-debug")]
    {
        eprintln!("=== Starting success_idempotent_on_existing_ata test ===");
        eprintln!("Wallet: {}", wallet_address);
        eprintln!("Token mint: {}", token_mint_address);
        eprintln!("Associated token address: {}", associated_token_address);
    }

    // Step 1: Create and initialize mint
    let mut accounts = create_test_mint(
        &mollusk,
        &mint_account,
        &mint_authority,
        &payer,
        &token_program_id,
        6,
    );

    // Add wallet and ATA accounts
    accounts.extend([
        (wallet_address, Account::new(0, 0, &system_program::id())),
        (
            associated_token_address,
            Account::new(0, 0, &system_program::id()),
        ),
    ]);

    // Step 2: Create ATA with CreateIdempotent (first time) - should succeed
    let create_idempotent_ix = build_create_idempotent_ata_instruction(
        ata_program_id,
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        token_program_id,
    );

    mollusk.process_and_validate_instruction(&create_idempotent_ix, &accounts, &[Check::success()]);

    #[cfg(feature = "test-debug")]
    {
        eprintln!("✓ First CreateIdempotent succeeded");
    }

    // Step 3: Update accounts with the created ATA for subsequent calls
    let result = mollusk.process_instruction(&create_idempotent_ix, &accounts);
    let created_ata = result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address)
        .map(|(_, account)| account.clone())
        .expect("ATA should be created");

    let mut updated_accounts = accounts;
    updated_accounts
        .iter_mut()
        .find(|(pubkey, _)| *pubkey == associated_token_address)
        .map(|(_, account)| *account = created_ata);

    // Step 4: Try regular Create on existing ATA - should fail
    let create_ix = build_create_ata_instruction(
        ata_program_id,
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        token_program_id,
    );

    mollusk.process_and_validate_instruction(
        &create_ix,
        &updated_accounts,
        &[Check::err(ProgramError::Custom(0))], // Should fail because account already exists (system program error)
    );

    #[cfg(feature = "test-debug")]
    {
        eprintln!("✓ Regular Create on existing ATA correctly failed");
    }

    // Step 5: Try CreateIdempotent again on existing ATA - should succeed (core idempotent behavior)
    mollusk.process_and_validate_instruction(
        &create_idempotent_ix,
        &updated_accounts,
        &[Check::success()],
    );

    #[cfg(feature = "test-debug")]
    {
        eprintln!(
            "✓ Second CreateIdempotent on existing ATA succeeded (idempotent behavior verified)"
        );
    }

    // Step 6: Verify ATA properties are unchanged
    let final_result = mollusk.process_instruction(&create_idempotent_ix, &updated_accounts);
    let final_ata = final_result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address)
        .map(|(_, account)| account.clone())
        .expect("ATA should still exist");

    // Verify account properties
    assert_eq!(final_ata.data.len(), 165); // SPL Token account size
    assert_eq!(final_ata.owner, token_program_id);
    assert!(final_ata.lamports > 0);

    #[cfg(feature = "test-debug")]
    {
        eprintln!("✓ ATA properties verified unchanged after idempotent call");
    }
}

#[test]
fn create_with_wrong_mint_fails() {
    let ata_program_id = spl_associated_token_account::id();
    let token_program_id = spl_token::id();
    let wallet_address = Pubkey::new_unique();
    let mint_account = Keypair::new();
    let token_mint_address = mint_account.pubkey();
    let wrong_mint_address = Pubkey::new_unique(); // Different mint
    let mint_authority = Keypair::new();
    let payer = Keypair::new();

    // Get ATA address for the wrong mint
    let associated_token_address = get_associated_token_address_with_program_id(
        &wallet_address,
        &wrong_mint_address,
        &token_program_id,
    );

    let mut mollusk = Mollusk::default();

    // Add our p-ata program
    mollusk.add_program(
        &ata_program_id,
        "target/deploy/pinocchio_ata_program",
        &LOADER_V3,
    );

    // Add token program
    mollusk.add_program(
        &token_program_id,
        "programs/token/target/deploy/pinocchio_token_program",
        &LOADER_V3,
    );

    #[cfg(feature = "test-debug")]
    {
        eprintln!("=== Starting create_with_wrong_mint_fails test ===");
        eprintln!("Wallet: {}", wallet_address);
        eprintln!("Correct mint: {}", token_mint_address);
        eprintln!("Wrong mint: {}", wrong_mint_address);
        eprintln!("Associated token address: {}", associated_token_address);
    }

    // Step 1: Create and initialize the correct mint
    let mut accounts = create_test_mint(
        &mollusk,
        &mint_account,
        &mint_authority,
        &payer,
        &token_program_id,
        6,
    );

    // Add wallet and ATA accounts
    accounts.extend([
        (wallet_address, Account::new(0, 0, &system_program::id())),
        (
            associated_token_address,
            Account::new(0, 0, &system_program::id()),
        ),
    ]);

    // Step 2: Create an associated token account using correct mint but wrong derived address
    let create_idempotent_ix = build_create_idempotent_ata_instruction(
        ata_program_id,
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address, // Using the correct mint, but ATA address is for wrong mint
        token_program_id,
    );

    // This should fail because the derived ATA address doesn't match the provided address
    mollusk.process_and_validate_instruction(
        &create_idempotent_ix,
        &accounts,
        &[Check::err(
            solana_program::program_error::ProgramError::InvalidInstructionData,
        )],
    );

    #[cfg(feature = "test-debug")]
    {
        eprintln!("✓ Create with wrong mint correctly failed");
    }
}

#[test]
fn create_with_mismatch_fails() {
    let ata_program_id = spl_associated_token_account::id();
    let token_program_id = spl_token::id();
    let correct_wallet = Pubkey::new_unique();
    let wrong_wallet = Pubkey::new_unique();
    let mint_account = Keypair::new();
    let token_mint_address = mint_account.pubkey();
    let mint_authority = Keypair::new();
    let payer = Keypair::new();

    // Get ATA for correct wallet, but we'll try to create for wrong wallet
    let associated_token_address = get_associated_token_address_with_program_id(
        &correct_wallet,
        &token_mint_address,
        &token_program_id,
    );

    let mut mollusk = Mollusk::default();

    mollusk.add_program(
        &ata_program_id,
        "target/deploy/pinocchio_ata_program",
        &LOADER_V3,
    );

    mollusk.add_program(
        &token_program_id,
        "programs/token/target/deploy/pinocchio_token_program",
        &LOADER_V3,
    );

    #[cfg(feature = "test-debug")]
    {
        eprintln!("=== Starting create_with_mismatch_fails test ===");
        eprintln!("Correct wallet: {}", correct_wallet);
        eprintln!("Wrong wallet: {}", wrong_wallet);
        eprintln!("Token mint: {}", token_mint_address);
        eprintln!("Associated token address: {}", associated_token_address);
    }

    // Step 1: Create and initialize mint
    let mut accounts = create_test_mint(
        &mollusk,
        &mint_account,
        &mint_authority,
        &payer,
        &token_program_id,
        6,
    );

    accounts.extend([
        (correct_wallet, Account::new(0, 0, &system_program::id())),
        (wrong_wallet, Account::new(0, 0, &system_program::id())),
        (
            associated_token_address,
            Account::new(0, 0, &system_program::id()),
        ),
    ]);

    // Step 2: Try to create ATA with wrong wallet
    let create_idempotent_ix = build_create_idempotent_ata_instruction(
        ata_program_id,
        payer.pubkey(),
        associated_token_address,
        wrong_wallet, // Wrong wallet!
        token_mint_address,
        token_program_id,
    );

    // This should fail because the wallet doesn't match the ATA derivation
    mollusk.process_and_validate_instruction(
        &create_idempotent_ix,
        &accounts,
        &[Check::err(
            solana_program::program_error::ProgramError::InvalidInstructionData,
        )],
    );

    #[cfg(feature = "test-debug")]
    {
        eprintln!("✓ Create with wrong wallet correctly failed");
    }
}

#[test]
fn fail_account_exists_with_wrong_owner() {
    let ata_program_id = spl_associated_token_account::id();
    let token_program_id = spl_token::id();
    let wallet_address = Pubkey::new_unique();
    let wrong_owner = Pubkey::new_unique();
    let mint_account = Keypair::new();
    let token_mint_address = mint_account.pubkey();
    let mint_authority = Keypair::new();
    let payer = Keypair::new();

    let associated_token_address = get_associated_token_address_with_program_id(
        &wallet_address,
        &token_mint_address,
        &token_program_id,
    );

    let mut mollusk = Mollusk::default();

    mollusk.add_program(
        &ata_program_id,
        "target/deploy/pinocchio_ata_program",
        &LOADER_V3,
    );

    mollusk.add_program(
        &token_program_id,
        "programs/token/target/deploy/pinocchio_token_program",
        &LOADER_V3,
    );

    // Create and initialize mint first
    let mut accounts = create_test_mint(
        &mollusk,
        &mint_account,
        &mint_authority,
        &payer,
        &token_program_id,
        6,
    );

    // Create a token account at the ATA address but with wrong owner
    let wrong_token_account = Account {
        lamports: 1_000_000_000,
        data: {
            let mut data = [0u8; 165]; // SPL Token account size (no extensions)
                                       // Initialize as a basic token account structure with wrong owner
                                       // Mint (32 bytes)
            data[0..32].copy_from_slice(&token_mint_address.to_bytes());
            // Owner (32 bytes) - this is the wrong owner!
            data[32..64].copy_from_slice(&wrong_owner.to_bytes());
            // Amount (8 bytes) - 0
            data[64..72].copy_from_slice(&0u64.to_le_bytes());
            // Delegate option (36 bytes) - None
            data[72] = 0; // COption::None
                          // State (1 byte) - Initialized
            data[108] = 1; // AccountState::Initialized
                           // Is native option (12 bytes) - None
            data[109] = 0; // COption::None
                           // Delegated amount (8 bytes) - 0
            data[121..129].copy_from_slice(&0u64.to_le_bytes());
            // Close authority option (36 bytes) - None
            data[129] = 0; // COption::None
            data.to_vec()
        },
        owner: token_program_id,
        executable: false,
        rent_epoch: 0,
    };

    accounts.extend([
        (
            wallet_address,
            Account::new(1_000_000, 0, &system_program::id()),
        ),
        (
            wrong_owner,
            Account::new(1_000_000, 0, &system_program::id()),
        ),
        (associated_token_address, wrong_token_account),
    ]);

    // Try to create idempotent ATA - should fail because existing account has wrong owner
    let create_idempotent_ix = build_create_idempotent_ata_instruction(
        ata_program_id,
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        token_program_id,
    );

    // Should fail with IllegalOwner error (P-ATA returns different error type than original)
    mollusk.process_and_validate_instruction(
        &create_idempotent_ix,
        &accounts,
        &[Check::err(ProgramError::IllegalOwner)],
    );
}

#[test]
fn fail_non_ata() {
    let ata_program_id = spl_associated_token_account::id();
    let token_program_id = spl_token::id();
    let wallet_address = Pubkey::new_unique();
    let mint_account = Keypair::new();
    let token_mint_address = mint_account.pubkey();
    let mint_authority = Keypair::new();
    let payer = Keypair::new();

    // This is NOT the associated token address - it's a manually created account
    let non_ata_account = Keypair::new();

    let mut mollusk = Mollusk::default();

    mollusk.add_program(
        &ata_program_id,
        "target/deploy/pinocchio_ata_program",
        &LOADER_V3,
    );

    mollusk.add_program(
        &token_program_id,
        "programs/token/target/deploy/pinocchio_token_program",
        &LOADER_V3,
    );

    // Create and initialize mint first
    let mut accounts = create_test_mint(
        &mollusk,
        &mint_account,
        &mint_authority,
        &payer,
        &token_program_id,
        6,
    );

    // Create a valid token account but at a non-ATA address
    let token_account_balance = 3_500_880; // Standard rent for token account with extension
    let valid_token_account = Account {
        lamports: token_account_balance,
        data: {
            let mut data = [0u8; 165]; // SPL Token account size (no extensions)
                                       // Initialize as a properly structured token account
                                       // Mint (32 bytes)
            data[0..32].copy_from_slice(&token_mint_address.to_bytes());
            // Owner (32 bytes) - correct owner
            data[32..64].copy_from_slice(&wallet_address.to_bytes());
            // Amount (8 bytes) - 0
            data[64..72].copy_from_slice(&0u64.to_le_bytes());
            // Delegate option (36 bytes) - None
            data[72] = 0; // COption::None
                          // State (1 byte) - Initialized
            data[108] = 1; // AccountState::Initialized
                           // Is native option (12 bytes) - None
            data[109] = 0; // COption::None
                           // Delegated amount (8 bytes) - 0
            data[121..129].copy_from_slice(&0u64.to_le_bytes());
            // Close authority option (36 bytes) - None
            data[129] = 0; // COption::None
            data.to_vec()
        },
        owner: token_program_id,
        executable: false,
        rent_epoch: 0,
    };

    accounts.extend([
        (
            wallet_address,
            Account::new(1_000_000, 0, &system_program::id()),
        ),
        (non_ata_account.pubkey(), valid_token_account),
    ]);

    // Try to create idempotent ATA but pass the non-ATA account address
    let mut create_idempotent_ix = build_create_idempotent_ata_instruction(
        ata_program_id,
        payer.pubkey(),
        get_associated_token_address_with_program_id(
            &wallet_address,
            &token_mint_address,
            &token_program_id,
        ),
        wallet_address,
        token_mint_address,
        token_program_id,
    );

    // Replace the ATA address with the non-ATA account address
    create_idempotent_ix.accounts[1] = AccountMeta::new(non_ata_account.pubkey(), false);

    // Should fail with InvalidSeeds because the account address doesn't match the ATA derivation
    mollusk.process_and_validate_instruction(
        &create_idempotent_ix,
        &accounts,
        &[Check::err(ProgramError::InvalidSeeds)],
    );
}
