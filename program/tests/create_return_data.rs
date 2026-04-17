use {
    mollusk_svm::result::Check,
    solana_instruction::AccountMeta,
    solana_program_error::ProgramError,
    solana_pubkey::Pubkey,
    spl_associated_token_account_mollusk_harness::{AtaTestHarness, CreateAtaInstructionType},
};

#[test]
fn create_rejects_nested_return_data_from_mock_token_program_when_child_is_not_forwarded() {
    // The only way a malicious token program could try to spoof ATA's `get_account_data_size()`
    // return data is by doing its own CPI into some child program and letting that child set the
    // transaction return-data slot. That attack requires ATA to forward an extra child-program
    // account into the token CPI. We append the child only to the outer ATA instruction to prove
    // ATA does not forward trailing accounts, so the nested CPI fails with `NotEnoughAccountKeys`
    // before any forged child return data can be produced.

    // `FORWARD_CHILD_RETURN_DATA` in `mock-programs/mock-token-program/src/lib.rs`.
    let mock_behavior = 2;
    let child_program_id = Pubkey::new_from_array([9; 32]);

    let mut harness = AtaTestHarness::new_with_token_program_name(
        &spl_token_2022_interface::id(),
        "mock_token_program",
    )
    .with_wallet(1_000_000)
    .with_raw_mint(
        spl_token_2022_interface::id(),
        1_000_000,
        vec![mock_behavior],
    );
    let mut instruction = harness.build_create_ata_instruction(CreateAtaInstructionType::Create {
        bump: None,
        account_len: None,
        rent_sysvar_via_account: false,
    });
    instruction
        .accounts
        .push(AccountMeta::new_readonly(child_program_id, false));

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[
            Check::err(ProgramError::NotEnoughAccountKeys),
            Check::return_data(&[]),
        ],
    );
}

#[test]
fn create_rejects_missing_account_size_return_data_from_mock_token_program() {
    // `NO_RETURN_DATA` in `mock-programs/mock-token-program/src/lib.rs`.
    let mock_behavior = 0;

    let mut harness = AtaTestHarness::new_with_token_program_name(
        &spl_token_2022_interface::id(),
        "mock_token_program",
    )
    .with_wallet(1_000_000)
    .with_raw_mint(
        spl_token_2022_interface::id(),
        1_000_000,
        vec![mock_behavior],
    );
    let instruction = harness.build_create_ata_instruction(CreateAtaInstructionType::Create {
        bump: None,
        account_len: None,
        rent_sysvar_via_account: false,
    });

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[
            Check::err(ProgramError::InvalidInstructionData),
            Check::return_data(&[]),
        ],
    );
}

#[test]
fn create_rejects_malformed_account_size_return_data_from_mock_token_program() {
    // `MALFORMED_RETURN_DATA` in `mock-programs/mock-token-program/src/lib.rs`.
    let mock_behavior = 1;

    let mut harness = AtaTestHarness::new_with_token_program_name(
        &spl_token_2022_interface::id(),
        "mock_token_program",
    )
    .with_wallet(1_000_000)
    .with_raw_mint(
        spl_token_2022_interface::id(),
        1_000_000,
        vec![mock_behavior],
    );
    let instruction = harness.build_create_ata_instruction(CreateAtaInstructionType::Create {
        bump: None,
        account_len: None,
        rent_sysvar_via_account: false,
    });

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[
            Check::err(ProgramError::InvalidInstructionData),
            Check::return_data(&[1, 2, 3, 4]),
        ],
    );
}
