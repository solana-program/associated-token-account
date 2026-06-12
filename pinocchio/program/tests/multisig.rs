//! RecoverNested with a token multisig wallet.
//!
//! A multisig wallet cannot sign, so it is authorized by its configured
//! signer accounts trailing the optional nested token program account.
//! Tests are ordered by where the code path exits in `process_recover_nested`.

use {
    mollusk_svm_result::Check,
    solana_account::Account as SolanaAccount,
    solana_address::Address,
    solana_instruction::Instruction,
    solana_program_error::ProgramError,
    solana_program_pack::Pack,
    solana_rent::Rent,
    solana_system_interface::{instruction as system_instruction, program as system_program},
    spl_associated_token_account_mollusk_harness::{
        AtaProgram, AtaTestHarness, build_recover_nested_instruction,
    },
    spl_token_2022_interface::{extension::StateWithExtensionsOwned, state::Account},
    spl_token_interface::{instruction::MAX_SIGNERS, state::Multisig},
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

/// Inserts a multisig-sized wallet with raw `m`/`n`/`is_initialized` bytes
/// that the token programs could never produce, to exercise the bounds guard.
fn create_crafted_multisig_wallet(
    harness: &AtaTestHarness,
    token_program_id: Address,
    m: u8,
    n: u8,
    is_initialized: u8,
) -> Address {
    let wallet = Address::new_unique();
    let mut data = vec![0u8; Multisig::LEN];
    data[0] = m;
    data[1] = n;
    data[2] = is_initialized;
    harness.ctx.account_store.borrow_mut().insert(
        wallet,
        SolanaAccount {
            lamports: Rent::default().minimum_balance(Multisig::LEN),
            data,
            owner: token_program_id,
            executable: false,
            rent_epoch: 0,
        },
    );
    wallet
}

fn create_sized_wallet(harness: &mut AtaTestHarness, owner: Address, space: usize) -> Address {
    let wallet = Address::new_unique();
    let create_wallet_ix = system_instruction::create_account(
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

/// Builds the nested ATA layout (owner ATA holding a nested ATA with tokens,
/// plus the recovery destination ATA) for an arbitrary wallet address.
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

/// Default setup: a 2-of-3 multisig wallet with everything under spl-token
/// unless specified otherwise.
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

/// Builds a RecoverNested instruction for setups where every token program is
/// spl-token.
fn build_recover_instruction(setup: &RecoverNestedSetup, signers: &[&Address]) -> Instruction {
    build_recover_nested_instruction(
        setup.wallet,
        setup.owner_mint,
        setup.nested_mint,
        spl_token_interface::id(),
        spl_token_interface::id(),
        signers,
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

// Exit: nested ATA derivation (`InvalidSeeds`), before wallet authorization.
// Multisig signer accounts come after the nested token program account, so
// omitting it while passing signers makes the first signer parse as the
// nested token program and break the derivation.
#[test]
fn fail_omitted_nested_token_program_with_signers() {
    let (setup, signers) = multisig_setup(
        spl_token_interface::id(),
        spl_token_interface::id(),
        spl_token_interface::id(),
    );

    let mut recover_instruction = build_recover_instruction(&setup, &[&signers[0], &signers[1]]);
    recover_instruction.accounts.remove(7);

    setup.harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::InvalidSeeds)],
    );
}

// Exit: multisig shape gate. A plain wallet that did not sign cannot be a
// multisig, so authorization fails like it does on the mainnet program.
#[test]
fn fail_wallet_did_not_sign() {
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

// Exit: multisig shape gate (token-program-owned but not multisig-sized,
// e.g. a token account used as the wallet).
#[test]
fn fail_non_signing_wallet_with_token_account_len() {
    let owner_token_program_id = spl_token_interface::id();
    let mut harness =
        AtaTestHarness::new_with_ata_program(&owner_token_program_id, AtaProgram::Pinocchio);
    let wallet = create_sized_wallet(&mut harness, owner_token_program_id, Account::LEN);
    let setup = recover_nested_setup_for_wallet(
        harness,
        wallet,
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

// Exit: multisig shape gate (multisig-sized but not owned by a token program).
#[test]
fn fail_non_signing_wallet_with_unrecognized_owner() {
    let owner_token_program_id = spl_token_interface::id();
    let mut harness =
        AtaTestHarness::new_with_ata_program(&owner_token_program_id, AtaProgram::Pinocchio);
    let wallet = create_sized_wallet(&mut harness, system_program::id(), Multisig::LEN);
    let setup = recover_nested_setup_for_wallet(
        harness,
        wallet,
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

// Exit: multisig initialization check.
#[test_case(spl_token_interface::id())]
#[test_case(spl_token_2022_interface::id())]
fn fail_uninitialized_multisig_wallet(wallet_token_program_id: Address) {
    let owner_token_program_id = spl_token_interface::id();
    let mut harness =
        AtaTestHarness::new_with_ata_program(&owner_token_program_id, AtaProgram::Pinocchio);
    let wallet = create_sized_wallet(&mut harness, wallet_token_program_id, Multisig::LEN);
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
        &[Check::err(ProgramError::UninitializedAccount)],
    );
}

// Exit: multisig bounds guard (initialized flag must be exactly 1). With
// `m = 0` this account would otherwise authorize with zero signers.
#[test]
fn fail_crafted_multisig_initialized_byte() {
    let owner_token_program_id = spl_token_interface::id();
    let harness =
        AtaTestHarness::new_with_ata_program(&owner_token_program_id, AtaProgram::Pinocchio);
    let wallet = create_crafted_multisig_wallet(&harness, owner_token_program_id, 0, 0, 2);
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
        &[Check::err(ProgramError::InvalidAccountData)],
    );
}

// Exit: multisig bounds guard (`n` beyond the maximum signer slots).
#[test]
fn fail_crafted_multisig_signer_count() {
    let owner_token_program_id = spl_token_interface::id();
    let harness =
        AtaTestHarness::new_with_ata_program(&owner_token_program_id, AtaProgram::Pinocchio);
    let wallet = create_crafted_multisig_wallet(
        &harness,
        owner_token_program_id,
        1,
        MAX_SIGNERS as u8 + 1,
        1,
    );
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
        &[Check::err(ProgramError::InvalidAccountData)],
    );
}

// Exit: a signer account matching a multisig slot did not sign. This is an
// error even when the threshold was already met, matching SPL Token's
// `validate_owner`.
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
    recover_instruction.accounts[9].is_signer = false;

    setup.harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::MissingRequiredSignature)],
    );
}

