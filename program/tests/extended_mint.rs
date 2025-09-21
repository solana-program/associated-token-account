use {
    ata_mollusk_harness::AtaTestHarness,
    mollusk_svm::result::Check,
    solana_program_error::ProgramError,
    spl_token_2022_interface::{
        extension::{
            transfer_fee, BaseStateWithExtensions, ExtensionType, StateWithExtensionsOwned,
        },
        state::Account,
    },
};

#[test]
fn test_associated_token_account_with_transfer_fees() {
    let maximum_fee = 100;
    let transfer_fee_basis_points = 1_000;
    let (harness, receiver_wallet) = AtaTestHarness::new(&spl_token_2022_interface::id())
        .with_wallet(1_000_000)
        .with_additional_wallet(1_000_000);
    let mut harness = harness
        .with_mint_with_extensions(&[ExtensionType::TransferFeeConfig])
        .initialize_transfer_fee(transfer_fee_basis_points, maximum_fee)
        .initialize_mint(0)
        .with_ata();
    let (sender_pubkey, mint, sender_ata, receiver_ata) = (
        harness.wallet.unwrap(),
        harness.mint.unwrap(),
        harness.ata_address.unwrap(),
        harness.create_ata_for_owner(receiver_wallet, 1_000_000),
    );
    harness.mint_tokens(50 * maximum_fee);

    // Insufficient funds transfer
    harness.ctx.process_and_validate_instruction(
        &transfer_fee::instruction::transfer_checked_with_fee(
            &spl_token_2022_interface::id(),
            &sender_ata,
            &mint,
            &receiver_ata,
            &sender_pubkey,
            &[],
            10_001,
            0,
            maximum_fee,
        )
        .unwrap(),
        &[Check::err(ProgramError::Custom(
            spl_token_2022_interface::error::TokenError::InsufficientFunds as u32,
        ))],
    );

    // Successful transfer
    let (transfer_amount, fee) = (500, 50);
    harness.ctx.process_and_validate_instruction(
        &transfer_fee::instruction::transfer_checked_with_fee(
            &spl_token_2022_interface::id(),
            &sender_ata,
            &mint,
            &receiver_ata,
            &sender_pubkey,
            &[],
            transfer_amount,
            0,
            fee,
        )
        .unwrap(),
        &[Check::success()],
    );

    // Verify final account states
    let sender_state =
        StateWithExtensionsOwned::<Account>::unpack(harness.get_account(sender_ata).data).unwrap();
    assert_eq!(sender_state.base.amount, 50 * maximum_fee - transfer_amount);
    assert_eq!(
        sender_state
            .get_extension::<transfer_fee::TransferFeeAmount>()
            .unwrap()
            .withheld_amount,
        0.into()
    );

    let receiver_state =
        StateWithExtensionsOwned::<Account>::unpack(harness.get_account(receiver_ata).data)
            .unwrap();
    assert_eq!(receiver_state.base.amount, transfer_amount - fee);
    assert_eq!(
        receiver_state
            .get_extension::<transfer_fee::TransferFeeAmount>()
            .unwrap()
            .withheld_amount,
        fee.into()
    );
}
