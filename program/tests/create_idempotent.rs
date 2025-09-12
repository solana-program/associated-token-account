mod utils;

use {
    crate::utils::test_util_exports::{
        account_builder, build_create_ata_instruction,
        build_create_ata_instruction_with_system_account, ctx_ensure_system_account_exists,
        ctx_ensure_system_accounts_with_lamports, setup_context_with_programs,
        CreateAtaInstructionType,
    },
    mollusk_svm::result::Check,
    solana_program::instruction::*,
    solana_program_test::*,
    solana_pubkey::Pubkey,
    solana_sdk::{program_error::ProgramError, signature::Signer, signer::keypair::Keypair},
    spl_associated_token_account_interface::address::get_associated_token_address_with_program_id,
    spl_token_2022_interface::{extension::ExtensionType, state::Account},
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

    let ctx = setup_context_with_programs(&spl_token_2022_interface::id());
    let payer = Keypair::new();
    ctx.account_store.borrow_mut().insert(
        token_mint_address,
        account_builder::AccountBuilder::extended_mint(6, &payer.pubkey()),
    );
    ctx_ensure_system_accounts_with_lamports(&ctx, &[(wallet_address, 1_000_000)]);
    ctx_ensure_system_account_exists(&ctx, associated_token_address, 0);
    let rent = solana_sdk::rent::Rent::default();
    let expected_token_account_len =
        ExtensionType::try_calculate_account_len::<Account>(&[ExtensionType::ImmutableOwner])
            .unwrap();
    let expected_token_account_balance = rent.minimum_balance(expected_token_account_len);

    // Fund payer with exactly the token account rent amount.
    ctx.account_store.borrow_mut().insert(
        payer.pubkey(),
        account_builder::AccountBuilder::system_account(expected_token_account_balance),
    );

    let instruction = build_create_ata_instruction_with_system_account(
        &mut Vec::new(),
        spl_associated_token_account::id(),
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        spl_token_2022_interface::id(),
        CreateAtaInstructionType::CreateIdempotent { bump: None },
    );
    ctx.process_and_validate_instruction(
        &instruction,
        &[
            Check::success(),
            Check::account(&associated_token_address)
                .space(expected_token_account_len)
                .owner(&spl_token_2022_interface::id())
                .lamports(expected_token_account_balance)
                .build(),
        ],
    );
    let associated_account = ctx
        .account_store
        .borrow()
        .get(&associated_token_address)
        .cloned()
        .expect("associated account exists");

    // Test failure case: try to Create when ATA already exists as token account
    // Replace any existing account at the ATA address with the token account from the first instruction
    ctx.account_store
        .borrow_mut()
        .insert(associated_token_address, associated_account.clone());

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
        CreateAtaInstructionType::default(),
    );
    // Should fail with IllegalOwner because the account already exists and is owned by token program
    ctx.process_and_validate_instruction(&instruction, &[Check::err(ProgramError::IllegalOwner)]);

    let instruction = build_create_ata_instruction_with_system_account(
        &mut Vec::new(),
        spl_associated_token_account::id(),
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        spl_token_2022_interface::id(),
        CreateAtaInstructionType::CreateIdempotent { bump: None },
    );
    ctx.process_and_validate_instruction(
        &instruction,
        &[
            Check::success(),
            Check::account(&associated_token_address)
                .space(expected_token_account_len)
                .owner(&spl_token_2022_interface::id())
                .lamports(expected_token_account_balance)
                .build(),
        ],
    );
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
    let ctx = setup_context_with_programs(&spl_token_2022_interface::id());
    let payer = Keypair::new();
    let expected_token_account_len =
        ExtensionType::try_calculate_account_len::<Account>(&[ExtensionType::ImmutableOwner])
            .unwrap();
    let expected_token_account_balance =
        solana_sdk::rent::Rent::default().minimum_balance(expected_token_account_len);
    ctx.account_store.borrow_mut().insert(
        payer.pubkey(),
        account_builder::AccountBuilder::system_account(expected_token_account_balance),
    );
    ctx.account_store.borrow_mut().insert(
        token_mint_address,
        account_builder::AccountBuilder::extended_mint(6, &payer.pubkey()),
    );
    ctx.account_store.borrow_mut().insert(
        associated_token_address,
        account_builder::AccountBuilder::token_account(
            &token_mint_address,
            &wrong_owner,
            0,
            &spl_token_2022_interface::id(),
        ),
    );
    ctx_ensure_system_accounts_with_lamports(
        &ctx,
        &[(wallet_address, 1_000_000), (wrong_owner, 1_000_000)],
    );

    let instruction = build_create_ata_instruction_with_system_account(
        &mut Vec::new(),
        spl_associated_token_account::id(),
        payer.pubkey(),
        associated_token_address,
        wallet_address,
        token_mint_address,
        spl_token_2022_interface::id(),
        CreateAtaInstructionType::CreateIdempotent { bump: None },
    );
    ctx.process_and_validate_instruction(
        &instruction,
        &[Check::err(ProgramError::Custom(
            spl_associated_token_account::error::AssociatedTokenAccountError::InvalidOwner as u32,
        ))],
    );
}

#[tokio::test]
async fn fail_non_ata() {
    let token_mint_address = Pubkey::new_unique();
    let wallet_address = Pubkey::new_unique();
    let account = Keypair::new();

    let ctx = setup_context_with_programs(&spl_token_2022_interface::id());
    let payer = Keypair::new();
    // For this test the instruction fails before funding; minimal rent amount is sufficient if needed.
    let expected_token_account_len =
        ExtensionType::try_calculate_account_len::<Account>(&[ExtensionType::ImmutableOwner])
            .unwrap();
    let expected_token_account_balance =
        solana_sdk::rent::Rent::default().minimum_balance(expected_token_account_len);
    ctx.account_store.borrow_mut().insert(
        payer.pubkey(),
        account_builder::AccountBuilder::system_account(expected_token_account_balance),
    );
    ctx.account_store.borrow_mut().insert(
        token_mint_address,
        account_builder::AccountBuilder::extended_mint(6, &payer.pubkey()),
    );
    ctx.account_store.borrow_mut().insert(
        account.pubkey(),
        account_builder::AccountBuilder::token_account(
            &token_mint_address,
            &wallet_address,
            0,
            &spl_token_2022_interface::id(),
        ),
    );
    ctx_ensure_system_accounts_with_lamports(&ctx, &[(wallet_address, 1_000_000)]);

    let mut instruction = build_create_ata_instruction_with_system_account(
        &mut Vec::new(),
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
    ctx.process_and_validate_instruction(&instruction, &[Check::err(ProgramError::InvalidSeeds)]);
}
