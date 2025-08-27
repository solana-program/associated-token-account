mod utils;

use {
    crate::utils::{
        account_builder, build_create_ata_instruction,
        build_create_ata_instruction_with_system_account, create_mollusk_base_accounts_with_token,
        setup_mollusk_with_programs, CreateAtaInstructionType,
    },
    mollusk_svm::result::ProgramResult,
    solana_program::{instruction::*, sysvar},
    solana_program_test::*,
    solana_pubkey::Pubkey,
    solana_sdk::{program_error::ProgramError, signature::Signer},
    spl_associated_token_account_interface::address::get_associated_token_address_with_program_id,
    spl_token_2022_interface::{extension::ExtensionType, state::Account},
};

#[tokio::test]
async fn test_associated_token_address() {
    let wallet_address = Pubkey::new_unique();
    let token_mint_address = Pubkey::new_unique();
    let associated_token_address = get_associated_token_address_with_program_id(
        &wallet_address,
        &token_mint_address,
        &spl_token_2022_interface::id(),
    );

    let mollusk = setup_mollusk_with_programs(&spl_token_2022_interface::id());
    let payer = solana_sdk::signer::keypair::Keypair::new();
    let mut accounts =
        create_mollusk_base_accounts_with_token(&payer, &spl_token_2022_interface::id());

    // Add mint and wallet to accounts
    accounts.push((
        token_mint_address,
        account_builder::AccountBuilder::extended_mint(6, &payer.pubkey()),
    ));
    accounts.push((
        wallet_address,
        account_builder::AccountBuilder::system_account(1_000_000),
    ));

    let expected_token_account_len =
        ExtensionType::try_calculate_account_len::<Account>(&[ExtensionType::ImmutableOwner])
            .unwrap();
    let expected_token_account_balance =
        solana_sdk::rent::Rent::default().minimum_balance(expected_token_account_len);

    let instruction = build_create_ata_instruction_with_system_account(
        &mut accounts,
        spl_associated_token_account::id(),
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        spl_token_2022_interface::id(),
        CreateAtaInstructionType::Create {
            bump: None,
            account_len: None,
        },
    );

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(result.program_result.is_ok());

    // Associated account now exists
    let associated_account = result
        .resulting_accounts
        .into_iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address)
        .expect("associated_account not none")
        .1;
    assert_eq!(associated_account.data.len(), expected_token_account_len,);
    assert_eq!(associated_account.owner, spl_token_2022_interface::id());
    assert_eq!(associated_account.lamports, expected_token_account_balance);
}

#[tokio::test]
async fn test_create_with_fewer_lamports() {
    let wallet_address = Pubkey::new_unique();
    let token_mint_address = Pubkey::new_unique();
    let associated_token_address = get_associated_token_address_with_program_id(
        &wallet_address,
        &token_mint_address,
        &spl_token_2022_interface::id(),
    );

    let mollusk = setup_mollusk_with_programs(&spl_token_2022_interface::id());
    let payer = solana_sdk::signer::keypair::Keypair::new();
    let mut accounts =
        create_mollusk_base_accounts_with_token(&payer, &spl_token_2022_interface::id());

    // Add mint and wallet to accounts
    accounts.push((
        token_mint_address,
        account_builder::AccountBuilder::extended_mint(6, &payer.pubkey()),
    ));
    accounts.push((
        wallet_address,
        account_builder::AccountBuilder::system_account(1_000_000),
    ));

    let expected_token_account_len =
        ExtensionType::try_calculate_account_len::<Account>(&[ExtensionType::ImmutableOwner])
            .unwrap();
    let expected_token_account_balance =
        solana_sdk::rent::Rent::default().minimum_balance(expected_token_account_len);

    // Pre-fund the ATA address with insufficient lamports (only enough for 0 data)
    let insufficient_lamports = 890880; // rent-exempt for 0 data but not for token account
    accounts.push((
        associated_token_address,
        solana_sdk::account::Account::new(
            insufficient_lamports,
            0,
            &solana_system_interface::program::id(),
        ),
    ));

    // Check that the program adds the extra lamports
    let instruction = build_create_ata_instruction_with_system_account(
        &mut accounts,
        spl_associated_token_account::id(),
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        spl_token_2022_interface::id(),
        CreateAtaInstructionType::Create {
            bump: None,
            account_len: None,
        },
    );

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(result.program_result.is_ok());

    // Verify the created account has the correct balance (program should add extra lamports)
    let created_account = result
        .resulting_accounts
        .into_iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address)
        .expect("ATA should be created")
        .1;

    assert_eq!(created_account.lamports, expected_token_account_balance);
    assert_eq!(created_account.owner, spl_token_2022_interface::id());
}

