mod utils;

use {
    mollusk_svm::result::ProgramResult,
    solana_program::instruction::*,
    solana_program_test::*,
    solana_pubkey::Pubkey,
    solana_sdk::{program_error::ProgramError, signature::Signer, signer::keypair::Keypair},
    spl_associated_token_account_interface::address::get_associated_token_address_with_program_id,
    spl_token_2022_interface::{extension::ExtensionType, state::Account},
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
    accounts.push((
        token_mint_address,
        account_builder::AccountBuilder::extended_mint(6, &payer.pubkey()),
    ));
    ensure_system_accounts_with_lamports(&mut accounts, &[(wallet_address, 1_000_000)]);
    let rent = solana_sdk::rent::Rent::default();
    let expected_token_account_len =
        ExtensionType::try_calculate_account_len::<Account>(&[ExtensionType::ImmutableOwner])
            .unwrap();
    let expected_token_account_balance = rent.minimum_balance(expected_token_account_len);

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
    let mollusk_result = process_and_merge_instruction(&mollusk, &instruction, &mut accounts);
    assert!(matches!(mollusk_result, ProgramResult::Success));
    let associated_account = get_account(&accounts, associated_token_address);
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
    let mollusk_result = process_and_merge_instruction(&mollusk, &instruction, &mut accounts);
    assert!(matches!(mollusk_result, ProgramResult::Success));
    let associated_account = get_account(&accounts, associated_token_address);
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
            associated_token_address,
            account_builder::AccountBuilder::token_account(
                &token_mint_address,
                &wrong_owner,
                0,
                &spl_token_2022_interface::id(),
            ),
        ),
    ]);
    ensure_system_accounts_with_lamports(
        &mut accounts,
        &[(wallet_address, 1_000_000), (wrong_owner, 1_000_000)],
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
    let mollusk_result = process_and_merge_instruction(&mollusk, &instruction, &mut accounts);
    assert_eq!(
        mollusk_result,
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
            account.pubkey(),
            account_builder::AccountBuilder::token_account(
                &token_mint_address,
                &wallet_address,
                0,
                &spl_token_2022_interface::id(),
            ),
        ),
    ]);
    ensure_system_accounts_with_lamports(&mut accounts, &[(wallet_address, 1_000_000)]);

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
    let mollusk_result = process_and_merge_instruction(&mollusk, &instruction, &mut accounts);
    assert_eq!(
        mollusk_result,
        ProgramResult::Failure(ProgramError::InvalidSeeds)
    );
}
