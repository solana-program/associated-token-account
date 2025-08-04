//! Migrated test for extended mint functionality with token-2022 transfer fees using mollusk
#![cfg(test)]

use {
    mollusk_svm::result::Check,
    pinocchio_ata_program::test_utils::{
        build_create_ata_instruction, create_mollusk_base_accounts_with_token_and_wallet,
        setup_mollusk_with_programs, CreateAtaInstructionType,
    },
    solana_pubkey::Pubkey,
    solana_sdk::{
        account::Account, program_error::ProgramError, signature::Keypair, signer::Signer,
    },
    solana_system_interface::{instruction as system_instruction, program as system_program},
    spl_associated_token_account_client::address::get_associated_token_address_with_program_id,
    spl_token_2022::{
        extension::{transfer_fee, ExtensionType, StateWithExtensionsOwned},
        state::{Account as TokenAccount, Mint},
    },
};

#[test]
fn test_associated_token_account_with_transfer_fees() {
    let ata_program_id = spl_associated_token_account::id();
    let token_program_id = spl_token_2022::id();

    let wallet_sender = Keypair::new();
    let wallet_address_sender = wallet_sender.pubkey();
    let wallet_address_receiver = Pubkey::new_unique();
    let mint_account = Keypair::new();
    let token_mint_address = mint_account.pubkey();
    let mint_authority = Keypair::new();
    let payer = Keypair::new();

    let mollusk = setup_mollusk_with_programs(&token_program_id);

    // Step 1: Create the mint account
    let space =
        ExtensionType::try_calculate_account_len::<Mint>(&[ExtensionType::TransferFeeConfig])
            .unwrap();
    let rent_lamports = 5_000_000; // Approximate rent for extended mint

    let create_mint_ix = system_instruction::create_account(
        &payer.pubkey(),
        &token_mint_address,
        rent_lamports,
        space as u64,
        &token_program_id,
    );

    let mut accounts = create_mollusk_base_accounts_with_token_and_wallet(
        &payer,
        &wallet_address_sender,
        &token_program_id,
    );

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

    // Step 2: Initialize transfer fee config
    let maximum_fee = 100;
    let transfer_fee_basis_points = 1_000;

    let init_transfer_fee_ix = transfer_fee::instruction::initialize_transfer_fee_config(
        &token_program_id,
        &token_mint_address,
        Some(&mint_authority.pubkey()),
        Some(&mint_authority.pubkey()),
        transfer_fee_basis_points,
        maximum_fee,
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

    mollusk.process_and_validate_instruction(&init_transfer_fee_ix, &accounts, &[Check::success()]);

    // Step 3: Initialize mint
    let init_mint_ix = spl_token_2022::instruction::initialize_mint(
        &token_program_id,
        &token_mint_address,
        &mint_authority.pubkey(),
        Some(&mint_authority.pubkey()),
        0, // decimals
    )
    .unwrap();

    // Update accounts with transfer fee config
    let fee_result = mollusk.process_instruction(&init_transfer_fee_ix, &accounts);
    let fee_mint = fee_result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == token_mint_address)
        .map(|(_, account)| account.clone())
        .expect("Mint with transfer fee should exist");

    accounts
        .iter_mut()
        .find(|(pubkey, _)| *pubkey == token_mint_address)
        .map(|(_, account)| *account = fee_mint);

    mollusk.process_and_validate_instruction(&init_mint_ix, &accounts, &[Check::success()]);

    // Step 4: Create associated token addresses
    let associated_token_address_sender = get_associated_token_address_with_program_id(
        &wallet_address_sender,
        &token_mint_address,
        &token_program_id,
    );
    let associated_token_address_receiver = get_associated_token_address_with_program_id(
        &wallet_address_receiver,
        &token_mint_address,
        &token_program_id,
    );

    // Step 5: Create sender's associated token account
    let create_sender_ix = build_create_ata_instruction(
        ata_program_id,
        payer.pubkey(),
        associated_token_address_sender,
        wallet_address_sender,
        token_mint_address,
        token_program_id,
        CreateAtaInstructionType::Create {
            bump: None,
            account_len: None,
        },
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

    // Add wallet addresses and ATA accounts
    accounts.extend([
        (
            wallet_address_sender,
            Account::new(0, 0, &system_program::id()),
        ),
        (
            wallet_address_receiver,
            Account::new(0, 0, &system_program::id()),
        ),
        (
            associated_token_address_sender,
            Account::new(0, 0, &system_program::id()),
        ),
    ]);

    mollusk.process_and_validate_instruction(&create_sender_ix, &accounts, &[Check::success()]);

    // Step 6: Create receiver's associated token account
    let create_receiver_ix = build_create_ata_instruction(
        ata_program_id,
        payer.pubkey(),
        associated_token_address_receiver,
        wallet_address_receiver,
        token_mint_address,
        token_program_id,
        CreateAtaInstructionType::Create {
            bump: None,
            account_len: None,
        },
    );

    // Update accounts with created sender ATA
    let sender_result = mollusk.process_instruction(&create_sender_ix, &accounts);
    let sender_ata = sender_result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address_sender)
        .map(|(_, account)| account.clone())
        .expect("Sender ATA should be created");

    accounts
        .iter_mut()
        .find(|(pubkey, _)| *pubkey == associated_token_address_sender)
        .map(|(_, account)| *account = sender_ata);

    // Add receiver ATA account
    accounts.push((
        associated_token_address_receiver,
        Account::new(0, 0, &system_program::id()),
    ));

    mollusk.process_and_validate_instruction(&create_receiver_ix, &accounts, &[Check::success()]);

    // Step 7: Mint tokens to sender
    let sender_amount = 50 * maximum_fee;

    let mint_to_ix = spl_token_2022::instruction::mint_to(
        &token_program_id,
        &token_mint_address,
        &associated_token_address_sender,
        &mint_authority.pubkey(),
        &[],
        sender_amount,
    )
    .unwrap();

    // Update accounts with created receiver ATA
    let receiver_result = mollusk.process_instruction(&create_receiver_ix, &accounts);
    let receiver_ata = receiver_result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address_receiver)
        .map(|(_, account)| account.clone())
        .expect("Receiver ATA should be created");

    accounts
        .iter_mut()
        .find(|(pubkey, _)| *pubkey == associated_token_address_receiver)
        .map(|(_, account)| *account = receiver_ata);

    mollusk.process_and_validate_instruction(&mint_to_ix, &accounts, &[Check::success()]);

    // Step 8: Test insufficient funds transfer (should fail)
    let insufficient_transfer_ix = transfer_fee::instruction::transfer_checked_with_fee(
        &token_program_id,
        &associated_token_address_sender,
        &token_mint_address,
        &associated_token_address_receiver,
        &wallet_address_sender,
        &[],
        10_001, // More than available
        0,      // decimals
        maximum_fee,
    )
    .unwrap();

    // Update accounts with minted tokens
    let mint_to_result = mollusk.process_instruction(&mint_to_ix, &accounts);
    let updated_sender = mint_to_result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address_sender)
        .map(|(_, account)| account.clone())
        .expect("Sender should have minted tokens");

    accounts
        .iter_mut()
        .find(|(pubkey, _)| *pubkey == associated_token_address_sender)
        .map(|(_, account)| *account = updated_sender);

    // This should fail due to insufficient funds
    mollusk.process_and_validate_instruction(
        &insufficient_transfer_ix,
        &accounts,
        &[Check::err(ProgramError::Custom(1))], // InsufficientFunds
    );

    // Step 9: Test successful transfer with fees
    let transfer_amount = 500;
    let fee = 50;

    let successful_transfer_ix = transfer_fee::instruction::transfer_checked_with_fee(
        &token_program_id,
        &associated_token_address_sender,
        &token_mint_address,
        &associated_token_address_receiver,
        &wallet_address_sender,
        &[],
        transfer_amount,
        0, // decimals
        fee,
    )
    .unwrap();

    mollusk.process_and_validate_instruction(
        &successful_transfer_ix,
        &accounts,
        &[Check::success()],
    );

    // Verify final state
    let final_result = mollusk.process_instruction(&successful_transfer_ix, &accounts);

    let final_sender = final_result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address_sender)
        .map(|(_, account)| account)
        .expect("Sender account should exist");

    let final_receiver = final_result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address_receiver)
        .map(|(_, account)| account)
        .expect("Receiver account should exist");

    // Parse and verify account states
    let sender_state =
        StateWithExtensionsOwned::<TokenAccount>::unpack(final_sender.data.clone()).unwrap();
    assert_eq!(sender_state.base.amount, sender_amount - transfer_amount);

    let receiver_state =
        StateWithExtensionsOwned::<TokenAccount>::unpack(final_receiver.data.clone()).unwrap();
    assert_eq!(receiver_state.base.amount, transfer_amount - fee);
}
