use {
    super::test_bump_utils::{
        find_wallet_with_on_curve_attack_opportunity, setup_mollusk_for_bump_tests,
    },
    mollusk_svm::result::Check,
    solana_keypair::Keypair,
    solana_pubkey::Pubkey,
    solana_sdk::program_error::ProgramError,
    solana_signer::Signer,
    spl_associated_token_account_mollusk_harness::{
        account_builder::AccountBuilder, build_create_ata_instruction, create_ata_test_accounts,
        CreateAtaInstructionType,
    },
};

#[test]
fn test_rejects_on_curve_address_in_idempotent_check() {
    let ata_program_id = spl_associated_token_account::id();
    let token_program_id = spl_token_interface::id();
    let mint_pubkey = Pubkey::new_unique();
    let payer = Keypair::new();

    // Find a wallet where find_program_address returns bump 253
    // This means bump 254 and 255 are both on curve
    let first_off_curve_bump = 253u8;

    let (wallet, _, on_curve_attack_address, attack_bump) =
        find_wallet_with_on_curve_attack_opportunity(
            first_off_curve_bump,
            token_program_id.as_array(),
            mint_pubkey.as_array(),
            ata_program_id.as_array(),
        )
        .expect("Could not find wallet with canonical bump 253 and on-curve attack opportunity");

    let mollusk = setup_mollusk_for_bump_tests(&token_program_id);

    // Step 1: Manually create a token account at the on-curve address
    // This simulates the attack where someone creates an account at an on-curve (invalid PDA) address
    let mut manual_token_account =
        AccountBuilder::token_account(&mint_pubkey, &wallet, 0, &spl_token_interface::id());
    manual_token_account.lamports = 2_039_280;
    manual_token_account.owner = token_program_id;

    let mut accounts = create_ata_test_accounts(
        &payer,
        on_curve_attack_address,
        wallet,
        mint_pubkey,
        token_program_id,
    );
    // Pre-existing account at on-curve address.
    accounts[1].1 = manual_token_account;

    // Step 2: Try to validate the account with CreateIdempotent using the on-curve address
    let idempotent_instruction = build_create_ata_instruction(
        ata_program_id,
        payer.pubkey(),
        on_curve_attack_address,
        wallet,
        mint_pubkey,
        token_program_id,
        CreateAtaInstructionType::CreateIdempotent {
            bump: Some(attack_bump),
        },
    );

    // This should fail with InvalidSeeds because the address is on-curve (invalid PDA)
    // The is_off_curve check in check_idempotent_account prevents this attack
    mollusk.process_and_validate_instruction(
        &idempotent_instruction,
        &accounts,
        &[Check::err(ProgramError::InvalidSeeds)],
    );
}
