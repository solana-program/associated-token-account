mod common;

use {
    common::expected_bump,
    mollusk_svm_result::Check,
    pinocchio_associated_token_account_interface::instruction::CreateMode,
    pinocchio_token::instructions::{Batch, InitializeAccount, InitializeImmutableOwner},
    solana_address::Address,
    solana_instruction::AccountMeta,
    solana_program_error::ProgramError,
    solana_program_pack::Pack,
    spl_associated_token_account_mollusk_harness::{
        AtaProgram, AtaTestHarness, CreateAtaInstructionType,
        token_2022_immutable_owner_account_len, token_2022_immutable_owner_rent_exempt_balance,
        token_account_rent_exempt_balance,
    },
    test_case::test_matrix,
};

fn expected_account_len(token_program_id: &Address) -> usize {
    if *token_program_id == spl_token_2022_interface::id() {
        token_2022_immutable_owner_account_len()
    } else {
        spl_token_interface::state::Account::LEN
    }
}

fn expected_rent_exempt_balance(token_program_id: &Address) -> u64 {
    if *token_program_id == spl_token_2022_interface::id() {
        token_2022_immutable_owner_rent_exempt_balance()
    } else {
        token_account_rent_exempt_balance()
    }
}

const TOKEN_2022_RENT_INIT_BATCH_DATA: &[u8] = &[
    Batch::DISCRIMINATOR,
    InitializeImmutableOwner::ACCOUNTS_LEN as u8,
    InitializeImmutableOwner::DATA_LEN as u8,
    InitializeImmutableOwner::DISCRIMINATOR,
    InitializeAccount::ACCOUNTS_LEN as u8,
    InitializeAccount::DATA_LEN as u8,
    InitializeAccount::DISCRIMINATOR,
];

#[test_matrix([spl_token_interface::id(), spl_token_2022_interface::id()])]
fn create_with_args_idempotent_accepts_existing_ata(token_program_id: Address) {
    let mut harness =
        AtaTestHarness::new_with_ata_program(&token_program_id, AtaProgram::Pinocchio)
            .with_wallet_and_mint(1_000_000, 6);
    let wallet = harness.wallet.unwrap();
    harness.insert_token_account_at_ata_address(wallet);

    let instruction =
        harness.build_create_ata_instruction(CreateAtaInstructionType::CreateWithArgs {
            mode: CreateMode::Idempotent,
            bump: None,
            account_len: None,
            rent_sysvar: false,
        });

    harness
        .ctx
        .process_and_validate_instruction(&instruction, &[Check::success()]);
}

#[test_matrix([spl_token_interface::id(), spl_token_2022_interface::id()])]
fn create_with_args_always_rejects_existing_ata(token_program_id: Address) {
    let mut harness =
        AtaTestHarness::new_with_ata_program(&token_program_id, AtaProgram::Pinocchio)
            .with_wallet_and_mint(1_000_000, 6);
    let wallet = harness.wallet.unwrap();
    harness.insert_token_account_at_ata_address(wallet);

    let instruction =
        harness.build_create_ata_instruction(CreateAtaInstructionType::CreateWithArgs {
            mode: CreateMode::Always,
            bump: None,
            account_len: None,
            rent_sysvar: false,
        });

    harness
        .ctx
        .process_and_validate_instruction(&instruction, &[Check::err(ProgramError::IllegalOwner)]);
}

#[test_matrix(
    [spl_token_interface::id(), spl_token_2022_interface::id()],
    [CreateMode::Always, CreateMode::Idempotent]
)]
fn create_with_args_rejects_wrong_rent_account(token_program_id: Address, mode: CreateMode) {
    let mut harness =
        AtaTestHarness::new_with_ata_program(&token_program_id, AtaProgram::Pinocchio)
            .with_wallet_and_mint(1_000_000, 6);
    let incorrect_rent_sysvar = Address::new_unique();
    let bump = expected_bump(&harness);

    let mut instruction =
        harness.build_create_ata_instruction(CreateAtaInstructionType::CreateWithArgs {
            mode,
            bump: Some(bump),
            account_len: Some(expected_account_len(&token_program_id) as u32),
            rent_sysvar: true,
        });
    let rent_sysvar = instruction.accounts[6].pubkey;
    instruction.accounts[6] = AccountMeta::new_readonly(incorrect_rent_sysvar, false);
    instruction
        .accounts
        .push(AccountMeta::new_readonly(rent_sysvar, false));

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[Check::err(ProgramError::InvalidArgument)],
    );
}

#[test_matrix(
    [spl_token_interface::id(), spl_token_2022_interface::id()],
    [CreateMode::Always, CreateMode::Idempotent],
    [false, true],
    [false, true],
    [false, true]
)]
fn create_with_args_accepts_optional_inputs(
    token_program_id: Address,
    mode: CreateMode,
    bump: bool,
    account_len: bool,
    rent_sysvar: bool,
) {
    let mut harness =
        AtaTestHarness::new_with_ata_program(&token_program_id, AtaProgram::Pinocchio)
            .with_wallet_and_mint(1_000_000, 6);
    let bump = if bump {
        Some(expected_bump(&harness))
    } else {
        None
    };
    let account_len = if account_len {
        Some(expected_account_len(&token_program_id) as u32)
    } else {
        None
    };
    let instruction =
        harness.build_create_ata_instruction(CreateAtaInstructionType::CreateWithArgs {
            mode,
            bump,
            account_len,
            rent_sysvar,
        });
    let ata_address = harness.ata_address.unwrap();

    let result = harness.ctx.process_and_validate_instruction(
        &instruction,
        &[
            Check::success(),
            Check::account(&ata_address)
                .space(expected_account_len(&token_program_id))
                .owner(&token_program_id)
                .lamports(expected_rent_exempt_balance(&token_program_id))
                .build(),
        ],
    );

    if token_program_id == spl_token_2022_interface::id() && rent_sysvar {
        assert!(result.inner_instructions.iter().any(|inner_instruction| {
            inner_instruction.instruction.data.as_slice() == TOKEN_2022_RENT_INIT_BATCH_DATA
        }));
    }
}

#[test_matrix(
    [spl_token_interface::id(), spl_token_2022_interface::id()],
    [CreateMode::Always, CreateMode::Idempotent]
)]
fn create_with_args_ignores_trailing_accounts_after_rent(
    token_program_id: Address,
    mode: CreateMode,
) {
    let mut harness =
        AtaTestHarness::new_with_ata_program(&token_program_id, AtaProgram::Pinocchio)
            .with_wallet_and_mint(1_000_000, 6);
    let trailing_account = Address::new_unique();
    let bump = expected_bump(&harness);

    let mut instruction =
        harness.build_create_ata_instruction(CreateAtaInstructionType::CreateWithArgs {
            mode,
            bump: Some(bump),
            account_len: Some(expected_account_len(&token_program_id) as u32),
            rent_sysvar: true,
        });
    instruction
        .accounts
        .push(AccountMeta::new_readonly(trailing_account, false));
    let ata_address = harness.ata_address.unwrap();

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[
            Check::success(),
            Check::account(&ata_address)
                .space(expected_account_len(&token_program_id))
                .owner(&token_program_id)
                .lamports(expected_rent_exempt_balance(&token_program_id))
                .build(),
        ],
    );
}
