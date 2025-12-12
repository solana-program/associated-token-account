use {
    super::test_bump_utils::{
        find_wallet_with_on_curve_attack_opportunity, setup_mollusk_for_bump_tests,
    },
    mollusk_svm::{program::loader_keys::LOADER_V3, result::Check},
    ata_mollusk_harness::{
        account_builder::AccountBuilder, build_create_ata_instruction, CreateAtaInstructionType,
        NATIVE_LOADER_ID,
    },
    solana_pubkey::Pubkey,
    solana_sdk::{
        account::Account, program_error::ProgramError, signature::Keypair, signer::Signer,
    },
    solana_sdk_ids::{system_program, sysvar},
    std::vec::Vec,
};

/// Simulate an attacker manually creating an account outside of the ATA program.
fn create_manual_token_account(mint: Pubkey, owner: Pubkey, token_program: Pubkey) -> Account {
    let token_account_data = AccountBuilder::token_account(&mint, &owner, 0, &spl_token::id()).data;

    Account {
        lamports: 2_039_280,
        data: token_account_data,
        owner: token_program,
        executable: false,
        rent_epoch: 0,
    }
}

#[test]
fn test_rejects_on_curve_address_in_idempotent_check() {
    let ata_program_id = spl_associated_token_account::id();
    let token_program_id = spl_token::id();
    let mint_pubkey = Pubkey::new_unique();
    let payer = Keypair::new();

    // Find a wallet where find_program_address returns bump 253
    // This means bump 254 and 255 are both on curve
    let first_off_curve_bump = 253u8;

    let (wallet, _, on_curve_attack_address, attack_bump) =
        find_wallet_with_on_curve_attack_opportunity(
            first_off_curve_bump,
            &token_program_id.to_bytes(),
            &mint_pubkey.to_bytes(),
            &ata_program_id.to_bytes(),
        )
        .expect("Could not find wallet with canonical bump 253 and on-curve attack opportunity");

    let mollusk = setup_mollusk_for_bump_tests(&token_program_id.to_bytes());

    // Step 1: Manually create a token account at the on-curve address
    // This simulates the attack where someone creates an account at an on-curve (invalid PDA) address
    let manual_token_account = create_manual_token_account(
        mint_pubkey,
        Pubkey::new_from_array(wallet),
        token_program_id,
    );

    let accounts = vec![
        (
            payer.pubkey(),
            Account::new(1_000_000_000, 0, &system_program::id()),
        ),
        (
            Pubkey::new_from_array(on_curve_attack_address),
            manual_token_account,
        ), // Pre-existing account at on-curve address
        (
            Pubkey::new_from_array(wallet),
            Account::new(0, 0, &system_program::id()),
        ),
        (
            mint_pubkey,
            Account {
                lamports: 1_461_600,
                data: AccountBuilder::mint(6, &spl_token::id()).data,
                owner: token_program_id,
                executable: false,
                rent_epoch: 0,
            },
        ),
        (
            system_program::id(),
            Account {
                lamports: 0,
                data: Vec::new(),
                owner: NATIVE_LOADER_ID,
                executable: true,
                rent_epoch: 0,
            },
        ),
        (
            token_program_id,
            Account {
                lamports: 0,
                data: Vec::new(),
                owner: LOADER_V3,
                executable: true,
                rent_epoch: 0,
            },
        ),
        (sysvar::rent::id(), Account::new(1009200, 17, &sysvar::id())),
    ];

    // Step 2: Try to validate the account with CreateIdempotent using the on-curve address
    let idempotent_instruction = build_create_ata_instruction(
        ata_program_id,
        payer.pubkey(),
        Pubkey::new_from_array(on_curve_attack_address),
        Pubkey::new_from_array(wallet),
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
