//! Migrated test for process_create_associated_token_account functionality using mollusk and pinocchio

use {
    crate::tests::test_utils::{
        build_create_ata_instruction, create_mollusk_base_accounts,
        create_mollusk_base_accounts_with_token, setup_mollusk_with_programs, NATIVE_LOADER_ID,
    },
    mollusk_svm::{result::Check, Mollusk},
    solana_instruction::{AccountMeta, Instruction},
    solana_program::program_error::ProgramError,
    solana_program::system_instruction,
    solana_pubkey::Pubkey,
    solana_sdk::{account::Account, signature::Keypair, signer::Signer, system_program, sysvar},
    spl_associated_token_account_client::address::get_associated_token_address_with_program_id,
    std::{eprintln, vec::Vec},
};

use mollusk_svm::program::loader_keys::LOADER_V3;

/// Create a test mint and return accounts with it initialized
pub(crate) fn create_test_mint(
    mollusk: &Mollusk,
    mint_account: &Keypair,
    mint_authority: &Keypair,
    payer: &Keypair,
    token_program: &Pubkey,
    decimals: u8,
) -> Vec<(Pubkey, Account)> {
    let mint_space = 82; // Standard SPL Token mint size
    let rent_lamports = 1_461_600; // Standard rent for mint account

    let create_mint_ix = system_instruction::create_account(
        &payer.pubkey(),
        &mint_account.pubkey(),
        rent_lamports,
        mint_space,
        token_program,
    );

    let mut accounts = create_mollusk_base_accounts_with_token(payer, token_program);

    // Add the mint account (uninitialized, owned by system program initially)
    accounts.push((
        mint_account.pubkey(),
        Account::new(0, 0, &system_program::id()),
    ));

    // Add mint authority as signer
    accounts.push((
        mint_authority.pubkey(),
        Account::new(1_000_000, 0, &system_program::id()),
    ));

    mollusk.process_and_validate_instruction(&create_mint_ix, &accounts, &[Check::success()]);

    // Initialize mint
    let init_mint_ix = spl_token::instruction::initialize_mint(
        token_program,
        &mint_account.pubkey(),
        &mint_authority.pubkey(),
        Some(&mint_authority.pubkey()),
        decimals,
    )
    .unwrap();

    // Update accounts with created mint account
    let result = mollusk.process_instruction(&create_mint_ix, &accounts);
    let created_mint = result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == mint_account.pubkey())
        .map(|(_, account)| account.clone())
        .expect("Mint account should be created");

    accounts
        .iter_mut()
        .find(|(pubkey, _)| *pubkey == mint_account.pubkey())
        .map(|(_, account)| *account = created_mint);

    mollusk.process_and_validate_instruction(&init_mint_ix, &accounts, &[Check::success()]);

    // Update accounts with initialized mint
    let mint_result = mollusk.process_instruction(&init_mint_ix, &accounts);
    let initialized_mint = mint_result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == mint_account.pubkey())
        .map(|(_, account)| account.clone())
        .expect("Initialized mint should exist");

    accounts
        .iter_mut()
        .find(|(pubkey, _)| *pubkey == mint_account.pubkey())
        .map(|(_, account)| *account = initialized_mint);

    accounts
}

#[test]
fn process_create_associated_token_account() {
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
        eprintln!("=== Starting process_create_associated_token_account test ===");
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
        (
            wallet_address,
            Account::new(1_000_000, 0, &system_program::id()),
        ),
        (
            associated_token_address,
            Account::new(0, 0, &system_program::id()),
        ),
    ]);

    let create_ix = build_create_ata_instruction(
        ata_program_id,
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        token_program_id,
    );

    mollusk.process_and_validate_instruction(&create_ix, &accounts, &[Check::success()]);

    // Verify the created account
    let result = mollusk.process_instruction(&create_ix, &accounts);
    let created_account = result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address)
        .map(|(_, account)| account)
        .expect("Associated token account should be created");

    // Token account should be 165 bytes (SPL Token account size)
    assert_eq!(created_account.data.len(), 165);
    // Should be owned by the token program
    assert_eq!(created_account.owner, token_program_id);
    // Should have minimum rent-exempt lamports
    assert!(created_account.lamports > 0);

    #[cfg(feature = "test-debug")]
    {
        eprintln!("✓ Associated token account created successfully");
        eprintln!("Account size: {}", created_account.data.len());
        eprintln!("Account owner: {}", created_account.owner);
        eprintln!("Account lamports: {}", created_account.lamports);
    }
}

