use pinocchio::{AccountView, Address, ProgramResult};

#[inline(always)]
pub(crate) fn process_recover_nested(
    _program_id: &Address,
    _accounts: &mut [AccountView],
) -> ProgramResult {
    unimplemented!()
}
