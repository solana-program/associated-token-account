mod common;

use {
    common::expected_bump,
    mollusk_svm_programs_token::token,
    mollusk_svm_result::Check,
    pinocchio_associated_token_account_interface::instruction::CreateMode,
    solana_address::Address,
    solana_instruction::{AccountMeta, error::InstructionError},
    solana_program_error::ProgramError,
    solana_program_option::COption,
    solana_program_pack::Pack,
    spl_associated_token_account_interface::address::get_associated_token_address_and_bump_seed,
    spl_associated_token_account_mollusk_harness::{
        AccountBuilder, AtaProgram, AtaTestHarness, CreateAtaInstructionType,
        token_account_rent_exempt_balance,
    },
    test_case::{test_case, test_matrix},
};

const MINT: Address = Address::from_str_const("8N6gdBxJaZUG9cBnSSaHDsx7vMeQ4VR1LmCmk9SCu38s");

fn ata_address_with_bump(
    wallet: &Address,
    mint: &Address,
    token_program_id: &Address,
    bump: u8,
) -> Address {
    Address::derive_address(
        &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
        Some(bump),
        &spl_associated_token_account_interface::program::id(),
    )
}

fn create_mint_account(token_program_id: &Address) -> AtaTestHarness {
    let mut harness = AtaTestHarness::new_with_ata_program(token_program_id, AtaProgram::Pinocchio);
    harness.ctx.account_store.borrow_mut().insert(
        MINT,
        token::create_account_for_mint(spl_token_interface::state::Mint {
            mint_authority: COption::None,
            supply: 0,
            decimals: 6,
            is_initialized: true,
            freeze_authority: COption::None,
        }),
    );
    harness.mint = Some(MINT);
    harness
}

#[test_case(
    255,
    254,
    Address::from_str_const("1117mWrzzrZr312ebPDHu8tbfMwFNvCvMbr6WepCNG")
)]
#[test_case(
    254,
    253,
    Address::from_str_const("111FJo4zLAGU9nzTWa6EnbV4VAmtG4FR8kcokrtZYr")
)]
#[test_case(
    253,
    250,
    Address::from_str_const("3zPynWFGj3nyJBtHhCy8UEJoGvbn1TmgHca2afHTSByQ")
)]
fn create_with_args_rejects_lower_off_curve_bump_hint(
    canonical_bump: u8,
    lower_bump: u8,
    wallet: Address,
) {
    let token_program_id = spl_token_interface::id();

    let (canonical_addr, actual_canonical_bump) = get_associated_token_address_and_bump_seed(
        &wallet,
        &MINT,
        &spl_associated_token_account_interface::program::id(),
        &token_program_id,
    );
    assert_eq!(actual_canonical_bump, canonical_bump);
    let lower_bump_addr = ata_address_with_bump(&wallet, &MINT, &token_program_id, lower_bump);
    assert!(!canonical_addr.is_on_curve());
    assert!(!lower_bump_addr.is_on_curve());
    let first_skipped_bump = lower_bump.checked_add(1).unwrap();
    for skipped_bump in first_skipped_bump..canonical_bump {
        assert!(
            ata_address_with_bump(&wallet, &MINT, &token_program_id, skipped_bump).is_on_curve()
        );
    }

    let mut harness = create_mint_account(&token_program_id);
    harness.ensure_account_exists_with_lamports(wallet, 1_000_000);
    harness.wallet = Some(wallet);
    let mut instruction =
        harness.build_create_ata_instruction(CreateAtaInstructionType::CreateWithArgs {
            mode: CreateMode::Always,
            bump: Some(lower_bump),
            account_len: None,
            rent_sysvar: false,
        });
    instruction.accounts[1] = AccountMeta::new(lower_bump_addr, false);
    harness
        .ctx
        .process_and_validate_instruction(&instruction, &[Check::err(ProgramError::InvalidSeeds)]);
}