#[test]
fn process_create_associated_token_account_with_invalid_mint() {
    let ata_program_id = spl_associated_token_account::id();
    let token_program_id = spl_token::id();
    let wallet_address = Pubkey::new_unique();
    let invalid_mint_address = Pubkey::new_unique(); // Non-existent mint
    let payer = Keypair::new();

    let associated_token_address = get_associated_token_address_with_program_id(
        &wallet_address,
        &invalid_mint_address,
        &token_program_id,
    );

    let mollusk = setup_mollusk_with_programs(&token_program_id);

    let create_ix = build_create_ata_instruction(
        ata_program_id,
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        invalid_mint_address,
        token_program_id,
    );

    // Include the invalid mint account but with invalid/empty data
    let mut accounts = create_mollusk_base_accounts_with_token(&payer, &token_program_id);

    // Add invalid mint account - owned by token program but with invalid data
    accounts.extend([
        (
            invalid_mint_address,
            Account::new(1_461_600, 0, &token_program_id),
        ), // No data - invalid mint
        (
            wallet_address,
            Account::new(1_000_000, 0, &system_program::id()),
        ),
        (
            associated_token_address,
            Account::new(0, 0, &system_program::id()),
        ),
    ]);

    // Should fail because mint data is invalid (empty)
    mollusk.process_and_validate_instruction(
        &create_ix,
        &accounts,
        &[Check::err(ProgramError::Custom(2))], // Invalid Mint error
    );
}

#[test]
fn process_create_associated_token_account_with_invalid_system_program() {
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

    // Create and initialize mint first
    let accounts = create_test_mint(
        &mollusk,
        &mint_account,
        &mint_authority,
        &payer,
        &token_program_id,
        6,
    );

    // Create instruction with invalid system program ID
    let invalid_system_program = Pubkey::new_unique();
    let accounts_metas = [
        AccountMeta::new(payer.pubkey(), true),
        AccountMeta::new(associated_token_address, false),
        AccountMeta::new_readonly(wallet_address, false),
        AccountMeta::new_readonly(token_mint_address, false),
        AccountMeta::new_readonly(invalid_system_program, false), // Invalid system program
        AccountMeta::new_readonly(token_program_id, false),
        AccountMeta::new_readonly(sysvar::rent::id(), false),
    ];

    let invalid_ix = Instruction {
        program_id: ata_program_id,
        accounts: accounts_metas.to_vec(),
        data: [0u8].to_vec(),
    };

    let mut test_accounts = accounts;
    test_accounts.extend([
        (
            wallet_address,
            Account::new(1_000_000, 0, &system_program::id()),
        ),
        (
            associated_token_address,
            Account::new(0, 0, &system_program::id()),
        ),
        (
            invalid_system_program,
            Account::new(0, 0, &NATIVE_LOADER_ID),
        ), // Invalid system program account
    ]);

    // The instruction should fail due to missing invalid system program account
    let result = mollusk.process_instruction(&invalid_ix, &test_accounts);
    assert!(
        result.program_result.is_err(),
        "Should fail due to missing system program account"
    );
}

#[test]
fn process_create_associated_token_account_with_invalid_rent_sysvar() {
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

    // Create and initialize mint first
    let accounts = create_test_mint(
        &mollusk,
        &mint_account,
        &mint_authority,
        &payer,
        &token_program_id,
        6,
    );

    // Create instruction with invalid rent sysvar
    let invalid_rent_sysvar = Pubkey::new_unique();
    let accounts_metas = [
        AccountMeta::new(payer.pubkey(), true),
        AccountMeta::new(associated_token_address, false),
        AccountMeta::new_readonly(wallet_address, false),
        AccountMeta::new_readonly(token_mint_address, false),
        AccountMeta::new_readonly(system_program::id(), false),
        AccountMeta::new_readonly(token_program_id, false),
        AccountMeta::new_readonly(invalid_rent_sysvar, false), // Invalid rent sysvar
    ];

    let invalid_ix = Instruction {
        program_id: ata_program_id,
        accounts: accounts_metas.to_vec(),
        data: [0u8].to_vec(),
    };

    let mut test_accounts = accounts;
    test_accounts.extend([
        (
            wallet_address,
            Account::new(1_000_000, 0, &system_program::id()),
        ),
        (
            associated_token_address,
            Account::new(0, 0, &system_program::id()),
        ),
        (invalid_rent_sysvar, Account::new(0, 0, &sysvar::id())), // Invalid rent sysvar account
    ]);

    // Should fail with InvalidArgument due to invalid rent sysvar
    mollusk.process_and_validate_instruction(
        &invalid_ix,
        &test_accounts,
        &[Check::err(ProgramError::InvalidArgument)],
    );
}

/// Helper function to calculate expected token account balance using mollusk's rent
fn calculate_token_account_balance() -> u64 {
    let mollusk = Mollusk::default();
    // For SPL Token standard account (165 bytes)
    let spl_token_account_size = 165;
    let balance = mollusk.sysvars.rent.minimum_balance(spl_token_account_size);

    // Debug: print the calculated values
    eprintln!(
        "DEBUG: SPL Token account size: {}, rent: {}",
        spl_token_account_size, balance
    );
    eprintln!(
        "DEBUG: Token-2022 size: 170, rent: {}",
        mollusk.sysvars.rent.minimum_balance(170)
    );

    balance
}

