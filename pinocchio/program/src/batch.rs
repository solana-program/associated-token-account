use {
    core::{mem::MaybeUninit, slice::from_raw_parts},
    pinocchio::{
        AccountView, Address, ProgramResult,
        cpi::{CpiAccount, invoke_unchecked},
        instruction::{InstructionAccount, InstructionView},
    },
    pinocchio_token::instructions::{
        Batch, InitializeAccount3, InitializeImmutableOwner, IntoBatch,
    },
};

const DATA_LEN: usize =
    Batch::header_data_len(2) + InitializeImmutableOwner::DATA_LEN + InitializeAccount3::DATA_LEN;
const ACCOUNTS_LEN: usize =
    InitializeImmutableOwner::ACCOUNTS_LEN + InitializeAccount3::ACCOUNTS_LEN;

// TODO: `pinocchio-token` v0.6 provides a Batch builder but its `invoke()` method hardcodes
//       SPL Token's program ID. `pinocchio-token-2022` does not yet offer its own batch builder.
//       Once it does, this can be replaced.
#[inline(always)]
pub(crate) fn batch_init_and_lock_owner(
    token_program: &Address,
    account: &AccountView,
    mint: &AccountView,
    owner: &Address,
) -> ProgramResult {
    // Buffers sized for exactly two sub-instructions
    let mut data = [const { MaybeUninit::<u8>::uninit() }; DATA_LEN];
    let mut instruction_accounts =
        [const { MaybeUninit::<InstructionAccount>::uninit() }; ACCOUNTS_LEN];
    let mut accounts = [const { MaybeUninit::<CpiAccount>::uninit() }; ACCOUNTS_LEN];

    // Serialize both sub-instructions into the buffers
    let mut batch = Batch::new(&mut data, &mut instruction_accounts, &mut accounts)?;
    InitializeImmutableOwner::new(account).into_batch(&mut batch)?;
    InitializeAccount3::new(account, mint, owner).into_batch(&mut batch)?;

    // Mirrors `Batch::invoke_signed()` but with a supplied program id:
    // https://github.com/anza-xyz/pinocchio/blob/pinocchio-token%40v0.6.0/programs/token/src/instructions/batch.rs#L92-L105
    unsafe {
        invoke_unchecked(
            &InstructionView {
                program_id: token_program,
                accounts: from_raw_parts(instruction_accounts.as_ptr() as _, ACCOUNTS_LEN),
                data: from_raw_parts(data.as_ptr() as _, DATA_LEN),
            },
            from_raw_parts(accounts.as_ptr() as _, ACCOUNTS_LEN),
        );
    }

    Ok(())
}