#[test]
fn create_with_args_rejects_max_bump_hint_when_derived_address_mismatches_account() {
    let token_program_id = spl_token_interface::id();
    let mut harness = create_mint_account(&token_program_id);

    let wallet = Address::from_str_const("931gKnvtYWR12hueG8Zb1yDigAntDghosriFUoK1Fr8P");
    let (_, canonical_bump) = get_associated_token_address_and_bump_seed(
        &wallet,
        &MINT,
        &spl_associated_token_account_interface::program::id(),
        &token_program_id,
    );

    // This fixture keeps `u8::MAX` on-curve, so the higher-bump loop is skipped and
    // rejection must come from the derived-address mismatch.
    assert_eq!(canonical_bump, 253);
    assert!(ata_address_with_bump(&wallet, &MINT, &token_program_id, 255).is_on_curve());

    harness.ensure_account_exists_with_lamports(wallet, 1_000_000);
    harness.wallet = Some(wallet);

    let instruction =
        harness.build_create_ata_instruction(CreateAtaInstructionType::CreateWithArgs {
            mode: CreateMode::Always,
            bump: Some(255),
            account_len: None,
            rent_sysvar: false,
        });

    harness
        .ctx
        .process_and_validate_instruction(&instruction, &[Check::err(ProgramError::InvalidSeeds)]);
}

#[test]
fn create_with_args_rejects_canonical_bump_hint_for_wrong_ata_account() {
    let token_program_id = spl_token_interface::id();
    let mut harness =
        AtaTestHarness::new_with_ata_program(&token_program_id, AtaProgram::Pinocchio)
            .with_wallet_and_mint(1_000_000, 6);
    let wallet = harness.wallet.unwrap();
    let mint = harness.mint.unwrap();
    let bump = expected_bump(&harness);

    // The bump is canonical for this wallet/mint, but this account is not the ATA PDA derived
    // from those seeds, so the address match must still fail.
    let wrong_ata_account_addr = Address::new_unique();

    harness.ctx.account_store.borrow_mut().insert(
        wrong_ata_account_addr,
        AccountBuilder::token_account(&mint, &wallet, 0, &token_program_id),
    );

    let mut instruction =
        harness.build_create_ata_instruction(CreateAtaInstructionType::CreateWithArgs {
            mode: CreateMode::Idempotent,
            bump: Some(bump),
            account_len: None,
            rent_sysvar: false,
        });
    instruction.accounts[1] = AccountMeta::new(wrong_ata_account_addr, false);

    harness
        .ctx
        .process_and_validate_instruction(&instruction, &[Check::err(ProgramError::InvalidSeeds)]);
}

#[test_case(CreateMode::Idempotent)]
#[test_case(CreateMode::Always)]
fn create_with_args_rejects_on_curve_bump_hint(mode: CreateMode) {
    let token_program_id = spl_token_interface::id();
    let mut harness = create_mint_account(&token_program_id);

    let wallet = Address::from_str_const("931gKnvtYWR12hueG8Zb1yDigAntDghosriFUoK1Fr8P");

    // The same wallet/mint with bump 254 derives this on-curve address
    let on_curve_bump_addr =
        Address::from_str_const("2XughgGgvRsf8VJiteF7ZEYF2jZbCTcTAFoEfrCfUXs7");

    let canonical_bump = 253u8;
    let attack_bump = canonical_bump.checked_add(1).unwrap();
    let (_, actual_bump) = get_associated_token_address_and_bump_seed(
        &wallet,
        &MINT,
        &spl_associated_token_account_interface::program::id(),
        &token_program_id,
    );
    assert_eq!(actual_bump, canonical_bump);
    assert_eq!(
        ata_address_with_bump(&wallet, &MINT, &token_program_id, attack_bump),
        on_curve_bump_addr
    );
    assert!(on_curve_bump_addr.is_on_curve());

    harness.ensure_account_exists_with_lamports(wallet, 1_000_000);
    harness.wallet = Some(wallet);

    if mode == CreateMode::Idempotent {
        harness.ctx.account_store.borrow_mut().insert(
            on_curve_bump_addr,
            AccountBuilder::token_account(&MINT, &wallet, 0, &token_program_id),
        );
    }
    let mut instruction =
        harness.build_create_ata_instruction(CreateAtaInstructionType::CreateWithArgs {
            mode,
            bump: Some(attack_bump),
            account_len: None,
            rent_sysvar: false,
        });
    instruction.accounts[1] = AccountMeta::new(on_curve_bump_addr, false);

    match mode {
        CreateMode::Idempotent => {
            harness.ctx.process_and_validate_instruction(
                &instruction,
                &[Check::err(ProgramError::InvalidSeeds)],
            );
        }
        CreateMode::Always => {
            harness.ctx.process_and_validate_instruction(
                &instruction,
                &[Check::instruction_err(
                    InstructionError::ProgramFailedToComplete,
                )],
            );
        }
    }
}

