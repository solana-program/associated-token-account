use {
    mollusk_svm_result::Check,
    solana_address::Address,
    solana_instruction::Instruction,
    solana_program_error::ProgramError,
    solana_program_pack::Pack,
    solana_rent::Rent,
    spl_associated_token_account_mollusk_harness::{
        AtaProgram, AtaTestHarness, build_recover_nested_instruction,
    },
    spl_token_2022_interface::{
        extension::StateWithExtensionsOwned, instruction::initialize_multisig2, state::Account,
    },
    spl_token_interface::state::Multisig,
    test_case::{test_case, test_matrix},
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

// Build a nested ATA layout where the owner and nested accounts can be under
// different token programs
fn recover_nested_setup(
    owner_token_program_id: Address,
    nested_token_program_id: Address,
) -> RecoverNestedSetup {
    let harness =
        AtaTestHarness::new_with_ata_program(&owner_token_program_id, AtaProgram::Pinocchio)
            .with_wallet(1_000_000);

    let wallet = harness.wallet.unwrap();

    recover_nested_setup_for_wallet(
        harness,
        wallet,
        owner_token_program_id,
        nested_token_program_id,
    )
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

#[test]
fn fail_missing_extra_account_when_programs_differ() {
    let owner_token_program_id = spl_token_interface::id();
    let nested_token_program_id = spl_token_2022_interface::id();
    let setup = recover_nested_setup(owner_token_program_id, nested_token_program_id);

    let mut recover_instruction = build_recover_nested_instruction(
        &setup.wallet,
        &setup.owner_mint,
        &setup.nested_mint,
        &owner_token_program_id,
        &nested_token_program_id,
        &[],
    );

    // Drop the optional nested token program account
    recover_instruction.accounts.truncate(7);

    setup.harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::InvalidSeeds)],
    );
}

#[test]
fn fail_wrong_nested_token_program_account() {
    let owner_token_program_id = spl_token_interface::id();
    let nested_token_program_id = spl_token_2022_interface::id();
    let setup = recover_nested_setup(owner_token_program_id, nested_token_program_id);

    let mut recover_instruction = build_recover_nested_instruction(
        &setup.wallet,
        &setup.owner_mint,
        &setup.nested_mint,
        &owner_token_program_id,
        &nested_token_program_id,
        &[],
    );

    // Point the nested token program account at the owner program to break PDA derivation
    recover_instruction.accounts[7].pubkey = owner_token_program_id;

    setup.harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::InvalidSeeds)],
    );
}

#[test_case(spl_token_interface::id(), spl_token_interface::id())]
#[test_case(spl_token_interface::id(), spl_token_2022_interface::id())]
#[test_case(spl_token_2022_interface::id(), spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id(), spl_token_2022_interface::id())]
fn success_mixed_token_programs(owner_token_program_id: Address, nested_token_program_id: Address) {
    let setup = recover_nested_setup(owner_token_program_id, nested_token_program_id);

    let recover_instruction = build_recover_nested_instruction(
        &setup.wallet,
        &setup.owner_mint,
        &setup.nested_mint,
        &owner_token_program_id,
        &nested_token_program_id,
        &[],
    );
    assert_eq!(
        recover_instruction.accounts.len(),
        if owner_token_program_id == nested_token_program_id {
            7
        } else {
            8
        }
    );

    assert_recover_nested_success(setup, recover_instruction);
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn success_same_token_program_with_redundant_nested_token_program_account(
    token_program_id: Address,
) {
    let setup = recover_nested_setup(token_program_id, token_program_id);

    let mut recover_instruction = build_recover_nested_instruction(
        &setup.wallet,
        &setup.owner_mint,
        &setup.nested_mint,
        &token_program_id,
        &token_program_id,
        &[],
    );
    assert_eq!(recover_instruction.accounts.len(), 7);
    recover_instruction
        .accounts
        .push(solana_instruction::AccountMeta::new_readonly(
            token_program_id,
            false,
        ));

    assert_recover_nested_success(setup, recover_instruction);
}

#[test]
fn fail_standard_wallet_did_not_sign() {
    let owner_token_program_id = spl_token_interface::id();
    let harness =
        AtaTestHarness::new_with_ata_program(&owner_token_program_id, AtaProgram::Pinocchio);
    let setup = recover_nested_setup_for_wallet(
        harness,
        Address::new_unique(),
        owner_token_program_id,
        owner_token_program_id,
    );

    let mut recover_instruction = build_recover_instruction(&setup, &[]);
    recover_instruction.accounts[5].is_signer = false;

    setup.harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::MissingRequiredSignature)],
    );
}

