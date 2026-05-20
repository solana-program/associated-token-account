use {
    mollusk_svm_result::Check,
    pinocchio_associated_token_account_interface::instruction::CreateMode,
    solana_address::Address,
    solana_program_error::ProgramError,
    solana_program_pack::Pack,
    solana_rent::Rent,
    spl_associated_token_account_mollusk_harness::{
        AtaProgram, AtaTestHarness, CreateAtaInstructionType,
        token_2022_immutable_owner_account_len, token_account_rent_exempt_balance,
    },
    spl_token_2022_interface::{extension::ExtensionType, state::Account as Token2022Account},
    test_case::{test_case, test_matrix},
};

fn token_2022_harness(transfer_fee_mint: bool) -> AtaTestHarness {
    let harness = AtaTestHarness::new_with_ata_program(
        &spl_token_2022_interface::id(),
        AtaProgram::Pinocchio,
    );

    if transfer_fee_mint {
        harness
            .with_wallet(1_000_000)
            .with_mint_with_extensions(&[ExtensionType::TransferFeeConfig])
            .initialize_transfer_fee(1_000, 100)
            .initialize_mint(0)
    } else {
        harness.with_wallet_and_mint(1_000_000, 6)
    }
}

fn token_2022_required_account_len(transfer_fee_mint: bool) -> usize {
    if transfer_fee_mint {
        ExtensionType::try_calculate_account_len::<Token2022Account>(&[
            ExtensionType::ImmutableOwner,
            ExtensionType::TransferFeeAmount,
        ])
        .unwrap()
    } else {
        token_2022_immutable_owner_account_len()
    }
}

#[test_matrix([spl_token_interface::id(), spl_token_2022_interface::id()], [1, u32::MAX])]
fn idempotent_existing_ata_ignores_hint(token_program_id: Address, account_len: u32) {
    let mut harness =
        AtaTestHarness::new_with_ata_program(&token_program_id, AtaProgram::Pinocchio)
            .with_wallet_and_mint(1_000_000, 6);
    let wallet = harness.wallet.unwrap();
    harness.insert_token_account_at_ata_address(wallet);
    let instruction =
        harness.build_create_ata_instruction(CreateAtaInstructionType::CreateWithArgs {
            mode: CreateMode::Idempotent,
            bump: None,
            account_len: Some(account_len),
            rent_sysvar: false,
        });

    harness
        .ctx
        .process_and_validate_instruction(&instruction, &[Check::success()]);
}

#[test]
fn always_mode_with_existing_ata_fails_before_hint_check() {
    let mut harness = token_2022_harness(false);
    let wallet = harness.wallet.unwrap();
    harness.insert_token_account_at_ata_address(wallet);
    let instruction =
        harness.build_create_ata_instruction(CreateAtaInstructionType::CreateWithArgs {
            mode: CreateMode::Always,
            bump: None,
            account_len: Some(u32::MAX),
            rent_sysvar: false,
        });

    // u32::MAX would fail with InvalidArgument if the hint were checked first.
    harness
        .ctx
        .process_and_validate_instruction(&instruction, &[Check::err(ProgramError::IllegalOwner)]);
}

#[test_matrix(
    [CreateMode::Always, CreateMode::Idempotent],
    [1, spl_token_interface::state::Account::LEN as u32, u32::MAX]
)]
fn spl_token_always_allocates_165_bytes_ignoring_hint(mode: CreateMode, account_len_hint: u32) {
    let mut harness =
        AtaTestHarness::new_with_ata_program(&spl_token_interface::id(), AtaProgram::Pinocchio)
            .with_wallet_and_mint(1_000_000, 6);
    let account_len = spl_token_interface::state::Account::LEN;
    let instruction =
        harness.build_create_ata_instruction(CreateAtaInstructionType::CreateWithArgs {
            mode,
            bump: None,
            account_len: Some(account_len_hint),
            rent_sysvar: false,
        });
    let ata_address = harness.ata_address.unwrap();

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[
            Check::success(),
            Check::account(&ata_address)
                .space(account_len)
                .owner(&spl_token_interface::id())
                .lamports(token_account_rent_exempt_balance())
                .build(),
        ],
    );
}

