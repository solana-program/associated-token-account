//! Migrated test for idempotent creation functionality using mollusk
#![cfg(test)]

use {
    mollusk_svm::result::{Check, ProgramResult},
    pinocchio_ata_program::test_utils::{
        account_builder::AccountBuilder, build_create_ata_instruction, create_test_mint,
        setup_mollusk_with_programs, CreateAtaInstructionType,
    },
    solana_instruction::AccountMeta,
    solana_program::program_error::ProgramError,
    solana_pubkey::Pubkey,
    solana_sdk::{account::Account, signature::Keypair, signer::Signer},
    solana_system_interface::program as system_program,
    spl_associated_token_account_client::address::get_associated_token_address_with_program_id,
};

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
        (wallet_address, Account::new(1, 0, &system_program::id())),
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
        CreateAtaInstructionType::CreateIdempotent { bump: None },
    );

    let result =
        mollusk.process_and_validate_instruction(&create_ix, &accounts, &[Check::success()]);

    // The account should now have more lamports, too
    let created_ata = result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address)
        .map(|(_, account)| account.clone())
        .expect("ATA should be created");
    assert!(created_ata.lamports > 2000000);
    // account properties should be as expected
    assert_eq!(created_ata.data.len(), 165, "ATA should be 165 bytes");
    assert_eq!(created_ata.owner, token_program_id);
    assert_eq!(created_ata.executable, false);

    // Step 3: Try with idempotent instruction on already created ATA - should also succeed
    let create_idempotent_ix = build_create_ata_instruction(
        ata_program_id,
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        token_program_id,
        CreateAtaInstructionType::CreateIdempotent { bump: None },
    );

    // Update accounts with the created ATA
    let result = mollusk.process_instruction(&create_ix, &accounts);
    let created_ata = result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address)
        .map(|(_, account)| account.clone())
        .expect("ATA should still be available");

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
    let create_idempotent_ix = build_create_ata_instruction(
        ata_program_id,
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        token_program_id,
        CreateAtaInstructionType::CreateIdempotent { bump: None },
    );

    mollusk.process_and_validate_instruction(&create_idempotent_ix, &accounts, &[Check::success()]);

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
        CreateAtaInstructionType::Create {
            bump: None,
            account_len: None,
        },
    );

    mollusk.process_and_validate_instruction(
        &create_ix,
        &updated_accounts,
        &[Check::err(ProgramError::Custom(0))], // Should fail because account already exists (system program error)
    );

    // Step 5: Try CreateIdempotent again on existing ATA - should succeed (core idempotent behavior)
    mollusk.process_and_validate_instruction(
        &create_idempotent_ix,
        &updated_accounts,
        &[Check::success()],
    );

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

    let mollusk = setup_mollusk_with_programs(&token_program_id);

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
    let create_idempotent_ix = build_create_ata_instruction(
        ata_program_id,
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address, // Using the correct mint, but ATA address is for wrong mint
        token_program_id,
        CreateAtaInstructionType::CreateIdempotent { bump: None },
    );

    // This should fail because the derived ATA address doesn't match the provided address
    // P-ATA fails downstream with UnknownError(PrivilegeEscalation) for address mismatches
    let result = mollusk.process_instruction(&create_idempotent_ix, &accounts);
    assert!(matches!(
        result.program_result,
        ProgramResult::UnknownError(_)
    ));
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

    accounts.extend([
        (correct_wallet, Account::new(0, 0, &system_program::id())),
        (wrong_wallet, Account::new(0, 0, &system_program::id())),
        (
            associated_token_address,
            Account::new(0, 0, &system_program::id()),
        ),
    ]);

    // Step 2: Try to create ATA with wrong wallet
    let create_idempotent_ix = build_create_ata_instruction(
        ata_program_id,
        payer.pubkey(),
        associated_token_address,
        wrong_wallet, // Wrong wallet!
        token_mint_address,
        token_program_id,
        CreateAtaInstructionType::CreateIdempotent { bump: None },
    );

    // This should fail because the wallet doesn't match the ATA derivation
    // P-ATA fails downstream with UnknownError(PrivilegeEscalation) for address mismatches
    let result = mollusk.process_instruction(&create_idempotent_ix, &accounts);
    assert!(matches!(
        result.program_result,
        ProgramResult::UnknownError(_)
    ));
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

    let mollusk = setup_mollusk_with_programs(&token_program_id);

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
    let wrong_token_account = {
        let mut account =
            AccountBuilder::token_account(&token_mint_address, &wrong_owner, 0, &token_program_id);
        account.lamports = 1_000_000_000;
        account
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
    let create_idempotent_ix = build_create_ata_instruction(
        ata_program_id,
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        token_program_id,
        CreateAtaInstructionType::CreateIdempotent { bump: None },
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

    let mollusk = setup_mollusk_with_programs(&token_program_id);

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
    let token_account_balance = 3_500_880;
    let valid_token_account = {
        let mut account = AccountBuilder::token_account(
            &token_mint_address,
            &wallet_address,
            0,
            &token_program_id,
        );
        account.lamports = token_account_balance;
        account
    };

    accounts.extend([
        (
            wallet_address,
            Account::new(1_000_000, 0, &system_program::id()),
        ),
        (non_ata_account.pubkey(), valid_token_account),
    ]);

    // Try to create idempotent ATA but pass the non-ATA account address
    let mut create_idempotent_ix = build_create_ata_instruction(
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
        CreateAtaInstructionType::CreateIdempotent { bump: None },
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
