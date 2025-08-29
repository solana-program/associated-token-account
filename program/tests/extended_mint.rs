mod utils;

use {
    mollusk_svm::result::ProgramResult,
    solana_program_test::tokio,
    solana_pubkey::Pubkey,
    solana_sdk::{
        program_error::ProgramError, rent::Rent, signature::Signer, signer::keypair::Keypair,
    },
    solana_system_interface::instruction as system_instruction,
    spl_token_2022_interface::{
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
    let mollusk = setup_mollusk_with_programs(&spl_token_2022_interface::id());
    let payer = Keypair::new();
    let mut accounts =
        create_mollusk_base_accounts_with_token(&payer, &spl_token_2022_interface::id());

    let mint_account = Keypair::new();
    let token_mint_address = mint_account.pubkey();
    let mint_authority = Keypair::new();
    ensure_system_accounts_with_lamports(
        &mut accounts,
        &[
            (wallet_address_sender, 1_000_000),
            (wallet_address_receiver, 1_000_000),
            (mint_authority.pubkey(), 1_000_000),
        ],
    );
    let space =
        ExtensionType::try_calculate_account_len::<Mint>(&[ExtensionType::TransferFeeConfig])
            .unwrap();
    let maximum_fee = 100;
    // Create mint account
    accounts.push((
        mint_account.pubkey(),
        solana_sdk::account::Account::new(0, 0, &solana_system_interface::program::id()),
    ));
    let mint_rent = Rent::default().minimum_balance(space);
    let create_mint_ix = system_instruction::create_account(
        &payer.pubkey(),
        &mint_account.pubkey(),
        mint_rent as u64,
        space as u64,
        &spl_token_2022_interface::id(),
    );
    let pr = process_and_merge_instruction(&mollusk, &create_mint_ix, &mut accounts);
    assert!(matches!(pr, ProgramResult::Success));

    // Initialize transfer fee config
    let init_fee_ix = transfer_fee::instruction::initialize_transfer_fee_config(
        &spl_token_2022_interface::id(),
        &token_mint_address,
        Some(&mint_authority.pubkey()),
        Some(&mint_authority.pubkey()),
        1_000,
        maximum_fee,
    )
    .unwrap();
    let pr = process_and_merge_instruction(&mollusk, &init_fee_ix, &mut accounts);
    assert!(matches!(pr, ProgramResult::Success));

    // Initialize mint
    let init_mint_ix = spl_token_2022_interface::instruction::initialize_mint(
        &spl_token_2022_interface::id(),
        &token_mint_address,
        &mint_authority.pubkey(),
        Some(&mint_authority.pubkey()),
        0,
    )
    .unwrap();
    let pr = process_and_merge_instruction(&mollusk, &init_mint_ix, &mut accounts);
    assert!(matches!(pr, ProgramResult::Success));

    // create extended ATAs (sender)
    let (associated_token_address_sender, pr) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet_address_sender,
        &token_mint_address,
        &spl_token_2022_interface::id(),
    );
    assert!(matches!(pr, ProgramResult::Success));

    let (associated_token_address_receiver, pr) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet_address_receiver,
        &token_mint_address,
        &spl_token_2022_interface::id(),
    );
    assert!(matches!(pr, ProgramResult::Success));

    // mint tokens
    let sender_amount = 50 * maximum_fee;
    let pr = mint_to_and_merge(
        &mollusk,
        &mut accounts,
        &spl_token_2022_interface::id(),
        &token_mint_address,
        &associated_token_address_sender,
        &mint_authority.pubkey(),
        sender_amount,
    );
    assert!(matches!(pr, ProgramResult::Success));

    // not enough tokens
    let insufficient_transfer_ix = transfer_fee::instruction::transfer_checked_with_fee(
        &spl_token_2022_interface::id(),
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
    let pr = process_and_merge_instruction(&mollusk, &insufficient_transfer_ix, &mut accounts);
    assert!(matches!(pr, ProgramResult::Failure(_)));
    assert_eq!(
        pr,
        ProgramResult::Failure(ProgramError::Custom(
            spl_token_2022_interface::error::TokenError::InsufficientFunds as u32,
        ))
    );

    // success
    let transfer_amount = 500;
    let fee = 50;
    let transfer_ix = transfer_fee::instruction::transfer_checked_with_fee(
        &spl_token_2022_interface::id(),
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
    let pr = process_and_merge_instruction(&mollusk, &transfer_ix, &mut accounts);
    assert!(matches!(pr, ProgramResult::Success));

    // Verify final account states by reading from updated accounts
    let sender_account = get_account(&accounts, associated_token_address_sender);
    let sender_state = StateWithExtensionsOwned::<Account>::unpack(sender_account.data).unwrap();
    assert_eq!(sender_state.base.amount, sender_amount - transfer_amount);
    let extension = sender_state
        .get_extension::<transfer_fee::TransferFeeAmount>()
        .unwrap();
    assert_eq!(extension.withheld_amount, 0.into());

    let receiver_account = get_account(&accounts, associated_token_address_receiver);
    let receiver_state =
        StateWithExtensionsOwned::<Account>::unpack(receiver_account.data).unwrap();
    assert_eq!(receiver_state.base.amount, transfer_amount - fee);
    let extension = receiver_state
        .get_extension::<transfer_fee::TransferFeeAmount>()
        .unwrap();
    assert_eq!(extension.withheld_amount, fee.into());
}
