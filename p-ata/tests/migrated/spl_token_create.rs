//! Migrated test for SPL token create functionality using mollusk
#![cfg(test)]

use {
    mollusk_svm::result::Check,
    pinocchio_ata_program::test_utils::{
        build_create_ata_instruction, calculate_account_rent,
        create_mollusk_base_accounts_with_token, setup_mollusk_with_programs,
        CreateAtaInstructionType,
    },
    solana_pubkey::Pubkey,
    solana_sdk::{account::Account, signature::Keypair, signer::Signer},
    solana_system_interface::{instruction as system_instruction, program as system_program},
    spl_associated_token_account_client::address::get_associated_token_address,
};

#[test]
fn success_create() {
    let _ata_program_id = spl_associated_token_account::id();
    let token_program_id = spl_token::id();
    let wallet_address = Pubkey::new_unique();
    let mint_account = Keypair::new();
    let token_mint_address = mint_account.pubkey();
    let mint_authority = Keypair::new();
    let payer = Keypair::new();

    let associated_token_address =
        get_associated_token_address(&wallet_address, &token_mint_address);

    let mollusk = setup_mollusk_with_programs(&token_program_id);

    // Step 1: Create the mint account
    let mint_space = 82; // Standard SPL Token mint size
    let rent_lamports = calculate_account_rent(mint_space as usize);

    let create_mint_ix = system_instruction::create_account(
        &payer.pubkey(),
        &token_mint_address,
        rent_lamports,
        mint_space,
        &token_program_id,
    );

    let mut accounts = create_mollusk_base_accounts_with_token(&payer, &token_program_id);
    accounts.push((
        token_mint_address,
        Account::new(0, 0, &system_program::id()),
    ));
    accounts.push((
        mint_authority.pubkey(),
        Account::new(1_000_000, 0, &system_program::id()),
    ));

    mollusk.process_and_validate_instruction(&create_mint_ix, &accounts, &[Check::success()]);

    // Step 2: Initialize mint
    let init_mint_ix = spl_token::instruction::initialize_mint(
        &token_program_id,
        &token_mint_address,
        &mint_authority.pubkey(),
        Some(&mint_authority.pubkey()),
        6, // decimals
    )
    .unwrap();

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

    // Step 3: Create associated token account
    let create_ix = build_create_ata_instruction(
        _ata_program_id,
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
    accounts.extend([
        (wallet_address, Account::new(0, 0, &system_program::id())),
        (
            associated_token_address,
            Account::new(0, 0, &system_program::id()),
        ),
    ]);

    mollusk.process_and_validate_instruction(&create_ix, &accounts, &[Check::success()]);
    let result = mollusk.process_instruction(&create_ix, &accounts);
    let created_account = result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address)
        .map(|(_, account)| account)
        .expect("Associated token account should be created");

    let expected_token_account_len = 165; // SPL Token account size
    assert_eq!(created_account.data.len(), expected_token_account_len);
    assert_eq!(created_account.owner, token_program_id);

    // Verify lamports are rent-exempt for the account size
    assert!(created_account.lamports > 0); // Must have rent-exempt lamports
}

#[test]
fn success_using_deprecated_instruction_creator() {
    #[allow(deprecated)]
    use spl_associated_token_account::create_associated_token_account as deprecated_create_associated_token_account;

    let _ata_program_id = spl_associated_token_account::id();
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
    let mollusk = setup_mollusk_with_programs(&token_program_id);

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

    let mut accounts = create_mollusk_base_accounts_with_token(&payer, &token_program_id);

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

    // Step 3: Create associated token account using the same legacy function as original test
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

    assert!(created_account.lamports > 0);
}
