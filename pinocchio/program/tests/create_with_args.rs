use {
    mollusk_svm_result::Check,
    pinocchio_associated_token_account_interface::instruction::CreateMode,
    solana_address::Address,
    solana_instruction::{AccountMeta, Instruction},
    solana_program_error::ProgramError,
    solana_program_pack::Pack,
    spl_associated_token_account_interface::address::get_associated_token_address_and_bump_seed,
    spl_associated_token_account_mollusk_harness::{
        AtaProgram, AtaTestHarness, CreateAtaInstructionType,
        token_2022_immutable_owner_account_len, token_2022_immutable_owner_rent_exempt_balance,
        token_account_rent_exempt_balance,
    },
    test_case::{test_case, test_matrix},
};

fn create_with_args_instruction_with_account_len(
    harness: &mut AtaTestHarness,
    mode: CreateMode,
    account_len: u64,
) -> Instruction {
    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let (_, bump) = get_associated_token_address_and_bump_seed(
        &wallet,
        &mint,
        &spl_associated_token_account_interface::program::id(),
        &harness.token_program_id,
    );

    harness.build_create_ata_instruction(CreateAtaInstructionType::CreateWithArgs {
        mode,
        bump,
        account_len,
    })
}

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

#[test_case(vec![3, 0, 254, 165, 0, 0, 0, 0, 0, 0], ProgramError::InvalidInstructionData; "truncated")]
#[test_case(vec![3, 0, 254, 165, 0, 0, 0, 0, 0, 0, 0, 0], ProgramError::InvalidInstructionData; "extra_byte")]
#[test_case(vec![3, 2, 254, 165, 0, 0, 0, 0, 0, 0, 0], ProgramError::InvalidInstructionData; "invalid_mode")]
fn create_with_args_rejects_non_canonical_payload(data: Vec<u8>, expected_error: ProgramError) {
    let token_program_id = spl_token_interface::id();
    let mut harness =
        AtaTestHarness::new_with_ata_program(&token_program_id, AtaProgram::Pinocchio)
            .with_wallet_and_mint(1_000_000, 6);
    let mut instruction = create_with_args_instruction_with_account_len(
        &mut harness,
        CreateMode::Always,
        expected_account_len(&token_program_id) as u64,
    );
    instruction.data = data;

    harness
        .ctx
        .process_and_validate_instruction(&instruction, &[Check::err(expected_error)]);
}

#[test_matrix(
    [spl_token_interface::id(), spl_token_2022_interface::id()],
    [CreateMode::Always, CreateMode::Idempotent]
)]
fn create_with_args_rejects_missing_rent_account(token_program_id: Address, mode: CreateMode) {
    let mut harness =
        AtaTestHarness::new_with_ata_program(&token_program_id, AtaProgram::Pinocchio)
            .with_wallet_and_mint(1_000_000, 6);
    let mut instruction = create_with_args_instruction_with_account_len(
        &mut harness,
        mode,
        expected_account_len(&token_program_id) as u64,
    );
    instruction.accounts.truncate(6);

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[Check::err(ProgramError::NotEnoughAccountKeys)],
    );
}

#[test_matrix(
    [spl_token_interface::id(), spl_token_2022_interface::id()],
    [CreateMode::Always, CreateMode::Idempotent]
)]
fn create_with_args_requires_rent_at_expected_account_position(
    token_program_id: Address,
    mode: CreateMode,
) {
    let mut harness =
        AtaTestHarness::new_with_ata_program(&token_program_id, AtaProgram::Pinocchio)
            .with_wallet_and_mint(1_000_000, 6);
    let incorrect_rent_sysvar = Address::new_unique();

    let mut instruction = create_with_args_instruction_with_account_len(
        &mut harness,
        mode,
        expected_account_len(&token_program_id) as u64,
    );
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
    [CreateMode::Always, CreateMode::Idempotent]
)]
fn create_with_args_accepts_fresh_account(token_program_id: Address, mode: CreateMode) {
    let mut harness =
        AtaTestHarness::new_with_ata_program(&token_program_id, AtaProgram::Pinocchio)
            .with_wallet_and_mint(1_000_000, 6);
    let instruction = create_with_args_instruction_with_account_len(
        &mut harness,
        mode,
        expected_account_len(&token_program_id) as u64,
    );
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

    let mut instruction = create_with_args_instruction_with_account_len(
        &mut harness,
        mode,
        expected_account_len(&token_program_id) as u64,
    );
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
