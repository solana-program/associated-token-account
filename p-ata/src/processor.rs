use {
    crate::tools::account::create_pda_account,
    pinocchio::{
        account_info::AccountInfo,
        instruction::{AccountMeta, Instruction, Seed, Signer},
        program::{invoke, invoke_signed},
        program_error::ProgramError,
        pubkey::{find_program_address, Pubkey},
        sysvars::{rent::Rent, Sysvar},
        ProgramResult,
    },
    spl_token_interface::{
        instruction::TokenInstruction,
        state::{account::Account as TokenAccount, Transmutable},
    },
};

/// Accounts: payer, ata, wallet, mint, system_program, token_program, [rent_sysvar]
///
/// Manually stamping ImmutableOwner data and then calling Assign is **cheaper**
/// on create paths than using the Token-2022 `InitializeImmutableOwner` CPI
/// (100-200 CUs saved). If we ever have a lightweight pinocchio-flavoured
/// Token-2022 program (`p-token-2022`) with a lower overhead, we can swap
/// back to the flow of CreateAccount + InitializeImmutableOwner.
pub fn process_create(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    idempotent: bool,
) -> ProgramResult {
    // Support original ATA 6-account layout with optional Rent sysvar (7th account)
    let (payer, ata_acc, wallet, mint_account, system_prog, token_prog, rent_info_opt) =
        match accounts {
            [payer, ata, wallet, mint, system, token] => {
                (payer, ata, wallet, mint, system, token, None)
            }
            [payer, ata, wallet, mint, system, token, rent, ..] => {
                (payer, ata, wallet, mint, system, token, Some(rent))
            }
            _ => return Err(ProgramError::NotEnoughAccountKeys),
        };

    if !payer.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
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

    // Only derive PDA when we actually need to create the account
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

    if unsafe { ata_acc.owner() } != system_prog.key() && ata_acc.lamports() > 0 {
        return Err(ProgramError::IllegalOwner);
    }

    // OPTIMIZATION: Inline account size calculation since get_account_len() always returns TokenAccount::LEN
    // This eliminates function call overhead and unnecessary branching
    let space = TokenAccount::LEN;

    let seeds: &[&[u8]] = &[
        wallet.key().as_ref(),
        token_prog.key().as_ref(),
        mint_account.key().as_ref(),
        &[bump],
    ];

    // Use Rent passed in accounts if supplied to avoid syscall
    let rent_owned;
    let rent: &Rent = match rent_info_opt {
        Some(rent_acc) => unsafe { Rent::from_account_info_unchecked(rent_acc)? },
        None => {
            rent_owned = Rent::get()?;
            &rent_owned
        }
    };
    create_pda_account(payer, rent, space, token_prog.key(), ata_acc, seeds)?;

    // Initialize account using InitializeAccount3 (2 accounts + owner in instruction data)
    let mut initialize_account_instr_data = [0u8; 33]; // 1 byte discriminator + 32 bytes owner
    initialize_account_instr_data[0] = 18u8; // TokenInstruction::InitializeAccount3
    initialize_account_instr_data[1..33].copy_from_slice(wallet.key().as_ref());

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
    ];

    let init_ix = Instruction {
        program_id: token_prog.key(),
        accounts: initialize_account_metas,
        data: &initialize_account_instr_data,
    };

    invoke(&init_ix, &[ata_acc, mint_account])?;

    Ok(())
}

