mod utils;

use {
    solana_program::{instruction::*, pubkey::Pubkey, system_instruction},
    solana_sdk::{
        signature::Signer,
        signer::keypair::Keypair,
        transaction::{Transaction, TransactionError},
    },
    spl_associated_token_account::instruction::create_associated_token_account,
    spl_associated_token_account_client::address::get_associated_token_address_with_program_id,
    spl_token_2022::{
        error::TokenError,
        extension::{
            transfer_fee, BaseStateWithExtensions, ExtensionType, StateWithExtensionsOwned,
        },
        state::{Account, Mint},
    },
    utils::*,
};

#[tokio::test]
async fn test_associated_token_account_with_transfer_fees() {
    let wallet_sender = Keypair::new();
    let wallet_address_sender = wallet_sender.pubkey();
    let wallet_address_receiver = Pubkey::new_unique();
    let mollusk = setup_mollusk_with_programs(&spl_token_2022::id());
    let payer = Keypair::new();
    let mut accounts = create_mollusk_base_accounts_with_token(&payer, &spl_token_2022::id());

    let mint_account = Keypair::new();
    let token_mint_address = mint_account.pubkey();
    let mint_authority = Keypair::new();
    accounts.extend([
        (
            wallet_address_sender,
            account_builder::AccountBuilder::system_account(1_000_000),
        ),
        (
            wallet_address_receiver,
            account_builder::AccountBuilder::system_account(1_000_000),
        ),
        (
            mint_authority.pubkey(),
            account_builder::AccountBuilder::system_account(1_000_000),
        ),
    ]);
    let space =
        ExtensionType::try_calculate_account_len::<Mint>(&[ExtensionType::TransferFeeConfig])
            .unwrap();
    let maximum_fee = 100;
    // Create mint account
    accounts.push((
        mint_account.pubkey(),
        solana_sdk::account::Account::new(0, 0, &solana_program::system_program::id()),
    ));
    let create_mint_ix = system_instruction::create_account(
        &payer.pubkey(),
        &mint_account.pubkey(),
        MINT_ACCOUNT_RENT_EXEMPT,
        space as u64,
        &spl_token_2022::id(),
    );
    let result = mollusk.process_instruction(&create_mint_ix, &accounts);
    assert!(result.program_result.is_ok());

    // Update accounts with created mint
    if let Some((_, account)) = result
        .resulting_accounts
        .into_iter()
        .find(|(pubkey, _)| *pubkey == mint_account.pubkey())
    {
        if let Some((_, existing_account)) = accounts
            .iter_mut()
            .find(|(pubkey, _)| *pubkey == mint_account.pubkey())
        {
            *existing_account = account;
        }
    }

    // Initialize transfer fee config
    let init_fee_ix = transfer_fee::instruction::initialize_transfer_fee_config(
        &spl_token_2022::id(),
        &token_mint_address,
        Some(&mint_authority.pubkey()),
        Some(&mint_authority.pubkey()),
        1_000,
        maximum_fee,
    )
    .unwrap();
    let result = mollusk.process_instruction(&init_fee_ix, &accounts);
    assert!(result.program_result.is_ok());

    // Update accounts after fee config
    if let Some((_, account)) = result
        .resulting_accounts
        .into_iter()
        .find(|(pubkey, _)| *pubkey == mint_account.pubkey())
    {
        if let Some((_, existing_account)) = accounts
            .iter_mut()
            .find(|(pubkey, _)| *pubkey == mint_account.pubkey())
        {
            *existing_account = account;
        }
    }

    // Initialize mint
    let init_mint_ix = spl_token_2022::instruction::initialize_mint(
        &spl_token_2022::id(),
        &token_mint_address,
        &mint_authority.pubkey(),
        Some(&mint_authority.pubkey()),
        0,
    )
    .unwrap();
    let result = mollusk.process_instruction(&init_mint_ix, &accounts);
    assert!(result.program_result.is_ok());

    // Update accounts after mint initialization
    if let Some((_, account)) = result
        .resulting_accounts
        .into_iter()
        .find(|(pubkey, _)| *pubkey == mint_account.pubkey())
    {
        if let Some((_, existing_account)) = accounts
            .iter_mut()
            .find(|(pubkey, _)| *pubkey == mint_account.pubkey())
        {
            *existing_account = account;
        }
    }

    // create extended ATAs
    let associated_token_address_sender = get_associated_token_address_with_program_id(
        &wallet_address_sender,
        &token_mint_address,
        &spl_token_2022::id(),
    );
    let create_ata_sender_ix = build_create_ata_instruction(
        spl_associated_token_account::id(),
        payer.pubkey(),
        associated_token_address_sender,
        wallet_address_sender,
        token_mint_address,
        spl_token_2022::id(),
        CreateAtaInstructionType::Create {
            bump: None,
            account_len: None,
        },
    );
    let result = mollusk.process_instruction(&create_ata_sender_ix, &accounts);
    assert!(result.program_result.is_ok());

    // Update accounts with created sender ATA
    if let Some((_, account)) = result
        .resulting_accounts
        .into_iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address_sender)
    {
        accounts.push((associated_token_address_sender, account));
    }

    let associated_token_address_receiver = get_associated_token_address_with_program_id(
        &wallet_address_receiver,
        &token_mint_address,
        &spl_token_2022::id(),
    );
    let create_ata_receiver_ix = build_create_ata_instruction(
        spl_associated_token_account::id(),
        payer.pubkey(),
        associated_token_address_receiver,
        wallet_address_receiver,
        token_mint_address,
        spl_token_2022::id(),
        CreateAtaInstructionType::Create {
            bump: None,
            account_len: None,
        },
    );
    let result = mollusk.process_instruction(&create_ata_receiver_ix, &accounts);
    assert!(result.program_result.is_ok());

    // Update accounts with created receiver ATA
    if let Some((_, account)) = result
        .resulting_accounts
        .into_iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address_receiver)
    {
        accounts.push((associated_token_address_receiver, account));
    }

    // mint tokens
    let sender_amount = 50 * maximum_fee;
    let mint_to_ix = spl_token_2022::instruction::mint_to(
        &spl_token_2022::id(),
        &token_mint_address,
        &associated_token_address_sender,
        &mint_authority.pubkey(),
        &[],
        sender_amount,
    )
    .unwrap();
    let result = mollusk.process_instruction(&mint_to_ix, &accounts);
    assert!(result.program_result.is_ok());

    // Update sender account after minting
    if let Some((_, account)) = result
        .resulting_accounts
        .into_iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address_sender)
    {
        if let Some((_, existing_account)) = accounts
            .iter_mut()
            .find(|(pubkey, _)| *pubkey == associated_token_address_sender)
        {
            *existing_account = account;
        }
    }

    // not enough tokens
    let insufficient_transfer_ix = transfer_fee::instruction::transfer_checked_with_fee(
        &spl_token_2022::id(),
        &associated_token_address_sender,
        &token_mint_address,
        &associated_token_address_receiver,
        &wallet_address_sender,
        &[],
        10_001,
        0,
        maximum_fee,
    )
    .unwrap();
    let result = mollusk.process_instruction(&insufficient_transfer_ix, &accounts);
    assert!(result.program_result.is_err());
    assert_eq!(
        result.program_result.unwrap_err(),
        InstructionError::Custom(TokenError::InsufficientFunds as u32).into()
    );

    // success
    let transfer_amount = 500;
    let fee = 50;
    let transfer_ix = transfer_fee::instruction::transfer_checked_with_fee(
        &spl_token_2022::id(),
        &associated_token_address_sender,
        &token_mint_address,
        &associated_token_address_receiver,
        &wallet_address_sender,
        &[],
        transfer_amount,
        0,
        fee,
    )
    .unwrap();
    let result = mollusk.process_instruction(&transfer_ix, &accounts);
    assert!(result.program_result.is_ok());

    // Verify final account states
    let sender_account = result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address_sender)
        .unwrap()
        .1
        .clone();
    let sender_state = StateWithExtensionsOwned::<Account>::unpack(sender_account.data).unwrap();
    assert_eq!(sender_state.base.amount, sender_amount - transfer_amount);
    let extension = sender_state
        .get_extension::<transfer_fee::TransferFeeAmount>()
        .unwrap();
    assert_eq!(extension.withheld_amount, 0.into());

    let receiver_account = result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| *pubkey == associated_token_address_receiver)
        .unwrap()
        .1
        .clone();
    let receiver_state =
        StateWithExtensionsOwned::<Account>::unpack(receiver_account.data).unwrap();
    assert_eq!(receiver_state.base.amount, transfer_amount - fee);
    let extension = receiver_state
        .get_extension::<transfer_fee::TransferFeeAmount>()
        .unwrap();
    assert_eq!(extension.withheld_amount, fee.into());
}
