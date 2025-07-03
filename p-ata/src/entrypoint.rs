#![allow(unexpected_cfgs)]

use {
    crate::processor::{process_create, process_create_with_bump, process_recover},
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
    let [discriminator, _instruction_data @ ..] = data else {
        // Empty data defaults to Create (discriminator 0)
        return process_create(program_id, accounts, false);
    };

    match *discriminator {
        // 0 - Create
        0 => process_create(program_id, accounts, false),
        // 1 - CreateIdempotent
        1 => process_create(program_id, accounts, true),
        // 2 - RecoverNested
        2 => process_recover(program_id, accounts),
        // 3 - CreateWithBump (optimized: client provides bump)
        3 => {
            if _instruction_data.is_empty() {
                return Err(TokenError::InvalidInstruction.into());
            }
            let bump = _instruction_data[0];
            process_create_with_bump(program_id, accounts, bump, false)
        }
        _ => Err(TokenError::InvalidInstruction.into()),
    }
}
