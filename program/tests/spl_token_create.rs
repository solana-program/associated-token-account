mod utils;

#[allow(deprecated)]
use spl_associated_token_account::create_associated_token_account as deprecated_create_associated_token_account;
use {
    solana_pubkey::Pubkey,
    solana_sdk::{program_pack::Pack, signature::Signer, transaction::Transaction},
    spl_associated_token_account_interface::{
        address::get_associated_token_address, instruction::create_associated_token_account,
    },
    spl_token_interface::state::Account,
    utils::*,
};

#[tokio::test]
async fn success_create() {
    let wallet_address = Pubkey::new_unique();
    let token_mint_address = Pubkey::new_unique();
    let associated_token_address =
        get_associated_token_address(&wallet_address, &token_mint_address);

    let mollusk = setup_mollusk_with_programs(&spl_token::id());
    let payer = solana_sdk::signer::keypair::Keypair::new();
    let mut accounts = create_mollusk_base_accounts_with_token(&payer, &spl_token::id());
    accounts.extend([
        (
            token_mint_address,
            account_builder::AccountBuilder::mint(6, &payer.pubkey()),
        ),
        (
            wallet_address,
            account_builder::AccountBuilder::system_account(1_000_000),
        ),
    ]);
    let (expected_token_account_len, expected_token_account_balance) =
        (Account::LEN, TOKEN_ACCOUNT_RENT_EXEMPT);

    let instruction = build_create_ata_instruction(
        spl_associated_token_account::id(),
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        spl_token::id(),
        CreateAtaInstructionType::Create {
            bump: None,
            account_len: None,
        },
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
    assert_eq!(associated_account.owner, spl_token_interface::id());
    assert_eq!(associated_account.lamports, expected_token_account_balance);
}

#[tokio::test]
async fn success_using_deprecated_instruction_creator() {
    let wallet_address = Pubkey::new_unique();
    let token_mint_address = Pubkey::new_unique();
    let associated_token_address =
        get_associated_token_address(&wallet_address, &token_mint_address);

    let mollusk = setup_mollusk_with_programs(&spl_token::id());
    let payer = solana_sdk::signer::keypair::Keypair::new();
    let mut accounts = create_mollusk_base_accounts_with_token(&payer, &spl_token::id());

    // Add mint and wallet to accounts
    accounts.push((
        token_mint_address,
        account_builder::AccountBuilder::mint(6, &payer.pubkey()),
    ));
    accounts.push((
        wallet_address,
        account_builder::AccountBuilder::system_account(1_000_000),
    ));

    let expected_token_account_len = Account::LEN;
    let expected_token_account_balance = TOKEN_ACCOUNT_RENT_EXEMPT;

    // Use legacy-style instruction (empty data to simulate deprecated function)
    let mut instruction = build_create_ata_instruction(
        spl_associated_token_account::id(),
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        spl_token::id(),
        CreateAtaInstructionType::Create {
            bump: None,
            account_len: None,
        },
    );
    instruction.data = vec![]; // Legacy deprecated instruction had empty data

    let result = mollusk.process_instruction(&instruction, &accounts);
    assert!(result.program_result.is_ok());
    let associated_account = result
        .resulting_accounts
        .into_iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address)
        .expect("associated_account not none")
        .1;
    assert_eq!(associated_account.data.len(), expected_token_account_len);
    assert_eq!(associated_account.owner, spl_token_interface::id());
    assert_eq!(associated_account.lamports, expected_token_account_balance);
}