#[test]
fn fail_token_owned_non_multisig_must_sign() {
    let owner_token_program_id = spl_token_interface::id();
    let mut harness =
        AtaTestHarness::new_with_ata_program(&owner_token_program_id, AtaProgram::Pinocchio);
    // Owned by the token program but Account::LEN (not Multisig::LEN)
    let wallet = create_sized_wallet(&mut harness, owner_token_program_id, Account::LEN);
    let setup = recover_nested_setup_for_wallet(
        harness,
        wallet,
        owner_token_program_id,
        owner_token_program_id,
    );

    let signer = Address::new_unique();
    setup
        .harness
        .ensure_account_exists_with_lamports(signer, 1_000_000);
    let recover_instruction = build_recover_instruction(&setup, &[&signer]);

    setup.harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::MissingRequiredSignature)],
    );
}

#[test]
fn fail_multisig_len_wallet_with_non_token_owner_must_sign() {
    let owner_token_program_id = spl_token_interface::id();
    let mut harness =
        AtaTestHarness::new_with_ata_program(&owner_token_program_id, AtaProgram::Pinocchio);
    let wallet = create_sized_wallet(&mut harness, Address::new_unique(), Multisig::LEN);
    let setup = recover_nested_setup_for_wallet(
        harness,
        wallet,
        owner_token_program_id,
        owner_token_program_id,
    );

    let signer = Address::new_unique();
    setup
        .harness
        .ensure_account_exists_with_lamports(signer, 1_000_000);
    let recover_instruction = build_recover_instruction(&setup, &[&signer]);

    setup.harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::MissingRequiredSignature)],
    );
}

// =============== MULTISIG TESTS ===============

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
    let create_multisig_ix = solana_system_interface::instruction::create_account(
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
    let initialize_multisig_ix =
        initialize_multisig2(&token_program_id, &multisig, &signer_refs, required_signers).unwrap();
    harness
        .ctx
        .process_and_validate_instruction(&initialize_multisig_ix, &[Check::success()]);

    multisig
}

fn create_sized_wallet(harness: &mut AtaTestHarness, owner: Address, space: usize) -> Address {
    let wallet = Address::new_unique();
    let create_wallet_ix = solana_system_interface::instruction::create_account(
        &harness.payer,
        &wallet,
        Rent::default().minimum_balance(space),
        space as u64,
        &owner,
    );
    harness
        .ctx
        .process_and_validate_instruction(&create_wallet_ix, &[Check::success()]);
    wallet
}

fn multisig_setup_with_signers(
    wallet_token_program_id: Address,
    owner_token_program_id: Address,
    nested_token_program_id: Address,
    signers: &[Address],
    required_signers: u8,
) -> RecoverNestedSetup {
    let mut harness =
        AtaTestHarness::new_with_ata_program(&owner_token_program_id, AtaProgram::Pinocchio);
    let wallet = create_multisig_wallet(
        &mut harness,
        wallet_token_program_id,
        signers,
        required_signers,
    );
    recover_nested_setup_for_wallet(
        harness,
        wallet,
        owner_token_program_id,
        nested_token_program_id,
    )
}

fn multisig_setup(
    wallet_token_program_id: Address,
    owner_token_program_id: Address,
    nested_token_program_id: Address,
) -> (RecoverNestedSetup, [Address; 3]) {
    let signers = [
        Address::new_unique(),
        Address::new_unique(),
        Address::new_unique(),
    ];
    let setup = multisig_setup_with_signers(
        wallet_token_program_id,
        owner_token_program_id,
        nested_token_program_id,
        &signers,
        2,
    );
    (setup, signers)
}

fn build_recover_instruction(setup: &RecoverNestedSetup, signers: &[&Address]) -> Instruction {
    build_recover_nested_instruction(
        &setup.wallet,
        &setup.owner_mint,
        &setup.nested_mint,
        &spl_token_interface::id(),
        &spl_token_interface::id(),
        signers,
    )
}

#[test]
fn fail_missing_nested_token_program_account_with_signers() {
    let (setup, signers) = multisig_setup(
        spl_token_interface::id(),
        spl_token_interface::id(),
        spl_token_interface::id(),
    );

    let mut recover_instruction = build_recover_instruction(&setup, &[&signers[0], &signers[1]]);
    // Drop the nested token program account so the first signer lands in its slot
    recover_instruction.accounts.remove(7);

    setup.harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::InvalidSeeds)],
    );
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn fail_uninitialized_multisig_wallet(owner_token_program_id: Address) {
    let mut harness =
        AtaTestHarness::new_with_ata_program(&owner_token_program_id, AtaProgram::Pinocchio);
    let wallet = create_sized_wallet(&mut harness, owner_token_program_id, Multisig::LEN);
    let setup = recover_nested_setup_for_wallet(
        harness,
        wallet,
        owner_token_program_id,
        owner_token_program_id,
    );

    let recover_instruction = build_recover_nested_instruction(
        &setup.wallet,
        &setup.owner_mint,
        &setup.nested_mint,
        &owner_token_program_id,
        &owner_token_program_id,
        &[],
    );

    setup.harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::UninitializedAccount)],
    );
}

