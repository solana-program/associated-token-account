use {
    crate::processor::{process_create, process_recover},
    pinocchio::{
        account_info::AccountInfo,
        msg, no_allocator, nostd_panic_handler, program_entrypoint,
        program_error::{ProgramError, ToStr},
        pubkey::Pubkey,
        ProgramResult,
    },
    spl_token_interface::error::TokenError,
};

program_entrypoint!(entry);
no_allocator!();
nostd_panic_handler!();

#[cold]
fn log_error(err: &ProgramError) {
    // re-use the interface's TokenError for stringification
    pinocchio::log::sol_log(err.to_str::<TokenError>());
}

#[inline(always)]
pub fn entry(program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    // empty or [0,..] = Create
    // [1,..]      = CreateIdempotent
    // [2,..]      = RecoverNested
    let instr = if data.is_empty() { 0 } else { data[0] };

    let res = match instr {
        0 => process_create(program_id, accounts, false),
        1 => process_create(program_id, accounts, true),
        2 => process_recover(program_id, accounts),
        _ => return Err(TokenError::InvalidInstruction.into()),
    };

    res.inspect_err(log_error)
}
