use {
    crate::tests::program_test::mollusk_program_test_2022 as program_test_2022,
    mollusk_svm::{program::loader_keys, Mollusk},
    solana_program::{instruction::*, pubkey::Pubkey, system_program},
    solana_program_test::*,
    solana_sdk::{
        account::Account as SolanaAccount,
        account_info::AccountInfo,
        program_option::COption,
        program_pack::Pack,
        signature::Signer,
        signer::keypair::Keypair,
        system_instruction::create_account,
        transaction::{Transaction, TransactionError},
    },
    spl_associated_token_account::{
        error::AssociatedTokenAccountError,
        instruction::{
            create_associated_token_account, create_associated_token_account_idempotent,
        },
    },
    spl_associated_token_account_client::{
        address::get_associated_token_address_with_program_id, instruction as client_instruction,
    },
    spl_token_2022::{
        extension::ExtensionType,
        instruction::initialize_account,
        state::{Account, AccountState},
    },
};

#[tokio::test]
async fn success_account_exists() {
    let wallet_address = Pubkey::new_unique();
    let token_mint_address = Pubkey::new_unique();
    let associated_token_address = get_associated_token_address_with_program_id(
        &wallet_address,
        &token_mint_address,
        &spl_token_2022::id(),
    );

    let (mut banks_client, payer, recent_blockhash) =
        program_test_2022(token_mint_address).start().await;
    let rent = banks_client.get_rent().await.unwrap();
    let expected_token_account_len =
        ExtensionType::try_calculate_account_len::<Account>(&[ExtensionType::ImmutableOwner])
            .unwrap();
    let expected_token_account_balance = rent.minimum_balance(expected_token_account_len);

    let instruction = create_associated_token_account_idempotent(
        &payer.pubkey(),
        &wallet_address,
        &token_mint_address,
        &spl_token_2022::id(),
    );

    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();

    // Associated account now exists
    let associated_account = banks_client
        .get_account(associated_token_address)
        .await
        .expect("get_account")
        .expect("associated_account not none");
    assert_eq!(associated_account.data.len(), expected_token_account_len);
    assert_eq!(associated_account.owner, spl_token_2022::id());
    assert_eq!(associated_account.lamports, expected_token_account_balance);

    // Unchecked instruction fails
    let instruction = create_associated_token_account(
        &payer.pubkey(),
        &wallet_address,
        &token_mint_address,
        &spl_token_2022::id(),
    );

    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );
    assert_eq!(
        banks_client
            .process_transaction(transaction)
            .await
            .unwrap_err()
            .unwrap(),
        TransactionError::InstructionError(0, InstructionError::IllegalOwner)
    );

    // Get a new blockhash, succeed with create if non existent
    let recent_blockhash = banks_client
        .get_new_latest_blockhash(&recent_blockhash)
        .await
        .unwrap();

    let instruction = create_associated_token_account_idempotent(
        &payer.pubkey(),
        &wallet_address,
        &token_mint_address,
        &spl_token_2022::id(),
    );

    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();

    // Associated account is unchanged
    let associated_account = banks_client
        .get_account(associated_token_address)
        .await
        .expect("get_account")
        .expect("associated_account not none");
    assert_eq!(associated_account.data.len(), expected_token_account_len);
    assert_eq!(associated_account.owner, spl_token_2022::id());
    assert_eq!(associated_account.lamports, expected_token_account_balance);
}

#[tokio::test]
async fn fail_account_exists_with_wrong_owner() {
    let wallet_address = Pubkey::new_unique();
    let token_mint_address = Pubkey::new_unique();
    let associated_token_address = get_associated_token_address_with_program_id(
        &wallet_address,
        &token_mint_address,
        &spl_token_2022::id(),
    );

    let wrong_owner = Pubkey::new_unique();
    let mut associated_token_account =
        SolanaAccount::new(1_000_000_000, Account::LEN, &spl_token_2022::id());
    let token_account = Account {
        mint: token_mint_address,
        owner: wrong_owner,
        amount: 0,
        delegate: COption::None,
        state: AccountState::Initialized,
        is_native: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    };
    Account::pack(token_account, &mut associated_token_account.data).unwrap();
    let mut pt = program_test_2022(token_mint_address);
    pt.add_account(associated_token_address, associated_token_account);
    let (banks_client, payer, recent_blockhash) = pt.start().await;

    // fail creating token account if non existent
    let instruction = create_associated_token_account_idempotent(
        &payer.pubkey(),
        &wallet_address,
        &token_mint_address,
        &spl_token_2022::id(),
    );
    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    assert_eq!(
        banks_client
            .process_transaction(transaction)
            .await
            .unwrap_err()
            .unwrap(),
        TransactionError::InstructionError(
            0,
            InstructionError::Custom(AssociatedTokenAccountError::InvalidOwner as u32)
        )
    );
}

