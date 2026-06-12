use {
    mollusk_svm_result::Check,
    solana_address::Address,
    solana_instruction::{AccountMeta, Instruction},
    solana_program_error::ProgramError,
    spl_associated_token_account_mollusk_harness::{
        AtaProgram, AtaTestHarness, build_recover_nested_instruction,
    },
    spl_token_2022_interface::{extension::StateWithExtensionsOwned, state::Account},
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

// Build a nested ATA layout where the owner and nested accounts can be under
// different token programs
fn recover_nested_setup(
    owner_token_program_id: Address,
    nested_token_program_id: Address,
) -> RecoverNestedSetup {
    let mut harness =
        AtaTestHarness::new_with_ata_program(&owner_token_program_id, AtaProgram::Pinocchio)
            .with_wallet(1_000_000);

    let wallet = harness.wallet.unwrap();
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
        setup.wallet,
        setup.owner_mint,
        setup.nested_mint,
        owner_token_program_id,
        nested_token_program_id,
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
        setup.wallet,
        setup.owner_mint,
        setup.nested_mint,
        owner_token_program_id,
        nested_token_program_id,
        &[],
    );

    // Point the nested token program account at the owner program to break PDA derivation
    recover_instruction.accounts[7].pubkey = owner_token_program_id;

    setup.harness.ctx.process_and_validate_instruction(
        &recover_instruction,
        &[Check::err(ProgramError::InvalidSeeds)],
    );
}

#[test]
fn fail_trailing_account_that_is_not_a_token_program() {
    let owner_token_program_id = spl_token_interface::id();
    let nested_token_program_id = spl_token_interface::id();
    let setup = recover_nested_setup(owner_token_program_id, nested_token_program_id);

    let mut recover_instruction = build_recover_nested_instruction(
        setup.wallet,
        setup.owner_mint,
        setup.nested_mint,
        owner_token_program_id,
        nested_token_program_id,
        &[],
    );

    // Position 7 is always parsed as the nested token program when present
    // (multisig signer accounts only come after it), so an arbitrary account
    // there breaks the nested ATA derivation.
    recover_instruction
        .accounts
        .push(AccountMeta::new_readonly(Address::new_unique(), false));

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
        setup.wallet,
        setup.owner_mint,
        setup.nested_mint,
        owner_token_program_id,
        nested_token_program_id,
        &[],
    );

    assert_recover_nested_success(setup, recover_instruction);
}
