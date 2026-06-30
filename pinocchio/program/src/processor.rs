use {
    crate::{create::process_create_associated_token_account, recover::process_recover_nested},
    pinocchio::{AccountView, Address, ProgramResult, error::ProgramError},
    pinocchio_associated_token_account_interface::instruction::{
        AssociatedTokenAccountInstruction, CreateMode,
    },
};

#[inline(always)]
pub fn process_instruction(
    program_id: &Address,
    accounts: &mut [AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    let instruction = AssociatedTokenAccountInstruction::try_from_bytes(instruction_data)
        .map_err(instruction_error)?;

    match instruction {
        AssociatedTokenAccountInstruction::Create => process_create_associated_token_account(
            program_id,
            accounts,
            CreateMode::Always,
            false,
            None,
            None,
        ),
        AssociatedTokenAccountInstruction::CreateIdempotent => {
            process_create_associated_token_account(
                program_id,
                accounts,
                CreateMode::Idempotent,
                false,
                None,
                None,
            )
        }
        AssociatedTokenAccountInstruction::CreateWithArgs {
            mode,
            bump,
            account_len,
        } => process_create_associated_token_account(
            program_id,
            accounts,
            mode,
            true,
            bump.get().map(Into::into),
            account_len.get().map(Into::into),
        ),
        AssociatedTokenAccountInstruction::RecoverNested => {
            process_recover_nested(program_id, accounts)
        }
    }
}

#[cold]
fn instruction_error(error: ProgramError) -> ProgramError {
    error
}
