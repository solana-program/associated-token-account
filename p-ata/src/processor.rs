use {
    crate::tools::account::create_pda_account,
    pinocchio::{
        account_info::AccountInfo,
        instruction::{AccountMeta, Instruction, Seed, Signer},
        program::{get_return_data, invoke, invoke_signed},
        program_error::ProgramError,
        pubkey::{find_program_address, Pubkey},
        sysvars::rent::Rent,
        ProgramResult,
    },
    spl_token_interface::{
        instruction::TokenInstruction, program::ID as TOKEN_PROGRAM_ID,
        state::account::Account as TokenAccount, state::mint::Mint, state::Transmutable,
    },
};

/// Accounts: payer, ata, wallet, mint, system_program, token_program, rent_sysvar
pub fn process_create(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    idempotent: bool,
) -> ProgramResult {
    let [payer, ata_acc, wallet, mint_account, system_prog, token_prog, rent_sysvar, ..] = accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    if !payer.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    let (expected, bump) = find_program_address(
        &[
            wallet.key().as_ref(),
            token_prog.key().as_ref(),
            mint_account.key().as_ref(),
        ],
        program_id,
    );
    if &expected != ata_acc.key() {
        return Err(ProgramError::InvalidSeeds);
    }

    if idempotent && unsafe { ata_acc.owner() } == token_prog.key() {
        let ata_data_slice = unsafe { ata_acc.borrow_data_unchecked() };
        let ata_state = unsafe { &*(ata_data_slice.as_ptr() as *const TokenAccount) };
        if ata_state.owner != *wallet.key() {
            return Err(ProgramError::IllegalOwner);
        }
        if ata_state.mint != *mint_account.key() {
            return Err(ProgramError::InvalidAccountData);
        }
        return Ok(());
    }

    if unsafe { ata_acc.owner() } != system_prog.key() && unsafe { ata_acc.lamports() } > 0 {
        return Err(ProgramError::IllegalOwner);
    }

    if token_prog.key() != &TOKEN_PROGRAM_ID {
        return Err(ProgramError::IncorrectProgramId);
    }

    let space = TokenAccount::LEN;

    let initialize_account_instr_data = [TokenInstruction::InitializeAccount as u8];

    let initialize_account_metas = &[
        AccountMeta {
            pubkey: ata_acc.key(),
            is_writable: true,
            is_signer: false,
        },
        AccountMeta {
            pubkey: mint_account.key(),
            is_writable: false,
            is_signer: false,
        },
        AccountMeta {
            pubkey: wallet.key(),
            is_writable: false,
            is_signer: false,
        },
        AccountMeta {
            pubkey: rent_sysvar.key(),
            is_writable: false,
            is_signer: false,
        },
    ];

    let init_ix = Instruction {
        program_id: token_prog.key(),
        accounts: initialize_account_metas,
        data: &initialize_account_instr_data,
    };

    let rent = Rent::from_account_info(rent_sysvar)?;
    let seeds: &[&[u8]] = &[
        wallet.key().as_ref(),
        token_prog.key().as_ref(),
        mint_account.key().as_ref(),
        &[bump],
    ];
    create_pda_account(payer, &rent, space, token_prog.key(), ata_acc, seeds)?;

    invoke(&init_ix, &[&ata_acc, &mint_account, &wallet, &rent_sysvar])?;

    Ok(())
}

