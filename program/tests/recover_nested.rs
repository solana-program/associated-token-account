mod utils;

use {
    crate::utils::test_util_exports::{
        account_builder, create_associated_token_account_mollusk,
        create_mollusk_base_accounts_with_token, create_test_mint, ensure_program_accounts_present,
        ensure_recover_nested_accounts, ensure_system_accounts_with_lamports, get_account,
        process_and_validate_then_merge, TestHarness,
    },
    mollusk_svm::result::{config::Config, Check},
    solana_program::program_pack::Pack,
    solana_pubkey::Pubkey,
    solana_sdk::{
        account::Account, instruction::AccountMeta, program_error::ProgramError, signature::Signer,
        signer::keypair::Keypair,
    },
    spl_associated_token_account_interface::{
        address::get_associated_token_address_with_program_id, instruction,
    },
    spl_token_2022_interface::{
        extension::StateWithExtensionsOwned, state::Account as TokenAccount,
    },
    utils::*,
};

/// Ensure all accounts required by a recover_nested instruction are provided to Mollusk
fn create_mint_mollusk(
    mollusk: &mollusk_svm::Mollusk,
    accounts: &mut Vec<(Pubkey, Account)>,
    payer: &Keypair,
    program_id: &Pubkey,
) -> (Pubkey, Keypair) {
    let mint_account = Keypair::new();
    let mint_authority = Keypair::new();
    accounts.extend([(
        mint_authority.pubkey(),
        account_builder::AccountBuilder::system_account(1_000_000),
    )]);
    let mint_accounts = create_test_mint(
        mollusk,
        &mint_account,
        &mint_authority,
        payer,
        program_id,
        0,
    );
    accounts.extend(mint_accounts.into_iter().filter(|(pubkey, _)| {
        *pubkey == mint_account.pubkey() || *pubkey == mint_authority.pubkey()
    }));
    (mint_account.pubkey(), mint_authority)
}

const TEST_MINT_AMOUNT: u64 = 100;

const VALIDATION_CONFIG: Config = Config {
    panic: true,
    verbose: true,
};

struct RecoverNestedTestContext<'a> {
    mollusk: &'a mollusk_svm::Mollusk,
    accounts: &'a mut Vec<(Pubkey, Account)>,
    program_id: &'a Pubkey,
    nested_mint: Pubkey,
    nested_mint_authority: &'a Keypair,
    nested_associated_token_address: Pubkey,
    destination_token_address: Pubkey,
    wallet: &'a Keypair,
    recover_instruction: solana_program::instruction::Instruction,
    expected_error: Option<ProgramError>,
}

fn execute_recover_nested_test_scenario(context: &mut RecoverNestedTestContext) {
    let initial_lamports = get_account(context.accounts, context.wallet.pubkey()).lamports;
    mint_test_tokens_to_nested_account(context);

    if let Some(expected_error) = context.expected_error.as_ref() {
        context.mollusk.process_and_validate_instruction(
            &context.recover_instruction,
            context.accounts,
            &[Check::err(expected_error.clone())],
        );
    } else {
        execute_successful_recovery_and_validate(context, initial_lamports);
    }
}

fn mint_test_tokens_to_nested_account(context: &mut RecoverNestedTestContext) {
    let mint_to_fn = if *context.program_id == spl_token_interface::id() {
        spl_token_interface::instruction::mint_to
    } else if *context.program_id == spl_token_2022_interface::id() {
        spl_token_2022_interface::instruction::mint_to
    } else {
        panic!("Unsupported token program id: {}", context.program_id);
    };
    let mint_to_ix = mint_to_fn(
        context.program_id,
        &context.nested_mint,
        &context.nested_associated_token_address,
        &context.nested_mint_authority.pubkey(),
        &[],
        TEST_MINT_AMOUNT,
    )
    .unwrap();
    process_and_validate_then_merge(
        context.mollusk,
        &mint_to_ix,
        context.accounts,
        &[Check::success()],
    );
}

fn execute_successful_recovery_and_validate(
    context: &mut RecoverNestedTestContext,
    initial_wallet_lamports: u64,
) {
    process_and_validate_then_merge(
        context.mollusk,
        &context.recover_instruction,
        context.accounts,
        &[Check::success()],
    );
    let result = validate_destination_token_account(context);
    validate_wallet_lamport_recovery(context, initial_wallet_lamports, result);
}