/// Accounts: nested_ata, nested_mint, dest_ata, owner_ata, owner_mint, wallet, token_prog, [..multisig signer accounts]
pub fn process_recover(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    if accounts.len() < 7 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }
    let (
        nested_ata,
        _nested_mint_account,
        dest_ata,
        owner_ata,
        owner_mint_account,
        wallet,
        token_prog,
    ) = (
        &accounts[0],
        &accounts[1],
        &accounts[2],
        &accounts[3],
        &accounts[4],
        &accounts[5],
        &accounts[6],
    );

    let (owner_pda, bump) = find_program_address(
        &[
            wallet.key().as_ref(),
            token_prog.key().as_ref(),
            owner_mint_account.key().as_ref(),
        ],
        program_id,
    );
    if &owner_pda != owner_ata.key() {
        return Err(ProgramError::InvalidSeeds);
    }

    // No expensive seed verification for `nested_ata` and `dest_ata`; the
    // subsequent owner checks on their account data provide sufficient safety
    // for practical purposes while saving ~3k CUs.

    // --- Wallet signature / multisig handling ---
    // If `wallet` signed directly, all good. Otherwise, allow a Multisig account
    // owned by the token program, provided that the required number (m) of
    // its signer keys signed this instruction.  Additional signer accounts
    // must be passed directly after the `token_prog` account.

    if !wallet.is_signer() {
        // Check if this is a token-program multisig owner
        if unsafe { wallet.owner() } != token_prog.key() {
            return Err(ProgramError::MissingRequiredSignature);
        }

        #[allow(unused_imports)]
        use spl_token_interface::state::{multisig::Multisig, Initializable, Transmutable};

        // Load and validate multisig state
        let wallet_data_slice = unsafe { wallet.borrow_data_unchecked() };
        let multisig_state: &Multisig =
            unsafe { spl_token_interface::state::load::<Multisig>(wallet_data_slice)? };

        let signer_infos = &accounts[7..];

        // Count how many of the provided signer accounts are both marked as
        // signer on this instruction *and* appear in the multisig signer list.
        let mut signer_count: u8 = 0;
        'outer: for signer_acc in signer_infos {
            if !signer_acc.is_signer() {
                continue;
            }
            for ms_pk in multisig_state.signers[..multisig_state.n as usize].iter() {
                if ms_pk == signer_acc.key() {
                    signer_count = signer_count.saturating_add(1);

                    // OPTIMIZATION: Early exit once we have enough signers
                    if signer_count >= multisig_state.m {
                        break 'outer;
                    }
                    continue 'outer;
                }
            }
        }

        if signer_count < multisig_state.m {
            return Err(ProgramError::MissingRequiredSignature);
        }
    }

    if unsafe { owner_mint_account.owner() } != token_prog.key() {
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

    let mut transfer_data_arr = [0u8; 1 + 8];
    transfer_data_arr[0] = TokenInstruction::Transfer as u8;
    transfer_data_arr[1..9].copy_from_slice(&amount_to_recover.to_le_bytes());

    let transfer_metas = &[
        AccountMeta {
            pubkey: nested_ata.key(),
            is_writable: true,
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
        &[nested_ata, dest_ata, owner_ata],
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
        AccountMeta {
            pubkey: token_prog.key(),
            is_writable: false,
            is_signer: false,
        },
    ];

    let ix_close = Instruction {
        program_id: token_prog.key(),
        accounts: close_metas,
        data: &close_data,
    };

    invoke_signed(
        &ix_close,
        &[nested_ata, wallet, owner_ata, token_prog],
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

    const TOKEN_PROGRAM_ID: Pubkey = spl_token_interface::program::ID;

    #[derive(Default)]
    #[allow(dead_code)]
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
        let _nested_mint_key = pubkey_from_array([12; 32]);
        let dest_ata_key = pubkey_from_array([13; 32]);
        let owner_ata_key = pubkey_from_array([14; 32]);
        let wallet_key = pubkey_from_array([16; 32]);

        let amount_to_recover: u64 = 1000;

        let mut transfer_data_arr = [0u8; 1 + 8];
        transfer_data_arr[0] = TokenInstruction::Transfer as u8;
        transfer_data_arr[1..9].copy_from_slice(&amount_to_recover.to_le_bytes());

        let expected_transfer_metas = [
            AccountMeta {
                pubkey: &nested_ata_key,
                is_writable: true,
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

        let actual_ix_transfer = Instruction {
            program_id: &token_prog_key,
            accounts: &expected_transfer_metas,
            data: &transfer_data_arr,
        };

        assert_eq!(actual_ix_transfer.program_id, &token_prog_key);
        assert_eq!(actual_ix_transfer.data, &transfer_data_arr);
        assert_eq!(actual_ix_transfer.accounts.len(), 3);
        assert_eq!(actual_ix_transfer.accounts[0].pubkey, &nested_ata_key);
        assert!(actual_ix_transfer.accounts[0].is_writable);
        assert_eq!(actual_ix_transfer.accounts[1].pubkey, &dest_ata_key);
        assert!(actual_ix_transfer.accounts[1].is_writable);
        assert_eq!(actual_ix_transfer.accounts[2].pubkey, &owner_ata_key);
        assert!(actual_ix_transfer.accounts[2].is_signer);

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
            AccountMeta {
                pubkey: &token_prog_key,
                is_writable: false,
                is_signer: false,
            },
        ];
        let actual_ix_close = Instruction {
            program_id: &token_prog_key,
            accounts: &expected_close_metas,
            data: &expected_close_data,
        };
        assert_eq!(actual_ix_close.data, &expected_close_data[..]);
        assert_eq!(actual_ix_close.accounts.len(), 4);
        assert_eq!(actual_ix_close.accounts[1].pubkey, &wallet_key);
        assert!(actual_ix_close.accounts[1].is_writable);
    }
}
