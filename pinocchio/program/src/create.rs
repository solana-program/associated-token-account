use pinocchio::{AccountView, Address, ProgramResult};

/// Specify when to create the associated token account.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CreateMode {
    /// Always try to create the associated token account.
    Always,
    /// Only try to create the associated token account if non-existent.
    Idempotent,
}

#[inline(always)]
pub(crate) fn process_create_associated_token_account(
    _program_id: &Address,
    _accounts: &[AccountView],
    _create_mode: CreateMode,
) -> ProgramResult {
    unimplemented!()
}