// Exit: signer threshold (nested token program present, no signer accounts).
#[test]
fn fail_no_signer_accounts() {
    let (setup, signers) = multisig_setup(
        spl_token_interface::id(),
        spl_token_interface::id(),
        spl_token_interface::id(),
    );

    // Build with one signer to get the multisig account layout, then drop it,
    // leaving the nested token program as the last account.
    let mut recover_instruction = build_recover_instruction(&setup, &[&signers[0]]);
    recover_instruction.accounts.pop();

    setup.harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::MissingRequiredSignature)],
    );
}

// Exit: signer threshold (no trailing accounts at all; the nested token
// program falls back to the owner token program and the signer slice is
// empty).
#[test]
fn fail_no_trailing_accounts() {
    let (setup, _) = multisig_setup(
        spl_token_interface::id(),
        spl_token_interface::id(),
        spl_token_interface::id(),
    );

    let mut recover_instruction = build_recover_instruction(&setup, &[]);
    recover_instruction.accounts[5].is_signer = false;

    setup.harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::MissingRequiredSignature)],
    );
}

// Exit: signer threshold (one valid signer of the required two).
#[test]
fn fail_insufficient_signers() {
    let (setup, signers) = multisig_setup(
        spl_token_interface::id(),
        spl_token_interface::id(),
        spl_token_interface::id(),
    );

    let recover_instruction = build_recover_instruction(&setup, &[&signers[0]]);

    setup.harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::MissingRequiredSignature)],
    );
}

// Exit: signer threshold (an account not in the multisig signer list does not
// count, even when it signed).
#[test]
fn fail_unknown_signer_account() {
    let (setup, signers) = multisig_setup(
        spl_token_interface::id(),
        spl_token_interface::id(),
        spl_token_interface::id(),
    );
    let unknown_signer = Address::new_unique();
    setup
        .harness
        .ensure_account_exists_with_lamports(unknown_signer, 1_000_000);

    let recover_instruction = build_recover_instruction(&setup, &[&signers[0], &unknown_signer]);

    setup.harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::MissingRequiredSignature)],
    );
}

