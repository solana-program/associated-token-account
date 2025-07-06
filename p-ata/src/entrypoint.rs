#![allow(unexpected_cfgs)]

use {
    crate::processor::{process_create, process_recover},
    pinocchio::{
        account_info::AccountInfo, no_allocator, nostd_panic_handler, program_entrypoint,
        pubkey::Pubkey, ProgramResult,
    },
    spl_token_interface::error::TokenError,
};

program_entrypoint!(entry);
no_allocator!();
nostd_panic_handler!();

#[inline(always)]
pub fn entry(program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    match data {
        // Empty data defaults to Create (discriminator 0) - preserving backward compatibility
        [] => process_create(program_id, accounts, false, None, None),
        [discriminator, instruction_data @ ..] => match *discriminator {
            // 0 - Create (with optional bump and/or account_len)
            0 => match instruction_data {
                // No additional data - compute bump and account_len on-chain (original behavior)
                [] => process_create(program_id, accounts, false, None, None),
                // Only bump provided
                [bump] => process_create(program_id, accounts, false, Some(*bump), None),
                // Bump + account_len provided (for Token-2022 optimization)
                [bump, account_len_bytes @ ..] => {
                    let account_len =
                        u16::from_le_bytes([account_len_bytes[0], account_len_bytes[1]]) as usize;
                    process_create(program_id, accounts, false, Some(*bump), Some(account_len))
                }
            },
            // 1 - CreateIdempotent
            1 => process_create(program_id, accounts, true, None, None),
            // 2 - RecoverNested (with optional bump)
            2 => match instruction_data {
                // No additional data - compute bump on-chain (original behavior)
                [] => process_recover(program_id, accounts, None),
                // Only bump provided
                [bump] => process_recover(program_id, accounts, Some(*bump)),
                _ => Err(TokenError::InvalidInstruction.into()),
            },
            _ => Err(TokenError::InvalidInstruction.into()),
        },
    }
}
