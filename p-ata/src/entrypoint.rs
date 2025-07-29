#![allow(unexpected_cfgs)]

use {
    crate::processor::process_create_associated_token_account,
    crate::recover::process_recover_nested,
    pinocchio::{
        account_info::AccountInfo, no_allocator, nostd_panic_handler, program_entrypoint,
        program_error::ProgramError, pubkey::Pubkey, ProgramResult,
    },
};

/// An arbitrary maximum limit to prevent accounts which are expensive
/// to work with (i.e. u16::MAX) from being created for others.
pub const MAX_SANE_ACCOUNT_LENGTH: u16 = 2048;

program_entrypoint!(process_instruction);
no_allocator!();
nostd_panic_handler!();

#[inline(always)]
pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    match data {
        // Empty data defaults to Create (discriminator 0) - preserving backward compatibility
        [] => process_create_associated_token_account(program_id, accounts, false, None, None),
        [discriminator, instruction_data @ ..] => {
            let idempotent = match *discriminator {
                0 => false,
                1 => true,
                2 => {
                    return match instruction_data {
                        [] => process_recover_nested(program_id, accounts, None),
                        [owner_bump, nested_bump, destination_bump] => process_recover_nested(
                            program_id,
                            accounts,
                            Some((*owner_bump, *nested_bump, *destination_bump)),
                        ),
                        _ => Err(pinocchio::program_error::ProgramError::InvalidInstructionData),
                    }
                }
                _ => return Err(pinocchio::program_error::ProgramError::InvalidInstructionData),
            };

            match instruction_data {
                // No additional data - compute bump and account_len on-chain (original behavior)
                [] => process_create_associated_token_account(
                    program_id, accounts, idempotent, None, None,
                ),
                // Only bump provided
                [bump] => process_create_associated_token_account(
                    program_id,
                    accounts,
                    idempotent,
                    Some(*bump),
                    None,
                ),
                // Bump + account_len provided (for Token-2022 optimization)
                [bump, account_len_bytes @ ..] => {
                    // SAFETY: runtime-bounded, and account_len is last.
                    let account_len = unsafe {
                        u16::from_le_bytes(*(account_len_bytes.as_ptr() as *const [u8; 2]))
                    };

                    if account_len > MAX_SANE_ACCOUNT_LENGTH {
                        return Err(ProgramError::InvalidInstructionData);
                    }

                    process_create_associated_token_account(
                        program_id,
                        accounts,
                        idempotent,
                        Some(*bump),
                        Some(account_len as usize),
                    )
                }
            }
        }
    }
}
