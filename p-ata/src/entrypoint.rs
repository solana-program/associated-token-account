#![allow(unexpected_cfgs)]

use {
    crate::{
        processor::{
            check_idempotent_account, parse_create_accounts,
            process_create_associated_token_account,
        },
        recover::process_recover_nested,
    },
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
/// ```ignore
/// [0] or []                     -> compute bump and ATA account data length on-chain  
/// [0, bump]                     -> use provided bump, compute ATA account data length
/// [0, bump, len_low, len_high]  -> use provided bump and ATA account data length
/// ```
///
/// ### Create ATA (Idempotent) - Discriminator: 1  
/// ```ignore
/// [1]                           -> compute bump and ATA account data length on-chain
/// [1, bump]                     -> use provided bump, compute ATA account data length  
/// [1, bump, len_low, len_high]  -> use provided bump and ATA account data length
/// ```
///
/// ### Recover Nested ATA - Discriminator: 2
/// ```ignore
/// [2]                                      -> computes all bumps on-chain
/// ```
///
/// ## Account Layout (Create)
/// ```ignore
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
        [] => {
            let create_accounts = parse_create_accounts(accounts)?;
            process_create_associated_token_account(program_id, &create_accounts, None, None)
        }
        [discriminator, instruction_data @ ..] => {
            let idempotent = match *discriminator {
                0 => false,
                1 => true,
                2 => {
                    return match instruction_data {
                        [] => process_recover_nested(program_id, accounts),
                        _ => Err(pinocchio::program_error::ProgramError::InvalidInstructionData),
                    }
                }
                _ => return Err(pinocchio::program_error::ProgramError::InvalidInstructionData),
            };

            let create_accounts = parse_create_accounts(accounts)?;
            let (expected_bump, known_account_len): (Option<u8>, Option<usize>) =
                match instruction_data {
                    // No additional data - compute bump and account_len on-chain (old SPL ATA behavior)
                    [] => (None, None),
                    // Only bump provided
                    [bump] => (Some(*bump), None),
                    // Bump + account_len provided
                    [expected_bump, account_len_bytes @ ..] => {
                        let account_len = u16::from_le_bytes(
                            account_len_bytes
                                .try_into()
                                .map_err(|_| ProgramError::InvalidInstructionData)?,
                        );

                        if account_len > MAX_SANE_ACCOUNT_LENGTH {
                            return Err(ProgramError::InvalidInstructionData);
                        }

                        (Some(*expected_bump), Some(account_len as usize))
                    }
                };

            // SAFETY: no mutable borrows of any accounts have occurred.
            if idempotent
                && unsafe {
                    check_idempotent_account(
                        create_accounts.associated_token_account_to_create,
                        create_accounts.wallet,
                        create_accounts.mint,
                        create_accounts.token_program,
                        program_id,
                        expected_bump,
                    )?
                }
            {
                return Ok(());
            }

            process_create_associated_token_account(
                program_id,
                &create_accounts,
                expected_bump,
                known_account_len,
            )
        }
    }
}
