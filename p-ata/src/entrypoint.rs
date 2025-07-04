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
        [] => process_create(program_id, accounts, false, None),
        [discriminator, instruction_data @ ..] => match *discriminator {
            // 0 - Create (with optional bump)
            0 => match instruction_data {
                // No bump provided - compute bump on-chain (original behavior)
                [] => process_create(program_id, accounts, false, None),
                // Bump provided - use for optimization
                [bump] => process_create(program_id, accounts, false, Some(*bump)),
                _ => Err(TokenError::InvalidInstruction.into()),
            },
            // 1 - CreateIdempotent
            1 => process_create(program_id, accounts, true, None),
            // 2 - RecoverNested (with optional bump)
            2 => match instruction_data {
                // No bump provided - compute bump on-chain (original behavior)
                [] => process_recover(program_id, accounts, None),
                // Bump provided - use for optimization
                [bump] => process_recover(program_id, accounts, Some(*bump)),
                _ => Err(TokenError::InvalidInstruction.into()),
            },
            _ => Err(TokenError::InvalidInstruction.into()),
        },
    }
}
