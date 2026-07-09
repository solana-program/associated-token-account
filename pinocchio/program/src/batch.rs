use {
    core::mem::MaybeUninit,
    pinocchio::{
        AccountView, Address, ProgramResult,
        cpi::{CpiAccount, Signer},
        instruction::InstructionAccount,
    },
    pinocchio_token_2022::instructions::{
        Batch, CloseAccount, InitializeAccount, InitializeAccount3, InitializeImmutableOwner,
        IntoBatch, TransferChecked,
    },
};

#[inline(always)]
pub(crate) fn batch_init_and_lock_owner(
    token_program: &Address,
    account: &AccountView,
    mint: &AccountView,
    owner: &AccountView,
    rent_sysvar: Option<&AccountView>,
) -> ProgramResult {
    /// `InitializeAccount` requires more accounts than `InitializeAccount3`,
    /// so the maximum is based on the former.
    const MAX_ACCOUNTS_LEN: usize =
        InitializeImmutableOwner::ACCOUNTS_LEN + InitializeAccount::ACCOUNTS_LEN;

    /// `InitializeAccount3` requires more instruction data than `InitializeAccount`,
    /// so the maximum is based on the former.
    const MAX_DATA_LEN: usize = Batch::header_data_len(2)
        + InitializeImmutableOwner::DATA_LEN
        + InitializeAccount3::DATA_LEN;

    let mut data = [const { MaybeUninit::<u8>::uninit() }; MAX_DATA_LEN];
    let mut instruction_accounts =
        [const { MaybeUninit::<InstructionAccount>::uninit() }; MAX_ACCOUNTS_LEN];
    let mut cpi_accounts = [const { MaybeUninit::<CpiAccount>::uninit() }; MAX_ACCOUNTS_LEN];

    // Serialize both sub-instructions into the buffers
    let mut batch = Batch::new(&mut data, &mut instruction_accounts, &mut cpi_accounts)?;

    InitializeImmutableOwner::new(account).into_batch(&mut batch)?;

    match rent_sysvar {
        Some(rent_sysvar) => {
            InitializeAccount::new(account, mint, owner, rent_sysvar).into_batch(&mut batch)?;
        }
        None => {
            InitializeAccount3::new(account, mint, owner.address()).into_batch(&mut batch)?;
        }
    };

    batch.invoke_with_unverified_program(token_program)
}

// This cannot be inlined because it makes the call site stack frame too large.
pub(crate) fn batch_transfer_and_close(
    token_program: &Address,
    nested_ata: &AccountView,
    nested_token_mint: &AccountView,
    destination_ata: &AccountView,
    owner_ata: &AccountView,
    wallet: &AccountView,
    amount: u64,
    decimals: u8,
    signer: &[Signer],
) -> ProgramResult {
    const MAX_ACCOUNTS_LEN: usize =
        TransferChecked::MAX_ACCOUNTS_LEN + CloseAccount::MAX_ACCOUNTS_LEN;

    const DATA_LEN: usize =
        Batch::header_data_len(2) + TransferChecked::DATA_LEN + CloseAccount::DATA_LEN;

    let mut data = [const { MaybeUninit::<u8>::uninit() }; DATA_LEN];
    let mut instruction_accounts =
        [const { MaybeUninit::<InstructionAccount>::uninit() }; MAX_ACCOUNTS_LEN];
    let mut cpi_accounts = [const { MaybeUninit::<CpiAccount>::uninit() }; MAX_ACCOUNTS_LEN];

    // Serialize both sub-instructions into the buffers
    let mut batch = Batch::new(&mut data, &mut instruction_accounts, &mut cpi_accounts)?;

    // Move all tokens from the nested ATA to the wallet's correct ATA
    TransferChecked::new(
        nested_ata,
        nested_token_mint,
        destination_ata,
        owner_ata,
        amount,
        decimals,
    )
    .into_batch(&mut batch)?;

    // Close the now-empty nested ATA and return its rent lamports to the wallet
    CloseAccount::new(nested_ata, wallet, owner_ata).into_batch(&mut batch)?;

    batch.invoke_signed_with_unverified_program(signer, token_program)
}
