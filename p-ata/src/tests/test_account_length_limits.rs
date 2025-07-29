use {
    mollusk_svm::{program::loader_keys::LOADER_V3, result::Check, Mollusk},
    solana_instruction::{AccountMeta, Instruction},
    solana_pubkey::Pubkey,
    solana_sdk::{program_error::ProgramError, signature::Keypair, signer::Signer},
    solana_sdk_ids::{system_program, sysvar},
    std::{vec, vec::Vec},
};

use crate::entrypoint::MAX_SANE_ACCOUNT_LENGTH;

/// Creates instruction data for account creation with specified account length
fn create_instruction_data_with_length(discriminator: u8, bump: u8, account_len: u16) -> Vec<u8> {
    let len_bytes = account_len.to_le_bytes();
    vec![discriminator, bump, len_bytes[0], len_bytes[1]]
}

#[test]
fn test_account_length_at_max_sane_limit_succeeds() {
    let mut mollusk = Mollusk::default();

    let program_id = spl_associated_token_account::id();
    mollusk.add_program(
        &program_id,
        "target/deploy/pinocchio_ata_program",
        &LOADER_V3,
    );

    mollusk.add_program(
        &spl_token_2022::id(),
        "programs/token-2022/target/deploy/spl_token_2022",
        &LOADER_V3,
    );

    let wallet = Pubkey::new_unique();
    let mint = Pubkey::new_unique();
    let token_program = spl_token_2022::id();
    let payer = Keypair::new();

    // Calculate the ATA address and bump
    let (ata_address, bump) = Pubkey::find_program_address(
        &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()],
        &program_id,
    );

    // Create instruction with account length at max sane limit
    let instruction_data = create_instruction_data_with_length(0, bump, MAX_SANE_ACCOUNT_LENGTH);

    let instruction = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),   // payer
            AccountMeta::new(ata_address, false),     // ATA account
            AccountMeta::new_readonly(wallet, false), // wallet
            AccountMeta::new_readonly(mint, false),   // mint
            AccountMeta::new_readonly(system_program::id(), false), // system program
            AccountMeta::new_readonly(token_program, false), // token program
            AccountMeta::new_readonly(sysvar::rent::id(), false), // rent sysvar
        ],
        data: instruction_data,
    };

    let accounts = crate::tests::test_utils::create_ata_test_accounts(
        &payer,
        ata_address,
        wallet,
        mint,
        token_program,
    );

    mollusk.process_and_validate_instruction(&instruction, &accounts, &[Check::success()]);
}

#[test]
fn test_account_length_over_max_sane_limit_fails() {
    let mut mollusk = Mollusk::default();

    let program_id = spl_associated_token_account::id();
    mollusk.add_program(
        &program_id,
        "target/deploy/pinocchio_ata_program",
        &LOADER_V3,
    );

    mollusk.add_program(
        &spl_token_2022::id(),
        "programs/token-2022/target/deploy/spl_token_2022",
        &LOADER_V3,
    );

    let wallet = Pubkey::new_unique();
    let mint = Pubkey::new_unique();
    let token_program = spl_token_2022::id();
    let payer = Keypair::new();

    // Calculate the ATA address and bump
    let (ata_address, bump) = Pubkey::find_program_address(
        &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()],
        &program_id,
    );

    let instruction_data =
        create_instruction_data_with_length(0, bump, MAX_SANE_ACCOUNT_LENGTH + 1);

    let instruction = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),   // payer
            AccountMeta::new(ata_address, false),     // ATA account
            AccountMeta::new_readonly(wallet, false), // wallet
            AccountMeta::new_readonly(mint, false),   // mint
            AccountMeta::new_readonly(system_program::id(), false), // system program
            AccountMeta::new_readonly(token_program, false), // token program
            AccountMeta::new_readonly(sysvar::rent::id(), false), // rent sysvar
        ],
        data: instruction_data,
    };

    let accounts = crate::tests::test_utils::create_ata_test_accounts(
        &payer,
        ata_address,
        wallet,
        mint,
        token_program,
    );

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(ProgramError::InvalidInstructionData)],
    );
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

    for (length, _description) in test_cases {
        let mut mollusk = Mollusk::default();

        let program_id = spl_associated_token_account::id();
        mollusk.add_program(
            &program_id,
            "target/deploy/pinocchio_ata_program",
            &LOADER_V3,
        );

        mollusk.add_program(
            &spl_token_2022::id(),
            "programs/token-2022/target/deploy/spl_token_2022",
            &LOADER_V3,
        );

        let wallet = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let token_program = spl_token_2022::id();
        let payer = Keypair::new();

        // Calculate the ATA address and bump
        let (ata_address, bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()],
            &program_id,
        );

        // Create instruction
        let instruction_data = create_instruction_data_with_length(0, bump, length);

        let instruction = Instruction {
            program_id,
            accounts: vec![
                AccountMeta::new(payer.pubkey(), true),   // payer
                AccountMeta::new(ata_address, false),     // ATA account
                AccountMeta::new_readonly(wallet, false), // wallet
                AccountMeta::new_readonly(mint, false),   // mint
                AccountMeta::new_readonly(system_program::id(), false), // system program
                AccountMeta::new_readonly(token_program, false), // token program
                AccountMeta::new_readonly(sysvar::rent::id(), false), // rent sysvar
            ],
            data: instruction_data,
        };

        let accounts = crate::tests::test_utils::create_ata_test_accounts(
            &payer,
            ata_address,
            wallet,
            mint,
            token_program,
        );

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
