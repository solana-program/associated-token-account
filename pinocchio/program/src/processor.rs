use {
    crate::{
        create::{process_create_associated_token_account, CreateMode},
        recover::process_recover_nested,
    },
    pinocchio::{error::ProgramError, AccountView, Address, ProgramResult},
    pinocchio_associated_token_account_interface::instruction::AssociatedTokenAccountInstruction,
};

#[inline(always)]
pub fn process_instruction(
    program_id: &Address,
    accounts: &mut [AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    match parse_instruction(instruction_data)? {
        AssociatedTokenAccountInstruction::Create => {
            process_create_associated_token_account(program_id, accounts, CreateMode::Always)
        }
        AssociatedTokenAccountInstruction::CreateIdempotent => {
            process_create_associated_token_account(program_id, accounts, CreateMode::Idempotent)
        }
        AssociatedTokenAccountInstruction::RecoverNested => {
            process_recover_nested(program_id, accounts)
        }
    }
}

/// Canonical ATA instruction format:
/// - `[]` or `[0]`: `Create`
/// - `[1]`: `CreateIdempotent`
/// - `[2]`: `RecoverNested`
/// - any other payload is invalid
#[inline(always)]
fn parse_instruction(
    instruction_data: &[u8],
) -> Result<AssociatedTokenAccountInstruction, ProgramError> {
    match instruction_data {
        [] => Ok(AssociatedTokenAccountInstruction::Create),
        [discriminator] => AssociatedTokenAccountInstruction::try_from(*discriminator)
            .map_err(|_| ProgramError::InvalidInstructionData),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_instruction, AssociatedTokenAccountInstruction};

    #[test]
    fn parse_instruction_matches_ata_wire_format() {
        assert_eq!(
            parse_instruction(&[]).unwrap(),
            AssociatedTokenAccountInstruction::Create
        );
        assert_eq!(
            parse_instruction(&[0]).unwrap(),
            AssociatedTokenAccountInstruction::Create
        );
        assert_eq!(
            parse_instruction(&[1]).unwrap(),
            AssociatedTokenAccountInstruction::CreateIdempotent
        );
        assert_eq!(
            parse_instruction(&[2]).unwrap(),
            AssociatedTokenAccountInstruction::RecoverNested
        );
    }

    #[test]
    fn parse_instruction_rejects_non_canonical_payloads() {
        assert!(parse_instruction(&[3]).is_err());
        assert!(parse_instruction(&[0, 0]).is_err());
        assert!(parse_instruction(&[1, 9, 9]).is_err());
    }
}