#[test]
fn fail_matched_signer_account_did_not_sign() {
    let signers = [
        Address::new_unique(),
        Address::new_unique(),
        Address::new_unique(),
    ];
    let setup = multisig_setup_with_signers(
        spl_token_interface::id(),
        spl_token_interface::id(),
        spl_token_interface::id(),
        &signers,
        1,
    );

    let mut recover_instruction = build_recover_instruction(&setup, &[&signers[0], &signers[1]]);
    recover_instruction
        .accounts
        .iter_mut()
        .find(|account| account.pubkey == signers[1])
        .unwrap()
        .is_signer = false;

    setup.harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::MissingRequiredSignature)],
    );
}

#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn fail_signed_multisig_wallet_without_signer_accounts(owner_token_program_id: Address) {
    let (setup, _) = multisig_setup(
        owner_token_program_id,
        owner_token_program_id,
        owner_token_program_id,
    );

    let recover_instruction = build_recover_nested_instruction(
        &setup.wallet,
        &setup.owner_mint,
        &setup.nested_mint,
        &owner_token_program_id,
        &owner_token_program_id,
        &[],
    );

    setup.harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::MissingRequiredSignature)],
    );
}

enum InvalidMultisigSignerSet {
    InsufficientSigners,
    UnknownSigner,
    DuplicateSignerAccount,
}

#[test_case(InvalidMultisigSignerSet::InsufficientSigners)]
#[test_case(InvalidMultisigSignerSet::UnknownSigner)]
#[test_case(InvalidMultisigSignerSet::DuplicateSignerAccount)]
fn fail_invalid_multisig_signer_sets(case: InvalidMultisigSignerSet) {
    let (setup, signers) = multisig_setup(
        spl_token_interface::id(),
        spl_token_interface::id(),
        spl_token_interface::id(),
    );
    let unknown_signer = Address::new_unique();
    setup
        .harness
        .ensure_account_exists_with_lamports(unknown_signer, 1_000_000);

    let recover_instruction = match case {
        InvalidMultisigSignerSet::InsufficientSigners => {
            build_recover_instruction(&setup, &[&signers[0]])
        }
        InvalidMultisigSignerSet::UnknownSigner => {
            build_recover_instruction(&setup, &[&signers[0], &unknown_signer])
        }
        InvalidMultisigSignerSet::DuplicateSignerAccount => {
            build_recover_instruction(&setup, &[&signers[0], &signers[0]])
        }
    };

    setup.harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::MissingRequiredSignature)],
    );
}

#[test_matrix(
    [spl_token_interface::id(), spl_token_2022_interface::id()],
    [spl_token_interface::id(), spl_token_2022_interface::id()],
    [spl_token_interface::id(), spl_token_2022_interface::id()]
)]
fn success_multisig_wallet(
    wallet_token_program_id: Address,
    owner_token_program_id: Address,
    nested_token_program_id: Address,
) {
    let (setup, signers) = multisig_setup(
        wallet_token_program_id,
        owner_token_program_id,
        nested_token_program_id,
    );

    let recover_instruction = build_recover_nested_instruction(
        &setup.wallet,
        &setup.owner_mint,
        &setup.nested_mint,
        &owner_token_program_id,
        &nested_token_program_id,
        &[&signers[2], &signers[0]],
    );

    assert_recover_nested_success(setup, recover_instruction);
}

#[test]
fn success_duplicate_multisig_slots_satisfied_by_one_account() {
    let duplicated = Address::new_unique();
    let signers = [duplicated, duplicated, Address::new_unique()];
    let setup = multisig_setup_with_signers(
        spl_token_interface::id(),
        spl_token_interface::id(),
        spl_token_interface::id(),
        &signers,
        2,
    );

    let recover_instruction = build_recover_instruction(&setup, &[&duplicated]);

    assert_recover_nested_success(setup, recover_instruction);
}

#[test]
fn success_extra_signer_accounts_ignored() {
    let signers = [
        Address::new_unique(),
        Address::new_unique(),
        Address::new_unique(),
    ];
    let setup = multisig_setup_with_signers(
        spl_token_interface::id(),
        spl_token_interface::id(),
        spl_token_interface::id(),
        &signers,
        1,
    );
    let unknown_signer = Address::new_unique();
    setup
        .harness
        .ensure_account_exists_with_lamports(unknown_signer, 1_000_000);

    let mut recover_instruction =
        build_recover_instruction(&setup, &[&unknown_signer, &signers[0], &signers[2]]);
    recover_instruction
        .accounts
        .iter_mut()
        .find(|account| account.pubkey == unknown_signer)
        .unwrap()
        .is_signer = false;

    assert_recover_nested_success(setup, recover_instruction);
}