fn validate_destination_token_account(
    context: &RecoverNestedTestContext,
) -> mollusk_svm::result::InstructionResult {
    let destination_account = get_account(context.accounts, context.destination_token_address);
    let result = mollusk_svm::result::InstructionResult {
        resulting_accounts: vec![(
            context.destination_token_address,
            destination_account.clone(),
        )],
        ..Default::default()
    };
    result.run_checks::<mollusk_svm::Mollusk>(
        &[Check::account(&context.destination_token_address)
            .owner(context.program_id)
            .rent_exempt()
            .build()],
        &VALIDATION_CONFIG,
        context.mollusk,
    );
    let destination_state =
        StateWithExtensionsOwned::<TokenAccount>::unpack(destination_account.data).unwrap();
    assert_eq!(destination_state.base.amount, TEST_MINT_AMOUNT);
    result
}

fn validate_wallet_lamport_recovery(
    context: &RecoverNestedTestContext,
    initial_lamports: u64,
    mut result: mollusk_svm::result::InstructionResult,
) {
    let wallet_account = get_account(context.accounts, context.wallet.pubkey());
    let expected_final_lamports =
        initial_lamports.saturating_add(calculate_ata_rent_for_program(context.program_id));
    result.resulting_accounts = vec![(context.wallet.pubkey(), wallet_account.clone())];
    result.run_checks::<mollusk_svm::Mollusk>(
        &[Check::account(&context.wallet.pubkey())
            .lamports(expected_final_lamports)
            .build()],
        &VALIDATION_CONFIG,
        context.mollusk,
    );
}

fn calculate_ata_rent_for_program(program_id: &Pubkey) -> u64 {
    let ata_space = if *program_id == spl_token_2022_interface::id() {
        spl_token_2022_interface::extension::ExtensionType::try_calculate_account_len::<
            spl_token_2022_interface::state::Account,
        >(&[spl_token_2022_interface::extension::ExtensionType::ImmutableOwner])
        .expect("failed to calculate Token-2022 account length")
    } else {
        spl_token_interface::state::Account::LEN
    };
    solana_sdk::rent::Rent::default().minimum_balance(ata_space)
}

fn check_same_mint_mollusk(program_id: &Pubkey) {
    let mut harness = TestHarness::new(program_id)
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();

    // Create nested ATA and mint tokens to it (not to the main ATA)
    let nested_ata = harness.create_nested_ata(owner_ata);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    // Build and execute recover instruction
    let recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    harness.execute_success(&recover_instruction);

    // Validate the recovery worked - tokens should be in the destination ATA (owner_ata)
    let destination_account = harness.get_account(owner_ata);
    let destination_state = if *program_id == spl_token_2022_interface::id() {
        let state = StateWithExtensionsOwned::<spl_token_2022_interface::state::Account>::unpack(
            destination_account.data,
        )
        .unwrap();
        state.base.amount
    } else {
        let state = spl_token_interface::state::Account::unpack(&destination_account.data).unwrap();
        state.amount
    };
    assert_eq!(destination_state, TEST_MINT_AMOUNT);
}

#[test]
fn success_same_mint_2022() {
    check_same_mint_mollusk(&spl_token_2022_interface::id());
}

#[test]
fn success_same_mint() {
    check_same_mint_mollusk(&spl_token_interface::id());
}

fn check_different_mints_mollusk(program_id: &Pubkey) {
    let mollusk = setup_mollusk_with_programs(program_id);
    let payer = Keypair::new();
    let wallet = Keypair::new();
    let mut accounts = create_mollusk_base_accounts_with_token(&payer, program_id);
    ensure_system_accounts_with_lamports(&mut accounts, &[(wallet.pubkey(), 1_000_000)]);

    let (owner_mint, _owner_mint_authority) =
        create_mint_mollusk(&mollusk, &mut accounts, &payer, program_id);
    let (nested_mint, nested_mint_authority) =
        create_mint_mollusk(&mollusk, &mut accounts, &payer, program_id);
    let (owner_associated_token_address, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &owner_mint,
        program_id,
    );
    let (nested_associated_token_address, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &owner_associated_token_address,
        &nested_mint,
        program_id,
    );
    let (destination_token_address, _) = create_associated_token_account_mollusk(
        &mollusk,
        &mut accounts,
        &payer,
        &wallet.pubkey(),
        &nested_mint,
        program_id,
    );

    let recover_instruction =
        instruction::recover_nested(&wallet.pubkey(), &owner_mint, &nested_mint, program_id);
    let mut context = RecoverNestedTestContext {
        mollusk: &mollusk,
        accounts: &mut accounts,
        program_id,
        nested_mint,
        nested_mint_authority: &nested_mint_authority,
        nested_associated_token_address,
        destination_token_address,
        wallet: &wallet,
        recover_instruction,
        expected_error: None,
    };
    execute_recover_nested_test_scenario(&mut context);
}

