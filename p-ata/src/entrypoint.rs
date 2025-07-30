#![allow(unexpected_cfgs)]

use {
    crate::processor::process_create_associated_token_account,
    crate::recover::process_recover_nested,
    pinocchio::{
        account_info::AccountInfo, no_allocator, nostd_panic_handler, program_entrypoint,
        program_error::ProgramError, pubkey::Pubkey, ProgramResult,
    },
};

/// Maximum allowed account `known_account_data_length` to prevent creation
/// of accounts that are expensive to work with. This mitigates potential
/// attacks where callers could create very large, expensive-to-work-with
/// accounts for others.
///
/// Any token program requiring a larger length must rely on CPI calls to
/// determine the account length and cannot pass in `known_account_data_length`.
pub const MAX_SANE_ACCOUNT_LENGTH: u16 = 1024;

program_entrypoint!(process_instruction);
no_allocator!();
nostd_panic_handler!();

/// Main instruction processor for the p-ATA program.
///
/// ## Instruction Format
///
/// ### Create ATA (Non-Idempotent) - Discriminator: 0 or Empty
/// ```
/// [0] or []                     -> compute bump and ATA account data length on-chain  
/// [0, bump]                     -> use provided bump, compute ATA account data length
/// [0, bump, len_low, len_high]  -> use provided bump and ATA account data length
/// ```
///
/// ### Create ATA (Idempotent) - Discriminator: 1  
/// ```
/// [1]                           -> compute bump and ATA account data length on-chain
/// [1, bump]                     -> use provided bump, compute ATA account data length  
/// [1, bump, len_low, len_high]  -> use provided bump and ATA account data length
/// ```
///
/// ### Recover Nested ATA - Discriminator: 2
/// ```
/// [2]                                      -> compute all bumps on-chain
/// [2, owner_bump, nested_bump, dest_bump]  -> use provided bumps
/// ```
///
/// ## Account Layout (Create)
/// ```
/// [0] payer                    (signer, writable) - pays for account creation (and rent if applicable)
/// [1] associated_token_account (writable)         - account to create  
/// [2] wallet                   (signer)           - token account owner
/// [3] mint                                        - token mint account
/// [4] system_program                              - system program
/// [5] token_program                               - token program
/// [6] rent_sysvar              (optional)         - rent sysvar (for optimization)
/// ```
///
/// ## Security
///
/// - All bump hints are validated for canonicality
/// - `token_account_len` is bounded by `MAX_SANE_ACCOUNT_LENGTH`
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
