use {
    mollusk_svm_result::Check,
    solana_address::Address,
    solana_instruction::AccountMeta,
    solana_program_error::ProgramError,
    spl_associated_token_account_mollusk_harness::{
        AtaProgram, AtaTestHarness, CreateAtaInstructionType,
    },
    test_case::test_matrix,
};

fn instruction_type(idempotent: bool, rent_sysvar_via_account: bool) -> CreateAtaInstructionType {
    if idempotent {
        CreateAtaInstructionType::CreateIdempotent {
            bump: None,
            rent_sysvar_via_account,
        }
    } else {
        CreateAtaInstructionType::Create {
            bump: None,
            account_len: None,
            rent_sysvar_via_account,
        }
    }
}

#[test_matrix(
    [spl_token_interface::id(), spl_token_2022_interface::id()],
    [true, false]
)]
fn create_rejects_wrong_optional_rent_account(token_program_id: Address, idempotent: bool) {
    let mut harness =
        AtaTestHarness::new_with_ata_program(&token_program_id, AtaProgram::Pinocchio)
            .with_wallet_and_mint(1_000_000, 6);
    let incorrect_rent_sysvar = Address::new_unique();
    harness.ensure_account_exists_with_lamports(incorrect_rent_sysvar, 1_000_000);

    let mut instruction = harness.build_create_ata_instruction(instruction_type(idempotent, true));
    instruction.accounts[6] = AccountMeta::new_readonly(incorrect_rent_sysvar, false);

    harness.ctx.process_and_validate_instruction(
        &instruction,
        &[Check::err(ProgramError::InvalidArgument)],
    );
}
