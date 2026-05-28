use {
    mollusk_svm_result::Check,
    solana_address::Address,
    solana_instruction::Instruction,
    solana_program_error::ProgramError,
    solana_program_pack::Pack,
    solana_rent::Rent,
    solana_system_interface::{instruction as system_instruction, program as system_program},
    spl_associated_token_account_interface::instruction::recover_nested,
    spl_associated_token_account_mollusk_harness::{
        build_recover_nested_instruction, AtaProgram, AtaTestHarness,
    },
    spl_token_2022_interface::{extension::StateWithExtensionsOwned, state::Account},
    spl_token_interface::state::Multisig,
    test_case::test_case,
};

const TEST_MINT_AMOUNT: u64 = 100;

struct RecoverNestedSetup {
    harness: AtaTestHarness,
    wallet: Address,
    owner_mint: Address,
    nested_mint: Address,
    nested_ata: Address,
    destination_ata: Address,
}

fn create_multisig_wallet(
    harness: &mut AtaTestHarness,
    token_program_id: Address,
    signers: &[Address],
    required_signers: u8,
) -> Address {
    let signer_accounts = signers
        .iter()
        .copied()
        .map(|signer| (signer, 1_000_000))
        .collect::<Vec<_>>();
    harness.ensure_accounts_with_lamports(&signer_accounts);

    let multisig = Address::new_unique();
    let rent_lamports = Rent::default().minimum_balance(Multisig::LEN);
    let create_multisig_ix = system_instruction::create_account(
        &harness.payer,
        &multisig,
        rent_lamports,
        Multisig::LEN as u64,
        &token_program_id,
    );
    harness
        .ctx
        .process_and_validate_instruction(&create_multisig_ix, &[Check::success()]);

    let signer_refs = signers.iter().collect::<Vec<_>>();
    let initialize_multisig_ix = if token_program_id == spl_token_interface::id() {
        spl_token_interface::instruction::initialize_multisig2(
            &token_program_id,
            &multisig,
            &signer_refs,
            required_signers,
        )
    } else {
        spl_token_2022_interface::instruction::initialize_multisig2(
            &token_program_id,
            &multisig,
            &signer_refs,
            required_signers,
        )
    }
    .expect("initialize multisig instruction");
    harness
        .ctx
        .process_and_validate_instruction(&initialize_multisig_ix, &[Check::success()]);

    multisig
}

fn create_sized_wallet(
    harness: &mut AtaTestHarness,
    owner: Address,
    space: usize,
    lamports: u64,
) -> Address {
    let wallet = Address::new_unique();
    let rent_lamports = Rent::default().minimum_balance(space);
    let create_wallet_ix = system_instruction::create_account(
        &harness.payer,
        &wallet,
        rent_lamports.max(lamports),
        space as u64,
        &owner,
    );
    harness
        .ctx
        .process_and_validate_instruction(&create_wallet_ix, &[Check::success()]);
    wallet
}

fn create_uninitialized_multisig_wallet(
    harness: &mut AtaTestHarness,
    token_program_id: Address,
) -> Address {
    create_sized_wallet(harness, token_program_id, Multisig::LEN, 0)
}

fn recover_nested_setup_for_wallet(
    mut harness: AtaTestHarness,
    wallet: Address,
    owner_token_program_id: Address,
    nested_token_program_id: Address,
) -> RecoverNestedSetup {
    let (owner_mint, _) = harness.create_mint_with_token_program(owner_token_program_id, 0);
    let owner_ata = harness.create_ata_for_owner_with_token_program(
        wallet,
        1_000_000,
        owner_mint,
        owner_token_program_id,
    );

    let (nested_mint, nested_mint_authority) =
        harness.create_mint_with_token_program(nested_token_program_id, 0);
    let nested_ata = harness.create_ata_for_owner_with_token_program(
        owner_ata,
        1_000_000,
        nested_mint,
        nested_token_program_id,
    );
    harness.mint_tokens_to_with_token_program(
        nested_mint,
        nested_mint_authority,
        nested_ata,
        nested_token_program_id,
        TEST_MINT_AMOUNT,
    );

    let destination_ata = harness.create_ata_for_owner_with_token_program(
        wallet,
        1_000_000,
        nested_mint,
        nested_token_program_id,
    );

    RecoverNestedSetup {
        harness,
        wallet,
        owner_mint,
        nested_mint,
        nested_ata,
        destination_ata,
    }
}

