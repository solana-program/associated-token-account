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
            bump,
            account_len,
        ),
        AssociatedTokenAccountInstruction::RecoverNested => {
            process_recover_nested(program_id, accounts)
        }
    }
}

/// Canonical ATA instruction format:
/// - `[]` or `[0]`: `Create`
/// - `[1]`: `CreateIdempotent`
/// - `[2]`: `RecoverNested`
/// - `[3, mode, bump_tag, bump?, account_len_tag, account_len?]`: `CreateWithArgs`
/// - any other payload is invalid
#[inline(always)]
fn parse_instruction(
    instruction_data: &[u8],
) -> Result<AssociatedTokenAccountInstruction, ProgramError> {
    match instruction_data {
        [] | [0] => Ok(AssociatedTokenAccountInstruction::Create),
        [1] => Ok(AssociatedTokenAccountInstruction::CreateIdempotent),
        [2] => Ok(AssociatedTokenAccountInstruction::RecoverNested),
        [3, mode, create_args @ ..] => {
            let (bump, account_len) = parse_create_args(create_args)?;
            Ok(AssociatedTokenAccountInstruction::CreateWithArgs {
                mode: CreateMode::try_from(*mode)
                    .map_err(|_| ProgramError::InvalidInstructionData)?,
                bump,
                account_len,
            })
        }
        _ => Err(ProgramError::InvalidInstructionData),
    }
}

#[inline(always)]
fn parse_create_args(create_args: &[u8]) -> Result<(Option<u8>, Option<u64>), ProgramError> {
    let (bump, rest) = match create_args {
        [0, rest @ ..] => (None, rest),
        [1, bump, rest @ ..] => (Some(*bump), rest),
        _ => return Err(ProgramError::InvalidInstructionData),
    };

    let account_len = match rest {
        [0] => None,
        [1, bytes @ ..] => Some(u64::from_le_bytes(
            bytes
                .try_into()
                .map_err(|_| ProgramError::InvalidInstructionData)?,
        )),
        _ => return Err(ProgramError::InvalidInstructionData),
    };

    Ok((bump, account_len))
}

#[cfg(test)]
mod tests {
    use {
        super::{ProgramError, parse_instruction},
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
            parse_instruction(&[3, 0, 0, 0]).unwrap(),
            AssociatedTokenAccountInstruction::CreateWithArgs {
                mode: CreateMode::Always,
                bump: None,
                account_len: None,
            }
        );
        assert_eq!(
            parse_instruction(&[3, 1, 1, 253, 0]).unwrap(),
            AssociatedTokenAccountInstruction::CreateWithArgs {
                mode: CreateMode::Idempotent,
                bump: Some(253),
                account_len: None,
            }
        );
        assert_eq!(
            parse_instruction(&[3, 0, 0, 1, 1, 2, 3, 4, 5, 6, 7, 8]).unwrap(),
            AssociatedTokenAccountInstruction::CreateWithArgs {
                mode: CreateMode::Always,
                bump: None,
                account_len: Some(u64::from_le_bytes([1, 2, 3, 4, 5, 6, 7, 8])),
            }
        );
        assert_eq!(
            parse_instruction(&[3, 1, 1, 253, 1, 1, 2, 3, 4, 5, 6, 7, 8]).unwrap(),
            AssociatedTokenAccountInstruction::CreateWithArgs {
                mode: CreateMode::Idempotent,
                bump: Some(253),
                account_len: Some(u64::from_le_bytes([1, 2, 3, 4, 5, 6, 7, 8])),
            }
        );
    }

    #[test]
    fn parse_instruction_rejects_non_canonical_payloads() {
        let cases: &[&[u8]] = &[
            &[4],                                       // unknown discriminator
            &[0, 0],                                    // trailing byte after Create
            &[1, 9, 9],                                 // trailing bytes after CreateIdempotent
            &[3],                                       // missing CreateWithArgs mode
            &[3, 2, 0, 0],                              // invalid CreateWithArgs mode
            &[3, 1],                                    // missing bump tag
            &[3, 1, 0],                                 // missing account_len tag
            &[3, 1, 2, 0],                              // invalid bump tag
            &[3, 1, 1],                                 // bump tag present without bump
            &[3, 1, 1, 253],                            // missing account_len tag after bump
            &[3, 1, 0, 2],                              // invalid account_len tag
            &[3, 1, 0, 1],                              // account_len tag without account_len
            &[3, 1, 0, 1, 165, 0, 0, 0, 0, 0, 0],       // truncated account_len
            &[3, 1, 0, 1, 165, 0, 0, 0, 0, 0, 0, 0, 0], // trailing byte after account_len
            &[3, 1, 0, 0, 0],                           // trailing byte after CreateWithArgs
            &[2, 0],                                    // trailing byte after RecoverNested
        ];
        for data in cases {
            assert_eq!(
                parse_instruction(data),
                Err(ProgramError::InvalidInstructionData)
            );
        }
    }
}
