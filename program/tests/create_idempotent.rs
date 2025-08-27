mod utils;

use {
    mollusk_svm::result::ProgramResult,
    solana_program::instruction::*,
    solana_program_test::*,
    solana_pubkey::Pubkey,
    solana_sdk::{
        account::Account as SolanaAccount,
        program_error::ProgramError,
        program_option::COption,
        program_pack::Pack,
        signature::Signer,
        signer::keypair::Keypair,
        transaction::{Transaction, TransactionError},
    },
    solana_system_interface::instruction::create_account,
    spl_associated_token_account::error::AssociatedTokenAccountError,
    spl_associated_token_account_interface::{
        address::get_associated_token_address_with_program_id,
        instruction::{
            create_associated_token_account, create_associated_token_account_idempotent,
        },
    },
    spl_token_2022_interface::{
        extension::ExtensionType,
        instruction::initialize_account,
        state::{Account, AccountState},
    },
    utils::*,
};

#[tokio::test]
async fn success_account_exists() {
    let wallet_address = Pubkey::new_unique();
    let token_mint_address = Pubkey::new_unique();
    let associated_token_address = get_associated_token_address_with_program_id(
        &wallet_address,
        &token_mint_address,
        &spl_token_2022_interface::id(),
    );

    let mollusk = setup_mollusk_with_programs(&spl_token_2022_interface::id());
    let payer = Keypair::new();
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

    let instruction = build_create_ata_instruction_with_system_account(
        &mut accounts,
        spl_associated_token_account::id(),
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        spl_token_2022_interface::id(),
        CreateAtaInstructionType::CreateIdempotent { bump: None },
    );
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

    // Test failure case: try to Create when ATA already exists as token account
    // Replace any existing account at the ATA address with the token account from the first instruction
    if let Some(existing_index) = accounts
        .iter()
        .position(|(pk, _)| *pk == associated_token_address)
    {
        accounts[existing_index] = (associated_token_address, associated_account.clone());
    } else {
        accounts.push((associated_token_address, associated_account.clone()));
    }

    // Build Create instruction - this should fail because account exists and is owned by token program
    // Note: We use the raw build_create_ata_instruction because we want to test the failure case
    // where an existing token account is present (not add a system account)
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

    // This should fail with IllegalOwner because the account already exists and is owned by token program
    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::IllegalOwner)
    );

    let instruction = build_create_ata_instruction_with_system_account(
        &mut accounts,
        spl_associated_token_account::id(),
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        spl_token_2022_interface::id(),
        CreateAtaInstructionType::CreateIdempotent { bump: None },
    );
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

#[tokio::test]
async fn fail_account_exists_with_wrong_owner() {
    let wallet_address = Pubkey::new_unique();
    let token_mint_address = Pubkey::new_unique();
    let associated_token_address = get_associated_token_address_with_program_id(
        &wallet_address,
        &token_mint_address,
        &spl_token_2022_interface::id(),
    );

    let wrong_owner = Pubkey::new_unique();
    let mollusk = setup_mollusk_with_programs(&spl_token_2022_interface::id());
    let payer = Keypair::new();
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
        (
            wrong_owner,
            account_builder::AccountBuilder::system_account(1_000_000),
        ),
        (
            associated_token_address,
            account_builder::AccountBuilder::token_account(
                &token_mint_address,
                &wrong_owner,
                0,
                &spl_token_2022_interface::id(),
            ),
        ),
    ]);

    let instruction = build_create_ata_instruction_with_system_account(
        &mut accounts,
        spl_associated_token_account::id(),
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        spl_token_2022_interface::id(),
        CreateAtaInstructionType::CreateIdempotent { bump: None },
    );
    assert_eq!(
        mollusk
            .process_instruction(&instruction, &accounts)
            .program_result,
        ProgramResult::Failure(ProgramError::Custom(
            spl_associated_token_account::error::AssociatedTokenAccountError::InvalidOwner as u32,
        ))
    );
}

#[tokio::test]
async fn fail_non_ata() {
    let token_mint_address = Pubkey::new_unique();
    let wallet_address = Pubkey::new_unique();
    let account = Keypair::new();

    let mollusk = setup_mollusk_with_programs(&spl_token_2022_interface::id());
    let payer = Keypair::new();
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
        (
            account.pubkey(),
            account_builder::AccountBuilder::token_account(
                &token_mint_address,
                &wallet_address,
                0,
                &spl_token_2022_interface::id(),
            ),
        ),
    ]);

    let mut instruction = build_create_ata_instruction_with_system_account(
        &mut accounts,
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
        CreateAtaInstructionType::CreateIdempotent { bump: None },
    );
    instruction.accounts[1] = AccountMeta::new(account.pubkey(), false);
    assert_eq!(
        mollusk
            .process_instruction(&instruction, &accounts)
            .program_result,
        ProgramResult::Failure(ProgramError::InvalidSeeds)
    );
}