#[test]
fn token_2022_fails_when_hint_exceeds_system_max_account_size() {
    let mut harness = token_2022_harness(false);
    let instruction =
        harness.build_create_ata_instruction(CreateAtaInstructionType::CreateWithArgs {
            mode: CreateMode::Always,
            bump: None,
            account_len: Some(u32::MAX),
            rent_sysvar: false,
        });

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[Check::err(ProgramError::InvalidArgument)],
    );
}

#[test_case(false; "base mint")]
#[test_case(true; "transfer fee mint")]
fn token_2022_fails_when_hint_is_tiny(transfer_fee_mint: bool) {
    let mut harness = token_2022_harness(transfer_fee_mint);
    let instruction =
        harness.build_create_ata_instruction(CreateAtaInstructionType::CreateWithArgs {
            mode: CreateMode::Always,
            bump: None,
            account_len: Some(1),
            rent_sysvar: false,
        });

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[Check::err(ProgramError::InvalidAccountData)],
    );
}

#[test_case(false; "base mint")]
#[test_case(true; "transfer fee mint")]
fn token_2022_fails_when_hint_is_one_byte_short(transfer_fee_mint: bool) {
    let mut harness = token_2022_harness(transfer_fee_mint);
    let account_len = token_2022_required_account_len(transfer_fee_mint);
    let instruction =
        harness.build_create_ata_instruction(CreateAtaInstructionType::CreateWithArgs {
            mode: CreateMode::Always,
            bump: None,
            account_len: Some(account_len.checked_sub(1).unwrap() as u32),
            rent_sysvar: false,
        });

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[Check::err(ProgramError::InvalidAccountData)],
    );
}

#[test]
fn token_2022_extended_fails_when_hint_omits_required_extension_space() {
    let mut harness = token_2022_harness(true);
    let account_len = token_2022_immutable_owner_account_len();
    let instruction =
        harness.build_create_ata_instruction(CreateAtaInstructionType::CreateWithArgs {
            mode: CreateMode::Always,
            bump: None,
            account_len: Some(account_len as u32),
            rent_sysvar: false,
        });

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[Check::err(ProgramError::InvalidAccountData)],
    );
}

#[test_case(false; "base mint")]
#[test_case(true; "transfer fee mint")]
fn token_2022_allocates_required_size_without_hint(transfer_fee_mint: bool) {
    let mut harness = token_2022_harness(transfer_fee_mint);
    let account_len = token_2022_required_account_len(transfer_fee_mint);
    let instruction =
        harness.build_create_ata_instruction(CreateAtaInstructionType::CreateWithArgs {
            mode: CreateMode::Always,
            bump: None,
            account_len: None,
            rent_sysvar: false,
        });
    let ata_address = harness.ata_address.unwrap();

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[
            Check::success(),
            Check::account(&ata_address)
                .space(account_len)
                .owner(&spl_token_2022_interface::id())
                .lamports(Rent::default().minimum_balance(account_len))
                .build(),
        ],
    );
}

#[test_matrix(
    [false, true],
    [CreateMode::Always, CreateMode::Idempotent],
    [0usize, 10usize]
)]
fn token_2022_allocates_full_hint_when_valid(
    transfer_fee_mint: bool,
    mode: CreateMode,
    extra_len: usize,
) {
    let mut harness = token_2022_harness(transfer_fee_mint);
    let account_len = token_2022_required_account_len(transfer_fee_mint)
        .checked_add(extra_len)
        .unwrap();
    let instruction =
        harness.build_create_ata_instruction(CreateAtaInstructionType::CreateWithArgs {
            mode,
            bump: None,
            account_len: Some(account_len as u32),
            rent_sysvar: false,
        });
    let ata_address = harness.ata_address.unwrap();

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[
            Check::success(),
            Check::account(&ata_address)
                .space(account_len)
                .owner(&spl_token_2022_interface::id())
                .lamports(Rent::default().minimum_balance(account_len))
                .build(),
        ],
    );
}