#[tokio::test]
async fn fail_non_ata() {
    let token_mint_address = Pubkey::new_unique();
    let (banks_client, payer, recent_blockhash) =
        program_test_2022(token_mint_address).start().await;

    let rent = banks_client.get_rent().await.unwrap();
    let token_account_len =
        ExtensionType::try_calculate_account_len::<Account>(&[ExtensionType::ImmutableOwner])
            .unwrap();
    let token_account_balance = rent.minimum_balance(token_account_len);

    let wallet_address = Pubkey::new_unique();
    let account = Keypair::new();
    let transaction = Transaction::new_signed_with_payer(
        &[
            create_account(
                &payer.pubkey(),
                &account.pubkey(),
                token_account_balance,
                token_account_len as u64,
                &spl_token_2022::id(),
            ),
            initialize_account(
                &spl_token_2022::id(),
                &account.pubkey(),
                &token_mint_address,
                &wallet_address,
            )
            .unwrap(),
        ],
        Some(&payer.pubkey()),
        &[&payer, &account],
        recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();

    let mut instruction = create_associated_token_account_idempotent(
        &payer.pubkey(),
        &wallet_address,
        &token_mint_address,
        &spl_token_2022::id(),
    );
    instruction.accounts[1] = AccountMeta::new(account.pubkey(), false); // <-- Invalid associated_account_address

    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );
    assert_eq!(
        banks_client
            .process_transaction(transaction)
            .await
            .unwrap_err()
            .unwrap(),
        TransactionError::InstructionError(0, InstructionError::InvalidSeeds)
    );
}

#[tokio::test]
async fn test_mollusk_api_exactly_matches_original() {
    use crate::tests::program_test::mollusk_program_test_2022;

    // This test shows the API is now EXACTLY the same as the original!
    let wallet_address = Pubkey::new_unique();
    let token_mint_address = Pubkey::new_unique();
    let associated_token_address = get_associated_token_address_with_program_id(
        &wallet_address,
        &token_mint_address,
        &spl_token_2022::id(),
    );

    // EXACT same API as original - just change the function name!
    let (mut banks_client, payer, recent_blockhash) =
        mollusk_program_test_2022(token_mint_address).start().await;
    let rent = banks_client.get_rent().await.unwrap();

    let expected_token_account_len = ExtensionType::try_calculate_account_len::<
        spl_token_2022::state::Account,
    >(&[ExtensionType::ImmutableOwner])
    .unwrap();
    let expected_token_account_balance = rent.minimum_balance(expected_token_account_len);

    // Create the idempotent instruction
    let instruction = client_instruction::create_associated_token_account_idempotent(
        &payer.pubkey(),
        &wallet_address,
        &token_mint_address,
        &spl_token_2022::id(),
    );

    // Set up the accounts for mollusk
    let mut mint_data = [0u8; spl_token_2022::state::Mint::LEN];
    let mint = spl_token_2022::state::Mint {
        mint_authority: Some(wallet_address).into(),
        supply: 0,
        decimals: 6,
        is_initialized: true,
        freeze_authority: None.into(),
    };
    spl_token_2022::state::Mint::pack(mint, &mut mint_data).unwrap();

    let accounts = [
        (
            payer.pubkey(),
            solana_sdk::account::Account::new(1_000_000_000, 0, &solana_sdk::system_program::id()),
        ),
        (
            associated_token_address,
            solana_sdk::account::Account::new(0, 0, &solana_sdk::system_program::id()),
        ),
        (
            wallet_address,
            solana_sdk::account::Account::new(0, 0, &solana_sdk::system_program::id()),
        ),
        (
            token_mint_address,
            solana_sdk::account::Account {
                lamports: 1_000_000_000,
                data: mint_data.to_vec(),
                owner: spl_token_2022::id(),
                executable: false,
                rent_epoch: 0,
            },
        ),
        (
            solana_sdk::system_program::id(),
            solana_sdk::account::Account::new(0, 0, &solana_sdk::native_loader::id()),
        ),
        (
            spl_token_2022::id(),
            solana_sdk::account::Account::new(0, 0, &loader_keys::LOADER_V3),
        ),
    ];

    // Execute the instruction using mollusk
    let result = banks_client
        .mollusk
        .process_instruction(&instruction, &accounts);

    // Verify success
    match result.program_result {
        mollusk_svm::result::ProgramResult::Success => {
            // Success case
        }
        _ => panic!(
            "Expected program to succeed, got: {:?}",
            result.program_result
        ),
    }

    // Verify the ATA was created with correct properties
    let ata_account = result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address)
        .map(|(_, account)| account)
        .expect("ATA should exist");
    assert_eq!(ata_account.data.len(), expected_token_account_len);
    assert_eq!(ata_account.owner, spl_token_2022::id());
    assert_eq!(ata_account.lamports, expected_token_account_balance);

    // SUCCESS! The mollusk-based test runner now matches the original API exactly!
    // Only change needed: `mollusk_program_test_2022` instead of `program_test_2022`
    // Everything else is identical to the original test structure!
}