fn recover_nested_multisig_setup_with_required_signers(
    wallet_token_program_id: Address,
    owner_token_program_id: Address,
    nested_token_program_id: Address,
    required_signers: u8,
) -> (RecoverNestedSetup, [Address; 3]) {
    let mut harness =
        AtaTestHarness::new_with_ata_program(&owner_token_program_id, AtaProgram::Pinocchio);
    let signers = [
        Address::new_unique(),
        Address::new_unique(),
        Address::new_unique(),
    ];
    let wallet = create_multisig_wallet(
        &mut harness,
        wallet_token_program_id,
        &signers,
        required_signers,
    );

    (
        recover_nested_setup_for_wallet(
            harness,
            wallet,
            owner_token_program_id,
            nested_token_program_id,
        ),
        signers,
    )
}

fn recover_nested_multisig_setup(
    wallet_token_program_id: Address,
    owner_token_program_id: Address,
    nested_token_program_id: Address,
) -> (RecoverNestedSetup, [Address; 3]) {
    recover_nested_multisig_setup_with_required_signers(
        wallet_token_program_id,
        owner_token_program_id,
        nested_token_program_id,
        2,
    )
}

fn assert_recover_nested_success(setup: RecoverNestedSetup, recover_instruction: Instruction) {
    let pre_wallet_lamports = {
        let store = setup.harness.ctx.account_store.borrow();
        store.get(&setup.wallet).unwrap().lamports
    };
    let nested_lamports = setup.harness.get_account(setup.nested_ata).lamports;

    setup.harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[
            Check::success(),
            Check::account(&setup.wallet)
                .lamports(pre_wallet_lamports.checked_add(nested_lamports).unwrap())
                .build(),
            Check::account(&setup.nested_ata).lamports(0).build(),
            Check::account(&setup.nested_ata).closed().build(),
        ],
    );

    let account = setup.harness.get_account(setup.destination_ata);
    assert_eq!(
        StateWithExtensionsOwned::<Account>::unpack(account.data)
            .unwrap()
            .base
            .amount,
        TEST_MINT_AMOUNT
    );
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn success_multisig_wallet(wallet_token_program_id: Address) {
    let owner_token_program_id = spl_token_interface::id();
    let nested_token_program_id = spl_token_interface::id();
    let (setup, signers) = recover_nested_multisig_setup(
        wallet_token_program_id,
        owner_token_program_id,
        nested_token_program_id,
    );

    let recover_instruction = build_recover_nested_instruction(
        setup.wallet,
        setup.owner_mint,
        setup.nested_mint,
        owner_token_program_id,
        nested_token_program_id,
        &[&signers[0], &signers[2]],
    );

    assert_recover_nested_success(setup, recover_instruction);
}

#[test]
fn success_multisig_wallet_with_explicit_nested_token_program() {
    let owner_token_program_id = spl_token_interface::id();
    let nested_token_program_id = spl_token_2022_interface::id();
    let (setup, signers) = recover_nested_multisig_setup(
        spl_token_interface::id(),
        owner_token_program_id,
        nested_token_program_id,
    );

    let recover_instruction = build_recover_nested_instruction(
        setup.wallet,
        setup.owner_mint,
        setup.nested_mint,
        owner_token_program_id,
        nested_token_program_id,
        &[&signers[0], &signers[1]],
    );

    assert_recover_nested_success(setup, recover_instruction);
}

#[test]
fn success_m1_multisig_wallet_with_extra_signers() {
    let owner_token_program_id = spl_token_interface::id();
    let nested_token_program_id = spl_token_interface::id();
    let (setup, signers) = recover_nested_multisig_setup_with_required_signers(
        spl_token_interface::id(),
        owner_token_program_id,
        nested_token_program_id,
        1,
    );

    let recover_instruction = build_recover_nested_instruction(
        setup.wallet,
        setup.owner_mint,
        setup.nested_mint,
        owner_token_program_id,
        nested_token_program_id,
        &[&signers[0], &signers[2]],
    );

    assert_recover_nested_success(setup, recover_instruction);
}