// Exit: signer threshold (the same signer account passed twice satisfies one
// unique multisig slot only once).
#[test]
fn fail_duplicate_signer_accounts() {
    let (setup, signers) = multisig_setup(
        spl_token_interface::id(),
        spl_token_interface::id(),
        spl_token_interface::id(),
    );

    let recover_instruction = build_recover_instruction(&setup, &[&signers[0], &signers[0]]);

    setup.harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::MissingRequiredSignature)],
    );
}

// Success: a multisig-shaped wallet that signs takes the plain wallet path
// without multisig validation, matching the mainnet program.
#[test]
fn success_signed_multisig_wallet() {
    let (setup, _) = multisig_setup(
        spl_token_interface::id(),
        spl_token_interface::id(),
        spl_token_interface::id(),
    );

    // The builder marks the wallet as a signer when no multisig signer
    // accounts are passed.
    let recover_instruction = build_recover_instruction(&setup, &[]);

    assert_recover_nested_success(setup, recover_instruction);
}

// Success: same for an uninitialized multisig-shaped wallet that signs.
#[test]
fn success_signed_uninitialized_multisig_wallet() {
    let owner_token_program_id = spl_token_interface::id();
    let mut harness =
        AtaTestHarness::new_with_ata_program(&owner_token_program_id, AtaProgram::Pinocchio);
    let wallet = create_sized_wallet(&mut harness, owner_token_program_id, Multisig::LEN);
    let setup = recover_nested_setup_for_wallet(
        harness,
        wallet,
        owner_token_program_id,
        owner_token_program_id,
    );

    let recover_instruction = build_recover_instruction(&setup, &[]);

    assert_recover_nested_success(setup, recover_instruction);
}

// Success: m-of-n authorization across multisig owner programs and ATA token
// programs, including a multisig owned by the other token program than the
// ATAs and mixed owner/nested token programs.
#[test_case(
    spl_token_interface::id(),
    spl_token_interface::id(),
    spl_token_interface::id()
)]
#[test_case(
    spl_token_2022_interface::id(),
    spl_token_interface::id(),
    spl_token_interface::id()
)]
#[test_case(
    spl_token_interface::id(),
    spl_token_2022_interface::id(),
    spl_token_2022_interface::id()
)]
#[test_case(
    spl_token_2022_interface::id(),
    spl_token_2022_interface::id(),
    spl_token_2022_interface::id()
)]
#[test_case(
    spl_token_interface::id(),
    spl_token_interface::id(),
    spl_token_2022_interface::id()
)]
#[test_case(
    spl_token_2022_interface::id(),
    spl_token_2022_interface::id(),
    spl_token_interface::id()
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
        setup.wallet,
        setup.owner_mint,
        setup.nested_mint,
        owner_token_program_id,
        nested_token_program_id,
        &[&signers[0], &signers[2]],
    );

    assert_recover_nested_success(setup, recover_instruction);
}

// Success: accounts not in the multisig signer list are skipped, and valid
// signers beyond the required threshold are harmless.
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

    let recover_instruction =
        build_recover_instruction(&setup, &[&unknown_signer, &signers[0], &signers[2]]);

    assert_recover_nested_success(setup, recover_instruction);
}

// Success: one signer account satisfies every multisig slot holding its
// address, matching SPL Token's `validate_owner`.
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

// Success: the maximum 11-of-11 multisig.
#[test]
fn success_max_multisig_signers() {
    let signers: Vec<Address> = (0..MAX_SIGNERS).map(|_| Address::new_unique()).collect();
    let setup = multisig_setup_with_signers(
        spl_token_interface::id(),
        spl_token_interface::id(),
        spl_token_interface::id(),
        &signers,
        MAX_SIGNERS as u8,
    );

    let signer_refs: Vec<&Address> = signers.iter().collect();
    let recover_instruction = build_recover_instruction(&setup, &signer_refs);

    assert_recover_nested_success(setup, recover_instruction);
}