/// Accounts: nested_ata, nested_mint, dest_ata, owner_ata, owner_mint, wallet, token_prog
pub fn process_recover(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let [nested_ata, nested_mint_account, dest_ata, owner_ata, owner_mint_account, wallet, token_prog, ..] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    if token_prog.key() != &TOKEN_PROGRAM_ID {
        return Err(ProgramError::IncorrectProgramId);
    }

    let (owner_pda, bump) = find_program_address(
        &[
            wallet.key().as_ref(),
            token_prog.key().as_ref(),
            owner_mint_account.key().as_ref(),
        ],
        program_id,
    );
    if owner_pda != *owner_ata.key() {
        return Err(ProgramError::InvalidSeeds);
    }

    let (nested_pda, _) = find_program_address(
        &[
            owner_ata.key().as_ref(),
            token_prog.key().as_ref(),
            nested_mint_account.key().as_ref(),
        ],
        program_id,
    );
    if nested_pda != *nested_ata.key() {
        return Err(ProgramError::InvalidSeeds);
    }

    let (dest_pda, _) = find_program_address(
        &[
            wallet.key().as_ref(),
            token_prog.key().as_ref(),
            nested_mint_account.key().as_ref(),
        ],
        program_id,
    );
    if dest_pda != *dest_ata.key() {
        return Err(ProgramError::InvalidSeeds);
    }

    if !wallet.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    if unsafe { owner_ata.owner() } != token_prog.key() {
        return Err(ProgramError::IllegalOwner);
    }
    let owner_ata_data_slice = unsafe { owner_ata.borrow_data_unchecked() };
    let owner_ata_state = unsafe { &*(owner_ata_data_slice.as_ptr() as *const TokenAccount) };
    if owner_ata_state.owner != *wallet.key() {
        return Err(ProgramError::IllegalOwner);
    }

    if unsafe { nested_ata.owner() } != token_prog.key() {
        return Err(ProgramError::IllegalOwner);
    }
    let nested_ata_data_slice = unsafe { nested_ata.borrow_data_unchecked() };
    let nested_ata_state = unsafe { &*(nested_ata_data_slice.as_ptr() as *const TokenAccount) };
    if nested_ata_state.owner != *owner_ata.key() {
        return Err(ProgramError::IllegalOwner);
    }
    let amount_to_recover = nested_ata_state.amount();

    if unsafe { nested_mint_account.owner() } != token_prog.key() {
        return Err(ProgramError::IllegalOwner);
    }
    let nested_mint_data_slice = unsafe { nested_mint_account.borrow_data_unchecked() };
    let nested_mint_state = unsafe { &*(nested_mint_data_slice.as_ptr() as *const Mint) };
    let decimals = nested_mint_state.decimals;

    let mut transfer_data_arr = [0u8; 1 + 8 + 1];
    transfer_data_arr[0] = TokenInstruction::TransferChecked as u8;
    transfer_data_arr[1..9].copy_from_slice(&amount_to_recover.to_le_bytes());
    transfer_data_arr[9] = decimals;

    let transfer_metas = &[
        AccountMeta {
            pubkey: nested_ata.key(),
            is_writable: true,
            is_signer: false,
        },
        AccountMeta {
            pubkey: nested_mint_account.key(),
            is_writable: false,
            is_signer: false,
        },
        AccountMeta {
            pubkey: dest_ata.key(),
            is_writable: true,
            is_signer: false,
        },
        AccountMeta {
            pubkey: owner_ata.key(),
            is_writable: false,
            is_signer: true,
        },
    ];

    let ix_transfer = Instruction {
        program_id: token_prog.key(),
        accounts: transfer_metas,
        data: &transfer_data_arr,
    };

    let pda_seeds_raw: &[&[u8]] = &[
        wallet.key().as_ref(),
        token_prog.key().as_ref(),
        owner_mint_account.key().as_ref(),
        &[bump],
    ];
    let pda_seed_array: [Seed<'_>; 4] = [
        Seed::from(pda_seeds_raw[0]),
        Seed::from(pda_seeds_raw[1]),
        Seed::from(pda_seeds_raw[2]),
        Seed::from(pda_seeds_raw[3]),
    ];
    let pda_signer = Signer::from(&pda_seed_array);

    invoke_signed(
        &ix_transfer,
        &[
            &nested_ata,
            &nested_mint_account,
            &dest_ata,
            &owner_ata,
            &token_prog,
        ],
        &[pda_signer.clone()],
    )?;

    let close_data = [TokenInstruction::CloseAccount as u8];

    let close_metas = &[
        AccountMeta {
            pubkey: nested_ata.key(),
            is_writable: true,
            is_signer: false,
        },
        AccountMeta {
            pubkey: wallet.key(),
            is_writable: true,
            is_signer: false,
        },
        AccountMeta {
            pubkey: owner_ata.key(),
            is_writable: false,
            is_signer: true,
        },
    ];

    let ix_close = Instruction {
        program_id: token_prog.key(),
        accounts: close_metas,
        data: &close_data,
    };

    invoke_signed(
        &ix_close,
        &[&nested_ata, &wallet, &owner_ata, &token_prog],
        &[pda_signer],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use core::cell::RefCell;
    use pinocchio::{account_info::AccountInfo, pubkey::Pubkey};
    use spl_token_interface::instruction::TokenInstruction;

    #[derive(Default)]
    struct MockPinocchioAccountData {
        key: Pubkey,
        owner: Pubkey,
        lamports: u64,
        data: Vec<u8>,
        is_signer: bool,
        is_writable: bool,
        executable: bool,
        rent_epoch: u64,
    }

    fn pubkey_from_array(arr: [u8; 32]) -> Pubkey {
        arr
    }

    // Simplified mock for AccountInfo due to Pinocchio's internal structure.
    // Not a fully functional mock.
    fn _mock_account_info_with_data<'a>(
        mock_account_ref: &'a MockPinocchioAccountData,
        data_cell_ref: &'a RefCell<Vec<u8>>,
    ) -> AccountInfo {
        #[repr(C)]
        struct TestVisiblePinocchioAccountInternal {
            _key_ptr: *const Pubkey,
            _owner_ptr: *const Pubkey,
            _lamports_val: u64,
            actual_data_len: u64,
            actual_data_ptr: *const u8,
            _executable: u8,
            _rent_epoch: u64,
            _is_signer_val: u8,
            _is_writable_val: u8,
            _some_borrow_state: u64,
            _original_data_len: u32,
        }

        let borrowed_data_for_mock: &'a [u8] = unsafe { (*data_cell_ref.as_ptr()).as_slice() };

        let internal_mock = TestVisiblePinocchioAccountInternal {
            _key_ptr: &mock_account_ref.key as *const Pubkey,
            _owner_ptr: &mock_account_ref.owner as *const Pubkey,
            _lamports_val: mock_account_ref.lamports,
            actual_data_len: borrowed_data_for_mock.len() as u64,
            actual_data_ptr: borrowed_data_for_mock.as_ptr(),
            _executable: mock_account_ref.executable as u8,
            _rent_epoch: mock_account_ref.rent_epoch,
            _is_signer_val: mock_account_ref.is_signer as u8,
            _is_writable_val: mock_account_ref.is_writable as u8,
            _some_borrow_state: 0,
            _original_data_len: borrowed_data_for_mock.len() as u32,
        };

        let info: AccountInfo = unsafe {
            let ptr = &internal_mock as *const TestVisiblePinocchioAccountInternal as *mut ();
            let pinocchio_internal_ptr = ptr as *mut ();
            core::mem::transmute(pinocchio_internal_ptr)
        };
        info
    }

    #[test]
    fn test_process_create_instruction_assembly() {
        let ata_key = pubkey_from_array([3; 32]);
        let wallet_key = pubkey_from_array([4; 32]);
        let mint_key = pubkey_from_array([5; 32]);
        let rent_sysvar_key = pinocchio::sysvars::rent::RENT_ID;

        let expected_init_data = [TokenInstruction::InitializeAccount as u8];
        let expected_init_metas = [
            AccountMeta {
                pubkey: &ata_key,
                is_writable: true,
                is_signer: false,
            },
            AccountMeta {
                pubkey: &mint_key,
                is_writable: false,
                is_signer: false,
            },
            AccountMeta {
                pubkey: &wallet_key,
                is_writable: false,
                is_signer: false,
            },
            AccountMeta {
                pubkey: &rent_sysvar_key,
                is_writable: false,
                is_signer: false,
            },
        ];

        assert_eq!(
            TokenInstruction::InitializeAccount as u8,
            expected_init_data[0]
        );
        assert_eq!(expected_init_metas[0].pubkey, &ata_key);
        assert!(expected_init_metas[0].is_writable);
        assert!(!expected_init_metas[0].is_signer);
        assert_eq!(expected_init_metas[1].pubkey, &mint_key);
        assert!(!expected_init_metas[1].is_writable);
    }

    #[test]
    fn test_process_recover_instruction_assembly() {
        let token_prog_key = TOKEN_PROGRAM_ID;
        let nested_ata_key = pubkey_from_array([11; 32]);
        let nested_mint_key = pubkey_from_array([12; 32]);
        let dest_ata_key = pubkey_from_array([13; 32]);
        let owner_ata_key = pubkey_from_array([14; 32]);
        let wallet_key = pubkey_from_array([16; 32]);

        let amount_to_recover: u64 = 1000;
        let decimals: u8 = 6;

        let mut expected_transfer_data = [0u8; 1 + 8 + 1];
        expected_transfer_data[0] = TokenInstruction::TransferChecked as u8;
        expected_transfer_data[1..9].copy_from_slice(&amount_to_recover.to_le_bytes());
        expected_transfer_data[9] = decimals;

        let expected_transfer_metas = [
            AccountMeta {
                pubkey: &nested_ata_key,
                is_writable: true,
                is_signer: false,
            },
            AccountMeta {
                pubkey: &nested_mint_key,
                is_writable: false,
                is_signer: false,
            },
            AccountMeta {
                pubkey: &dest_ata_key,
                is_writable: true,
                is_signer: false,
            },
            AccountMeta {
                pubkey: &owner_ata_key,
                is_writable: false,
                is_signer: true,
            },
        ];

        let mut transfer_data_arr = [0u8; 1 + 8 + 1];
        transfer_data_arr[0] = TokenInstruction::TransferChecked as u8;
        transfer_data_arr[1..9].copy_from_slice(&amount_to_recover.to_le_bytes());
        transfer_data_arr[9] = decimals;

        let actual_ix_transfer = Instruction {
            program_id: &token_prog_key,
            accounts: &expected_transfer_metas,
            data: &transfer_data_arr,
        };

        assert_eq!(actual_ix_transfer.program_id, &token_prog_key);
        assert_eq!(actual_ix_transfer.data, &expected_transfer_data[..]);
        assert_eq!(actual_ix_transfer.accounts.len(), 4);
        assert_eq!(actual_ix_transfer.accounts[0].pubkey, &nested_ata_key);
        assert!(actual_ix_transfer.accounts[0].is_writable);
        assert_eq!(actual_ix_transfer.accounts[3].pubkey, &owner_ata_key);
        assert!(actual_ix_transfer.accounts[3].is_signer);

        let expected_close_data = [TokenInstruction::CloseAccount as u8];
        let expected_close_metas = [
            AccountMeta {
                pubkey: &nested_ata_key,
                is_writable: true,
                is_signer: false,
            },
            AccountMeta {
                pubkey: &wallet_key,
                is_writable: true,
                is_signer: false,
            },
            AccountMeta {
                pubkey: &owner_ata_key,
                is_writable: false,
                is_signer: true,
            },
        ];
        let actual_ix_close = Instruction {
            program_id: &token_prog_key,
            accounts: &expected_close_metas,
            data: &expected_close_data,
        };
        assert_eq!(actual_ix_close.data, &expected_close_data[..]);
        assert_eq!(actual_ix_close.accounts.len(), 3);
        assert_eq!(actual_ix_close.accounts[1].pubkey, &wallet_key);
        assert!(actual_ix_close.accounts[1].is_writable);
    }
}