#[test]
fn success_different_mints() {
    check_different_mints_mollusk(&spl_token_interface::id());
}

#[test]
fn success_different_mints_2022() {
    check_different_mints_mollusk(&spl_token_2022_interface::id());
}

// Error test cases using mollusk
#[test]
fn fail_missing_wallet_signature_2022() {
    let mut harness = TestHarness::new(&spl_token_2022_interface::id())
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_nested_ata(owner_ata);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    let mut recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    recover_instruction.accounts[5] =
        AccountMeta::new(harness.wallet.as_ref().unwrap().pubkey(), false);

    harness.execute_error(&recover_instruction, ProgramError::MissingRequiredSignature);
}

#[test]
fn fail_missing_wallet_signature() {
    let mut harness = TestHarness::new(&spl_token_interface::id())
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_nested_ata(owner_ata);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    let mut recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    recover_instruction.accounts[5] =
        AccountMeta::new(harness.wallet.as_ref().unwrap().pubkey(), false);

    harness.execute_error(&recover_instruction, ProgramError::MissingRequiredSignature);
}

#[test]
fn fail_wrong_signer_2022() {
    let mut harness = TestHarness::new(&spl_token_2022_interface::id())
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_nested_ata(owner_ata);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    // Test-specific logic: create wrong wallet and instruction with wrong signer
    let wrong_wallet = Keypair::new();
    ensure_system_accounts_with_lamports(
        &mut harness.accounts,
        &[(wrong_wallet.pubkey(), 1_000_000)],
    );

    ensure_recover_nested_accounts(
        &mut harness.accounts,
        &wrong_wallet.pubkey(),
        &mint,
        &mint,
        &spl_token_2022_interface::id(),
    );

    let recover_instruction = instruction::recover_nested(
        &wrong_wallet.pubkey(),
        &mint,
        &mint,
        &spl_token_2022_interface::id(),
    );

    harness.execute_error(&recover_instruction, ProgramError::IllegalOwner);
}

#[test]
fn fail_wrong_signer() {
    let mut harness = TestHarness::new(&spl_token_interface::id())
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_nested_ata(owner_ata);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    // Test-specific logic: create wrong wallet and instruction with wrong signer
    let wrong_wallet = Keypair::new();
    ensure_system_accounts_with_lamports(
        &mut harness.accounts,
        &[(wrong_wallet.pubkey(), 1_000_000)],
    );

    ensure_recover_nested_accounts(
        &mut harness.accounts,
        &wrong_wallet.pubkey(),
        &mint,
        &mint,
        &spl_token_interface::id(),
    );

    let recover_instruction = instruction::recover_nested(
        &wrong_wallet.pubkey(),
        &mint,
        &mint,
        &spl_token_interface::id(),
    );

    harness.execute_error(&recover_instruction, ProgramError::IllegalOwner);
}

#[test]
fn fail_not_nested_2022() {
    let mut harness = TestHarness::new(&spl_token_2022_interface::id())
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let wrong_wallet = Pubkey::new_unique();

    // Create nested ATA under wrong wallet instead of owner ATA
    let nested_ata = harness.create_ata_for_owner(wrong_wallet);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    let recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    harness.execute_error(&recover_instruction, ProgramError::IllegalOwner);
}

#[test]
fn fail_not_nested() {
    let mut harness = TestHarness::new(&spl_token_interface::id())
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let wrong_wallet = Pubkey::new_unique();

    // Create nested ATA under wrong wallet instead of owner ATA
    let nested_ata = harness.create_ata_for_owner(wrong_wallet);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    let recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    harness.execute_error(&recover_instruction, ProgramError::IllegalOwner);
}
#[test]
fn fail_wrong_address_derivation_owner_2022() {
    let mut harness = TestHarness::new(&spl_token_2022_interface::id())
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_nested_ata(owner_ata);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    let mut recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    let wrong_owner_address = Pubkey::new_unique();
    recover_instruction.accounts[3] = AccountMeta::new_readonly(wrong_owner_address, false);

    harness.accounts.push((
        wrong_owner_address,
        account_builder::AccountBuilder::system_account(0),
    ));

    harness.execute_error(&recover_instruction, ProgramError::InvalidSeeds);
}