#[test]
fn fail_multisig_wallet_with_insufficient_signers() {
    let owner_token_program_id = spl_token_interface::id();
    let nested_token_program_id = spl_token_interface::id();
    let (setup, signers) = recover_nested_multisig_setup(
        spl_token_interface::id(),
        owner_token_program_id,
        nested_token_program_id,
    );

    let recover_instruction = build_recover_nested_instruction(
        setup.wallet,
        setup.owner_mint,
        setup.nested_mint,
        owner_token_program_id,
        nested_token_program_id,
        &[&signers[0]],
    );

    setup.harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::MissingRequiredSignature)],
    );
}

#[test]
fn fail_multisig_wallet_signed_without_configured_signers() {
    let owner_token_program_id = spl_token_interface::id();
    let nested_token_program_id = spl_token_interface::id();
    let (setup, _) = recover_nested_multisig_setup(
        spl_token_interface::id(),
        owner_token_program_id,
        nested_token_program_id,
    );

    let recover_instruction = recover_nested(
        &setup.wallet,
        &setup.owner_mint,
        &setup.nested_mint,
        &owner_token_program_id,
    );

    setup.harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::MissingRequiredSignature)],
    );
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn fail_uninitialized_multisig_wallet(wallet_token_program_id: Address) {
    let owner_token_program_id = spl_token_interface::id();
    let nested_token_program_id = spl_token_interface::id();
    let mut harness =
        AtaTestHarness::new_with_ata_program(&owner_token_program_id, AtaProgram::Pinocchio);
    let wallet = create_uninitialized_multisig_wallet(&mut harness, wallet_token_program_id);
    let setup = recover_nested_setup_for_wallet(
        harness,
        wallet,
        owner_token_program_id,
        nested_token_program_id,
    );

    let recover_instruction = build_recover_nested_instruction(
        setup.wallet,
        setup.owner_mint,
        setup.nested_mint,
        owner_token_program_id,
        nested_token_program_id,
        &[],
    );

    setup.harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::InvalidAccountData)],
    );
}

#[test]
fn fail_multisig_sized_wallet_with_unrecognized_owner() {
    let owner_token_program_id = spl_token_interface::id();
    let nested_token_program_id = spl_token_interface::id();
    let mut harness =
        AtaTestHarness::new_with_ata_program(&owner_token_program_id, AtaProgram::Pinocchio);
    let wallet = create_sized_wallet(&mut harness, system_program::id(), Multisig::LEN, 0);
    let setup = recover_nested_setup_for_wallet(
        harness,
        wallet,
        owner_token_program_id,
        nested_token_program_id,
    );

    // Manually flip the wallet's is_signer flag to false to exercise the
    // authorization fall-through: wallet has multisig-LEN data so the multisig
    // path is considered, but neither token program owns it, so it falls
    // through to the `is_signer` check, which fails.
    let mut recover_instruction = build_recover_nested_instruction(
        setup.wallet,
        setup.owner_mint,
        setup.nested_mint,
        owner_token_program_id,
        nested_token_program_id,
        &[],
    );
    recover_instruction.accounts[5].is_signer = false;

    setup.harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::MissingRequiredSignature)],
    );
}

#[test]
fn fail_multisig_wallet_with_wrong_signer() {
    let owner_token_program_id = spl_token_interface::id();
    let nested_token_program_id = spl_token_interface::id();
    let (setup, signers) = recover_nested_multisig_setup(
        spl_token_interface::id(),
        owner_token_program_id,
        nested_token_program_id,
    );
    let wrong_signer = Address::new_unique();
    setup
        .harness
        .ensure_account_exists_with_lamports(wrong_signer, 1_000_000);

    let recover_instruction = build_recover_nested_instruction(
        setup.wallet,
        setup.owner_mint,
        setup.nested_mint,
        owner_token_program_id,
        nested_token_program_id,
        &[&signers[0], &wrong_signer],
    );

    setup.harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::MissingRequiredSignature)],
    );
}

#[test]
fn fail_multisig_wallet_with_duplicate_signer() {
    let owner_token_program_id = spl_token_interface::id();
    let nested_token_program_id = spl_token_interface::id();
    let (setup, signers) = recover_nested_multisig_setup(
        spl_token_interface::id(),
        owner_token_program_id,
        nested_token_program_id,
    );

    let recover_instruction = build_recover_nested_instruction(
        setup.wallet,
        setup.owner_mint,
        setup.nested_mint,
        owner_token_program_id,
        nested_token_program_id,
        &[&signers[0], &signers[0]],
    );

    setup.harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::MissingRequiredSignature)],
    );
}
