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
    match parse_instruction(instruction_data)? {
        AssociatedTokenAccountInstruction::Create => {
            process_create_associated_token_account(program_id, accounts, CreateMode::Always, None)
        }
        AssociatedTokenAccountInstruction::CreateIdempotent => {
            process_create_associated_token_account(
                program_id,
                accounts,
                CreateMode::Idempotent,
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
            Some((bump, account_len)),
        ),
        AssociatedTokenAccountInstruction::RecoverNested => {
            process_recover_nested(program_id, accounts)
        }
    }
}

/// Canonical ATA instruction format:
/// - `[]` or `[0]`: `Create`
/// - `[1]`: `CreateIdempotent`
/// - `[3, mode, bump, account_len]`: `CreateWithArgs`
/// - `[2]`: `RecoverNested`
/// - any other payload is invalid
#[inline(always)]
fn parse_instruction(
    instruction_data: &[u8],
) -> Result<AssociatedTokenAccountInstruction, ProgramError> {
    match instruction_data {
        [] | [0] => Ok(AssociatedTokenAccountInstruction::Create),
        [1] => Ok(AssociatedTokenAccountInstruction::CreateIdempotent),
        [2] => Ok(AssociatedTokenAccountInstruction::RecoverNested),
        [3, mode, bump, account_len @ ..] if account_len.len() == 8 => {
            Ok(AssociatedTokenAccountInstruction::CreateWithArgs {
                mode: CreateMode::try_from(*mode)
                    .map_err(|_| ProgramError::InvalidInstructionData)?,
                bump: *bump,
                account_len: u64::from_le_bytes(account_len.try_into().unwrap()),
            })
        }
        _ => Err(ProgramError::InvalidInstructionData),
    }
}

#[cfg(test)]
mod tests {
    use {
        super::parse_instruction,
        pinocchio_associated_token_account_interface::instruction::{
            AssociatedTokenAccountInstruction, CreateMode,
        },
    };

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
        assert_eq!(
            parse_instruction(&[3, 0, 254, 165, 0, 0, 0, 0, 0, 0, 0]).unwrap(),
            AssociatedTokenAccountInstruction::CreateWithArgs {
                mode: CreateMode::Always,
                bump: 254,
                account_len: 165,
            }
        );
        assert_eq!(
            parse_instruction(&[3, 1, 253, 170, 0, 0, 0, 0, 0, 0, 0]).unwrap(),
            AssociatedTokenAccountInstruction::CreateWithArgs {
                mode: CreateMode::Idempotent,
                bump: 253,
                account_len: 170,
            }
        );
    }

    #[test]
    fn parse_instruction_rejects_non_canonical_payloads() {
        assert!(parse_instruction(&[4]).is_err());
        assert!(parse_instruction(&[0, 0]).is_err());
        assert!(parse_instruction(&[1, 9, 9]).is_err());
        assert!(parse_instruction(&[3]).is_err());
        assert!(parse_instruction(&[3, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0]).is_err());
        assert!(parse_instruction(&[3, 1]).is_err());
        assert!(parse_instruction(&[2, 0]).is_err());
    }
}