#[test]
fn fail_wrong_address_derivation_owner() {
    let mut harness = TestHarness::new(&spl_token_interface::id())
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_nested_ata(owner_ata);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    let mut recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    let wrong_owner_address = Pubkey::new_unique();
    recover_instruction.accounts[3] = AccountMeta::new_readonly(wrong_owner_address, false);

    harness.accounts.push((
        wrong_owner_address,
        account_builder::AccountBuilder::system_account(0),
    ));

    harness.execute_error(&recover_instruction, ProgramError::InvalidSeeds);
}

#[test]
fn fail_owner_account_does_not_exist() {
    let mut harness = TestHarness::new(&spl_token_2022_interface::id())
        .with_wallet(1_000_000)
        .with_mint(0);
    // Note: deliberately NOT calling .with_ata() - owner ATA should not exist

    let mint = harness.mint.unwrap();
    let wallet_pubkey = harness.wallet.as_ref().unwrap().pubkey();
    let owner_ata_address = get_associated_token_address_with_program_id(
        &wallet_pubkey,
        &mint,
        &spl_token_2022_interface::id(),
    );

    // Test-specific logic: create nested ATA using non-existent owner ATA address
    harness.accounts.push((
        spl_associated_token_account::id(),
        account_builder::AccountBuilder::executable_program(spl_associated_token_account::id()),
    ));

    let (nested_ata, _) = create_associated_token_account_mollusk(
        &harness.mollusk,
        &mut harness.accounts,
        &harness.payer,
        &owner_ata_address,
        &mint,
        &spl_token_2022_interface::id(),
    );

    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    ensure_recover_nested_accounts(
        &mut harness.accounts,
        &wallet_pubkey,
        &mint,
        &mint,
        &spl_token_2022_interface::id(),
    );

    if !harness
        .accounts
        .iter()
        .any(|(pubkey, _)| *pubkey == harness.mint_authority.as_ref().unwrap().pubkey())
    {
        harness.accounts.push((
            harness.mint_authority.as_ref().unwrap().pubkey(),
            account_builder::AccountBuilder::system_account(0),
        ));
    }

    ensure_program_accounts_present(
        &mut harness.accounts,
        &[
            spl_token_2022_interface::id(),
            spl_associated_token_account::id(),
            solana_system_interface::program::id(),
        ],
    );

    let recover_instruction = instruction::recover_nested(
        &wallet_pubkey,
        &mint,
        &mint,
        &spl_token_2022_interface::id(),
    );

    harness.execute_error(&recover_instruction, ProgramError::IllegalOwner);
}

#[test]
fn fail_wrong_spl_token_program() {
    let mut harness = TestHarness::new(&spl_token_2022_interface::id())
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_nested_ata(owner_ata);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    // Test-specific logic: ensure accounts for wrong program and use wrong program in instruction
    ensure_recover_nested_accounts(
        &mut harness.accounts,
        &harness.wallet.as_ref().unwrap().pubkey(),
        &mint,
        &mint,
        &spl_token_interface::id(),
    );
    ensure_program_accounts_present(&mut harness.accounts, &[spl_token_interface::id()]);

    let recover_instruction = instruction::recover_nested(
        &harness.wallet.as_ref().unwrap().pubkey(),
        &mint,
        &mint,
        &spl_token_interface::id(), // Wrong program ID
    );

    harness.execute_error(&recover_instruction, ProgramError::IllegalOwner);
}

#[test]
fn fail_destination_not_wallet_ata() {
    let mut harness = TestHarness::new(&spl_token_2022_interface::id())
        .with_wallet(1_000_000)
        .with_mint(0)
        .with_ata();

    let mint = harness.mint.unwrap();
    let owner_ata = harness.ata_address.unwrap();
    let nested_ata = harness.create_nested_ata(owner_ata);
    harness.mint_tokens_to(nested_ata, TEST_MINT_AMOUNT);

    // Create wrong destination ATA
    let wrong_wallet = Pubkey::new_unique();
    let wrong_destination_ata = harness.create_ata_for_owner(wrong_wallet);

    let mut recover_instruction = harness.build_recover_nested_instruction(mint, mint);
    recover_instruction.accounts[2] = AccountMeta::new(wrong_destination_ata, false);

    harness.execute_error(&recover_instruction, ProgramError::InvalidSeeds);
}