#[test]
fn test_create_with_fewer_lamports() {
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

    let expected_token_account_balance = calculate_token_account_balance();

    // Use rent-exempt amount for 0 data (like the original test)
    let insufficient_lamports = mollusk.sysvars.rent.minimum_balance(0);
    eprintln!(
        "DEBUG: insufficient_lamports (rent for 0 data): {}",
        insufficient_lamports
    );

    // Add associated token address with insufficient lamports (enough for 0 data but not token account)
    let payer_initial_lamports = 10_000_000_000u64; // 10 SOL, should be plenty
    eprintln!("DEBUG: payer_initial_lamports: {}", payer_initial_lamports);
    eprintln!(
        "DEBUG: required missing lamports: {}",
        expected_token_account_balance - insufficient_lamports
    );

    accounts.extend([
        (
            wallet_address,
            Account::new(1_000_000, 0, &system_program::id()),
        ),
        (
            associated_token_address,
            Account::new(insufficient_lamports, 0, &system_program::id()),
        ),
    ]);

    // Create instruction to create ATA
    let create_ix = build_create_ata_instruction(
        ata_program_id,
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        token_program_id,
    );

    // Process instruction - program should add the missing lamports
    let result = mollusk.process_instruction(&create_ix, &accounts);

    // Verify the ATA was created with proper balance
    let created_ata = result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address)
        .map(|(_, account)| account)
        .expect("ATA should be created");

    eprintln!("DEBUG: created_ata.lamports: {}", created_ata.lamports);
    eprintln!(
        "DEBUG: expected_token_account_balance: {}",
        expected_token_account_balance
    );
    eprintln!("DEBUG: created_ata.data.len(): {}", created_ata.data.len());
    eprintln!("DEBUG: created_ata.owner: {}", created_ata.owner);

    assert_eq!(created_ata.lamports, expected_token_account_balance);
    assert_eq!(created_ata.owner, token_program_id);
}

#[test]
fn test_create_with_excess_lamports() {
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

    let expected_token_account_balance = calculate_token_account_balance();
    let excess_lamports = expected_token_account_balance + 1; // More than needed

    // Add associated token address with excess lamports
    accounts.extend([
        (
            wallet_address,
            Account::new(1_000_000, 0, &system_program::id()),
        ),
        (
            associated_token_address,
            Account::new(excess_lamports, 0, &system_program::id()),
        ),
    ]);

    // Create instruction to create ATA
    let create_ix = build_create_ata_instruction(
        ata_program_id,
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        token_program_id,
    );

    // Process instruction - program should preserve excess lamports (not steal them)
    let result = mollusk.process_instruction(&create_ix, &accounts);

    // Verify the ATA was created and excess lamports were preserved
    let created_ata = result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address)
        .map(|(_, account)| account)
        .expect("ATA should be created");

    assert_eq!(created_ata.lamports, excess_lamports); // Should preserve excess
    assert_eq!(created_ata.owner, token_program_id);
}

#[test]
fn test_create_associated_token_account_using_legacy_implicit_instruction() {
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

    // Add associated token address (not yet created)
    accounts.extend([
        (
            wallet_address,
            Account::new(1_000_000, 0, &system_program::id()),
        ),
        (
            associated_token_address,
            Account::new(0, 0, &system_program::id()),
        ),
    ]);

    // Create legacy instruction with empty data and explicit rent sysvar
    let accounts_metas = [
        AccountMeta::new(payer.pubkey(), true),
        AccountMeta::new(associated_token_address, false),
        AccountMeta::new_readonly(wallet_address, false),
        AccountMeta::new_readonly(token_mint_address, false),
        AccountMeta::new_readonly(system_program::id(), false),
        AccountMeta::new_readonly(token_program_id, false),
        AccountMeta::new_readonly(sysvar::rent::id(), false), // Explicit rent sysvar for legacy support
    ];

    let legacy_ix = Instruction {
        program_id: ata_program_id,
        accounts: accounts_metas.to_vec(),
        data: Vec::new(), // Empty data for legacy implicit instruction
    };

    // Process legacy instruction - should work for backwards compatibility
    let result = mollusk.process_instruction(&legacy_ix, &accounts);

    // Verify the ATA was created properly
    let created_ata = result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address)
        .map(|(_, account)| account)
        .expect("ATA should be created with legacy instruction");

    let expected_token_account_balance = calculate_token_account_balance();
    assert_eq!(created_ata.lamports, expected_token_account_balance);
    assert_eq!(created_ata.owner, token_program_id);
    assert!(created_ata.data.len() > 0); // Should have token account data
}