#[tokio::test]
async fn test_create_with_excess_lamports() {
    let wallet_address = Pubkey::new_unique();
    let token_mint_address = Pubkey::new_unique();
    let associated_token_address = get_associated_token_address_with_program_id(
        &wallet_address,
        &token_mint_address,
        &spl_token_2022_interface::id(),
    );

    let mollusk = setup_mollusk_with_programs(&spl_token_2022_interface::id());
    let payer = solana_sdk::signer::keypair::Keypair::new();
    let mut accounts =
        create_mollusk_base_accounts_with_token(&payer, &spl_token_2022_interface::id());
    accounts.extend([
        (
            token_mint_address,
            account_builder::AccountBuilder::extended_mint(6, &payer.pubkey()),
        ),
        (
            wallet_address,
            account_builder::AccountBuilder::system_account(1_000_000),
        ),
    ]);
    let expected_token_account_len =
        ExtensionType::try_calculate_account_len::<Account>(&[ExtensionType::ImmutableOwner])
            .unwrap();
    let expected_token_account_balance =
        solana_sdk::rent::Rent::default().minimum_balance(expected_token_account_len);
    let excess_lamports = expected_token_account_balance + 1;
    accounts.push((
        associated_token_address,
        solana_sdk::account::Account::new(
            excess_lamports,
            0,
            &solana_system_interface::program::id(),
        ),
    ));

    // This test provides its own ATA account with excess lamports, so use raw instruction
    let instruction = build_create_ata_instruction(
        spl_associated_token_account::id(),
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        spl_token_2022_interface::id(),
        CreateAtaInstructionType::Create {
            bump: None,
            account_len: None,
        },
    );
    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(result.program_result.is_ok());
    let created_account = result
        .resulting_accounts
        .into_iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address)
        .expect("ATA should be created")
        .1;
    assert_eq!(
        (created_account.lamports, created_account.owner),
        (excess_lamports, spl_token_2022_interface::id())
    );
}

#[tokio::test]
async fn test_create_account_mismatch() {
    let wallet_address = Pubkey::new_unique();
    let token_mint_address = Pubkey::new_unique();
    let _associated_token_address = get_associated_token_address_with_program_id(
        &wallet_address,
        &token_mint_address,
        &spl_token_2022_interface::id(),
    );

    let mollusk = setup_mollusk_with_programs(&spl_token_2022_interface::id());
    let payer = solana_sdk::signer::keypair::Keypair::new();
    let mut accounts =
        create_mollusk_base_accounts_with_token(&payer, &spl_token_2022_interface::id());
    accounts.extend([
        (
            token_mint_address,
            account_builder::AccountBuilder::extended_mint(6, &payer.pubkey()),
        ),
        (
            wallet_address,
            account_builder::AccountBuilder::system_account(1_000_000),
        ),
    ]);

    // Add ATA system account for Mollusk (needed for all test cases)
    let associated_token_address = get_associated_token_address_with_program_id(
        &wallet_address,
        &token_mint_address,
        &spl_token_2022_interface::id(),
    );
    accounts.push((
        associated_token_address,
        account_builder::AccountBuilder::system_account(0),
    ));

    for (account_idx, _comment) in [
        (1, "Invalid associated_account_address"),
        (2, "Invalid wallet_address"),
        (3, "Invalid token_mint_address"),
    ] {
        let mut instruction = build_create_ata_instruction(
            spl_associated_token_account::id(),
            payer.pubkey(),
            get_associated_token_address_with_program_id(
                &wallet_address,
                &token_mint_address,
                &spl_token_2022_interface::id(),
            ),
            wallet_address,
            token_mint_address,
            spl_token_2022_interface::id(),
            CreateAtaInstructionType::Create {
                bump: None,
                account_len: None,
            },
        );
        instruction.accounts[account_idx] = AccountMeta::new(Pubkey::default(), false); // <-- {comment}
        assert_eq!(
            mollusk
                .process_instruction(&instruction, &accounts)
                .program_result,
            ProgramResult::Failure(ProgramError::InvalidSeeds)
        );
    }
}

#[tokio::test]
async fn test_create_associated_token_account_using_legacy_implicit_instruction() {
    let wallet_address = Pubkey::new_unique();
    let token_mint_address = Pubkey::new_unique();
    let associated_token_address = get_associated_token_address_with_program_id(
        &wallet_address,
        &token_mint_address,
        &spl_token_2022_interface::id(),
    );

    let mollusk = setup_mollusk_with_programs(&spl_token_2022_interface::id());
    let payer = solana_sdk::signer::keypair::Keypair::new();
    let mut accounts =
        create_mollusk_base_accounts_with_token(&payer, &spl_token_2022_interface::id());
    accounts.extend([
        (
            token_mint_address,
            account_builder::AccountBuilder::extended_mint(6, &payer.pubkey()),
        ),
        (
            wallet_address,
            account_builder::AccountBuilder::system_account(1_000_000),
        ),
    ]);
    let expected_token_account_len =
        ExtensionType::try_calculate_account_len::<Account>(&[ExtensionType::ImmutableOwner])
            .unwrap();
    let expected_token_account_balance =
        solana_sdk::rent::Rent::default().minimum_balance(expected_token_account_len);

    // Add ATA system account for Mollusk
    accounts.push((
        associated_token_address,
        account_builder::AccountBuilder::system_account(0),
    ));

    let mut instruction = build_create_ata_instruction(
        spl_associated_token_account::id(),
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        spl_token_2022_interface::id(),
        CreateAtaInstructionType::Create {
            bump: None,
            account_len: None,
        },
    );
    instruction.data = vec![];
    instruction
        .accounts
        .push(AccountMeta::new_readonly(sysvar::rent::id(), false));
    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(result.program_result.is_ok());
    let associated_account = result
        .resulting_accounts
        .into_iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address)
        .expect("associated_account not none")
        .1;
    assert_eq!(associated_account.data.len(), expected_token_account_len);
    assert_eq!(associated_account.owner, spl_token_2022_interface::id());
    assert_eq!(associated_account.lamports, expected_token_account_balance);
}
