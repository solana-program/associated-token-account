use {
    core::{mem::MaybeUninit, slice::from_raw_parts},
    pinocchio::{
        AccountView, Address, ProgramResult,
        cpi::{CpiAccount, invoke_unchecked},
        instruction::{InstructionAccount, InstructionView},
    },
    pinocchio_token::instructions::{
        Batch, InitializeAccount, InitializeAccount3, InitializeImmutableOwner, IntoBatch,
    },
};

struct BatchLens {
    data: usize,
    accounts: usize,
}

const INIT_WITH_ACCOUNT: BatchLens = BatchLens {
    data: Batch::header_data_len(2)
        + InitializeImmutableOwner::DATA_LEN
        + InitializeAccount::DATA_LEN,
    accounts: InitializeImmutableOwner::ACCOUNTS_LEN + InitializeAccount::ACCOUNTS_LEN,
};
const INIT_WITH_ACCOUNT3: BatchLens = BatchLens {
    data: Batch::header_data_len(2)
        + InitializeImmutableOwner::DATA_LEN
        + InitializeAccount3::DATA_LEN,
    accounts: InitializeImmutableOwner::ACCOUNTS_LEN + InitializeAccount3::ACCOUNTS_LEN,
};
// TODO: `pinocchio-token` v0.6 provides a Batch builder but its `invoke()` method hardcodes
//       SPL Token's program ID. `pinocchio-token-2022` does not yet offer its own batch builder.
//       Once it does, this can be replaced.
#[inline(always)]
pub(crate) fn batch_init_and_lock_owner(
    token_program: &Address,
    account: &AccountView,
    mint: &AccountView,
    owner: &AccountView,
    rent_sysvar: Option<&AccountView>,
) -> ProgramResult {
    // `InitializeAccount3` has the larger data payload, `InitializeAccount` has
    // the larger account list because it includes the rent sysvar.
    let mut data = [const { MaybeUninit::<u8>::uninit() }; INIT_WITH_ACCOUNT3.data];
    let mut instruction_accounts =
        [const { MaybeUninit::<InstructionAccount>::uninit() }; INIT_WITH_ACCOUNT.accounts];
    let mut cpi_accounts =
        [const { MaybeUninit::<CpiAccount>::uninit() }; INIT_WITH_ACCOUNT.accounts];

    // Serialize both sub-instructions into the buffers
    let mut batch = Batch::new(&mut data, &mut instruction_accounts, &mut cpi_accounts)?;
    InitializeImmutableOwner::new(account).into_batch(&mut batch)?;
    let lens = match rent_sysvar {
        Some(rent_sysvar) => {
            InitializeAccount::new(account, mint, owner, rent_sysvar).into_batch(&mut batch)?;
            INIT_WITH_ACCOUNT
        }
        None => {
            InitializeAccount3::new(account, mint, owner.address()).into_batch(&mut batch)?;
            INIT_WITH_ACCOUNT3
        }
    };

    // Mirrors `Batch::invoke_signed()` but with a supplied program id:
    // https://github.com/anza-xyz/pinocchio/blob/pinocchio-token%40v0.6.0/programs/token/src/instructions/batch.rs#L92-L105
    unsafe {
        invoke_unchecked(
            &InstructionView {
                program_id: token_program,
                accounts: from_raw_parts(instruction_accounts.as_ptr() as _, lens.accounts),
                data: from_raw_parts(data.as_ptr() as _, lens.data),
            },
            from_raw_parts(cpi_accounts.as_ptr().cast(), lens.accounts),
        );
    }

    Ok(())
}
