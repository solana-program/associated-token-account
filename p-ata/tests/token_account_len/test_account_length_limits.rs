use {
    mollusk_svm::result::Check,
    ata_mollusk_harness::{
        build_create_ata_instruction, create_ata_test_accounts, setup_mollusk_with_programs,
        CreateAtaInstructionType,
    },
    solana_pubkey::Pubkey,
    solana_sdk::{program_error::ProgramError, signature::Keypair, signer::Signer},
    std::vec,
};

use pinocchio_ata_program::entrypoint::MAX_SANE_ACCOUNT_LENGTH;

#[test]
fn test_account_length_too_small_cases() {
    let test_cases = vec![
        (0, "zero length"),
        (1, "single byte"),
        (10, "very small"),
        (164, "just under SPL Token minimum"),
        (165, "SPL Token minimum but insufficient for Token-2022"),
        (169, "just under Token-2022 minimum"),
    ];

    let mollusk = setup_mollusk_with_programs(&spl_token_2022::id());
    let program_id = spl_associated_token_account::id();

    let wallet = Pubkey::new_unique();
    let mint = Pubkey::new_unique();
    let token_program = spl_token_2022::id();
    let payer = Keypair::new();
    let (ata_address, bump) = Pubkey::find_program_address(
        &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()],
        &program_id,
    );

    for (length, _) in &test_cases {
        let instruction = build_create_ata_instruction(
            program_id,
            payer.pubkey(),
            ata_address,
            wallet,
            mint,
            token_program,
            CreateAtaInstructionType::Create {
                bump: Some(bump),
                account_len: Some(*length),
            },
        );

        let accounts = create_ata_test_accounts(&payer, ata_address, wallet, mint, token_program);

        // Token-2022 requires minimum 170 bytes (165 base + 5 for ImmutableOwner)
        // All lengths under 170 should fail with InvalidAccountData
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[Check::err(ProgramError::InvalidAccountData)],
        );
    }
}

#[test]
fn test_account_length_boundary_values() {
    let test_cases = vec![
        (170, "standard extended token account"),
        (512, "small extension"),
        (1024, "medium extension"),
        (MAX_SANE_ACCOUNT_LENGTH - 1, "just under limit"),
        (MAX_SANE_ACCOUNT_LENGTH, "at limit"),
        (MAX_SANE_ACCOUNT_LENGTH + 1, "just over limit"),
        (4096, "way over limit"),
        (65535, "max over limit"),
    ];

    let mollusk = setup_mollusk_with_programs(&spl_token_2022::id());
    let program_id = spl_associated_token_account::id();

    let wallet = Pubkey::new_unique();
    let mint = Pubkey::new_unique();
    let token_program = spl_token_2022::id();
    let payer = Keypair::new();
    let (ata_address, bump) = Pubkey::find_program_address(
        &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()],
        &program_id,
    );

    for (length, _) in test_cases {
        let instruction = build_create_ata_instruction(
            program_id,
            payer.pubkey(),
            ata_address,
            wallet,
            mint,
            token_program,
            CreateAtaInstructionType::Create {
                bump: Some(bump),
                account_len: Some(length),
            },
        );

        let accounts = create_ata_test_accounts(&payer, ata_address, wallet, mint, token_program);

        if length <= MAX_SANE_ACCOUNT_LENGTH {
            mollusk.process_and_validate_instruction(&instruction, &accounts, &[Check::success()]);
        } else {
            mollusk.process_and_validate_instruction(
                &instruction,
                &accounts,
                &[Check::err(ProgramError::InvalidInstructionData)],
            );
        }
    }
}
