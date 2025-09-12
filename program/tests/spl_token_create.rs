mod utils;

use crate::utils::test_util_exports::{
    account_builder, build_create_ata_instruction, ctx_ensure_system_account_exists,
    ctx_ensure_system_accounts_with_lamports, setup_context_with_programs,
    CreateAtaInstructionType,
};

use {
    mollusk_svm::result::Check, solana_program_pack::Pack, solana_pubkey::Pubkey,
    solana_sdk::signature::Signer,
    spl_associated_token_account_interface::address::get_associated_token_address,
    spl_token_interface::state::Account,
};

#[test]
fn success_create() {
    let wallet_address = Pubkey::new_unique();
    let token_mint_address = Pubkey::new_unique();
    let associated_token_address =
        get_associated_token_address(&wallet_address, &token_mint_address);

    let ctx = setup_context_with_programs(&spl_token_interface::id());
    let payer = solana_sdk::signer::keypair::Keypair::new();
    let expected_token_account_balance =
        solana_sdk::rent::Rent::default().minimum_balance(Account::LEN);
    ctx.account_store.borrow_mut().insert(
        payer.pubkey(),
        account_builder::AccountBuilder::system_account(expected_token_account_balance),
    );
    ctx.account_store.borrow_mut().insert(
        token_mint_address,
        account_builder::AccountBuilder::mint(6, &payer.pubkey()),
    );
    ctx_ensure_system_accounts_with_lamports(&ctx, &[(wallet_address, 1_000_000)]);
    ctx_ensure_system_account_exists(&ctx, associated_token_address, 0);
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
        CreateAtaInstructionType::default(),
    );
    ctx.process_and_validate_instruction(
        &instruction,
        &[
            Check::success(),
            Check::account(&associated_token_address)
                .space(expected_token_account_len)
                .owner(&spl_token_interface::id())
                .lamports(expected_token_account_balance)
                .build(),
        ],
    );
}

#[test]
fn success_using_deprecated_instruction_creator() {
    let wallet_address = Pubkey::new_unique();
    let token_mint_address = Pubkey::new_unique();
    let associated_token_address =
        get_associated_token_address(&wallet_address, &token_mint_address);

    let ctx = setup_context_with_programs(&spl_token_interface::id());
    let payer = solana_sdk::signer::keypair::Keypair::new();
    let expected_token_account_balance =
        solana_sdk::rent::Rent::default().minimum_balance(Account::LEN);
    ctx.account_store.borrow_mut().insert(
        payer.pubkey(),
        account_builder::AccountBuilder::system_account(expected_token_account_balance),
    );
    ctx.account_store.borrow_mut().insert(
        token_mint_address,
        account_builder::AccountBuilder::mint(6, &payer.pubkey()),
    );
    ctx_ensure_system_accounts_with_lamports(&ctx, &[(wallet_address, 1_000_000)]);
    ctx_ensure_system_account_exists(&ctx, associated_token_address, 0);

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
        CreateAtaInstructionType::default(),
    );
    instruction.data = vec![]; // Legacy deprecated instruction had empty data

    ctx.process_and_validate_instruction(
        &instruction,
        &[
            Check::success(),
            Check::account(&associated_token_address)
                .space(expected_token_account_len)
                .owner(&spl_token_interface::id())
                .lamports(expected_token_account_balance)
                .build(),
        ],
    );
}