#[test_matrix(
    [
        (
            255,
            Address::from_str_const("1117mWrzzrZr312ebPDHu8tbfMwFNvCvMbr6WepCNG")
        ),
        (
            253,
            Address::from_str_const("931gKnvtYWR12hueG8Zb1yDigAntDghosriFUoK1Fr8P")
        )
    ],
    [CreateMode::Always, CreateMode::Idempotent]
)]
fn create_with_args_accepts_canonical_bump_hint(bump_fixture: (u8, Address), mode: CreateMode) {
    let token_program_id = spl_token_interface::id();
    let mut harness = create_mint_account(&token_program_id);
    let (expected_bump, wallet) = bump_fixture;
    let (ata_address, canonical_bump) = get_associated_token_address_and_bump_seed(
        &wallet,
        &MINT,
        &spl_associated_token_account_interface::program::id(),
        &token_program_id,
    );
    assert_eq!(canonical_bump, expected_bump);
    assert_eq!(
        ata_address_with_bump(&wallet, &MINT, &token_program_id, canonical_bump),
        ata_address
    );
    assert!(!ata_address.is_on_curve());

    harness.ensure_account_exists_with_lamports(wallet, 1_000_000);
    harness.wallet = Some(wallet);

    if mode == CreateMode::Idempotent {
        harness.ctx.account_store.borrow_mut().insert(
            ata_address,
            AccountBuilder::token_account(&MINT, &wallet, 0, &token_program_id),
        );
    }

    let instruction =
        harness.build_create_ata_instruction(CreateAtaInstructionType::CreateWithArgs {
            mode,
            bump: Some(canonical_bump),
            account_len: None,
            rent_sysvar: false,
        });

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[
            Check::success(),
            Check::account(&ata_address)
                .space(spl_token_interface::state::Account::LEN)
                .owner(&token_program_id)
                .lamports(token_account_rent_exempt_balance())
                .build(),
        ],
    );
}

#[test_matrix([spl_token_interface::id(), spl_token_2022_interface::id()])]
fn create_with_args_idempotent_accepts_existing_ata_with_canonical_bump_hint(
    token_program_id: Address,
) {
    let mut harness =
        AtaTestHarness::new_with_ata_program(&token_program_id, AtaProgram::Pinocchio)
            .with_wallet_and_mint(1_000_000, 6);
    let wallet = harness.wallet.unwrap();
    let bump = expected_bump(&harness);
    harness.insert_token_account_at_ata_address(wallet);

    let instruction =
        harness.build_create_ata_instruction(CreateAtaInstructionType::CreateWithArgs {
            mode: CreateMode::Idempotent,
            bump: Some(bump),
            account_len: None,
            rent_sysvar: false,
        });

    harness
        .ctx
        .process_and_validate_instruction(&instruction, &[Check::success()]);
}
