mod utils;

use solana_program_test::tokio;
use {
    solana_program_pack::Pack, solana_pubkey::Pubkey, solana_sdk::signature::Signer,
    spl_associated_token_account_interface::address::get_associated_token_address,
    spl_token_interface::state::Account, utils::*,
};

#[tokio::test]
async fn success_create() {
    let wallet_address = Pubkey::new_unique();
    let token_mint_address = Pubkey::new_unique();
    let associated_token_address =
        get_associated_token_address(&wallet_address, &token_mint_address);

    let mollusk = setup_mollusk_with_programs(&spl_token_interface::id());
    let payer = solana_sdk::signer::keypair::Keypair::new();
    let mut accounts = create_mollusk_base_accounts_with_token(&payer, &spl_token_interface::id());
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
    // Ensure the derived ATA exists as a placeholder system account for Mollusk
    ensure_system_account_exists(&mut accounts, associated_token_address, 0);
    let expected_token_account_len = Account::LEN;
    let expected_token_account_balance =
        solana_sdk::rent::Rent::default().minimum_balance(expected_token_account_len);

    let instruction = build_create_ata_instruction(
        spl_associated_token_account::id(),
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        spl_token_interface::id(),
        CreateAtaInstructionType::Create {
            bump: None,
            account_len: None,
        },
    );
    let pr = process_and_merge_instruction(&mollusk, &instruction, &mut accounts);
    assert!(pr.is_ok());
    let associated_account = accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address)
        .expect("associated_account not none")
        .1
        .clone();
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

    let mollusk = setup_mollusk_with_programs(&spl_token_interface::id());
    let payer = solana_sdk::signer::keypair::Keypair::new();
    let mut accounts = create_mollusk_base_accounts_with_token(&payer, &spl_token_interface::id());

    // Add mint and wallet to accounts
    accounts.push((
        token_mint_address,
        account_builder::AccountBuilder::mint(6, &payer.pubkey()),
    ));
    accounts.push((
        wallet_address,
        account_builder::AccountBuilder::system_account(1_000_000),
    ));
    // Ensure the derived ATA exists as a placeholder system account for Mollusk
    ensure_system_account_exists(&mut accounts, associated_token_address, 0);

    let expected_token_account_len = Account::LEN;
    let expected_token_account_balance =
        solana_sdk::rent::Rent::default().minimum_balance(expected_token_account_len);

    // Use legacy-style instruction (empty data to simulate deprecated function)
    let mut instruction = build_create_ata_instruction(
        spl_associated_token_account::id(),
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        spl_token_interface::id(),
        CreateAtaInstructionType::Create {
            bump: None,
            account_len: None,
        },
    );
    instruction.data = vec![]; // Legacy deprecated instruction had empty data

    let pr = process_and_merge_instruction(&mollusk, &instruction, &mut accounts);
    assert!(pr.is_ok());
    let associated_account = accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address)
        .expect("associated_account not none")
        .1
        .clone();
    assert_eq!(associated_account.data.len(), expected_token_account_len);
    assert_eq!(associated_account.owner, spl_token_interface::id());
    assert_eq!(associated_account.lamports, expected_token_account_balance);
}
