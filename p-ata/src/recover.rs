//! Processor and helpers related only to `RecoverNested` operations.

use {
    crate::processor::{
        build_transfer_checked_data, derive_canonical_ata_pda, get_decimals_from_mint,
        get_token_account,
    },
    pinocchio::{
        account_info::AccountInfo,
        cpi::invoke_signed,
        instruction::{AccountMeta, Instruction, Seed, Signer},
        msg,
        program_error::ProgramError,
        pubkey::Pubkey,
        ProgramResult,
    },
    spl_token_interface::state::multisig::{Multisig, MAX_SIGNERS},
};

pub const CLOSE_ACCOUNT_DISCRIMINATOR: u8 = 9;
pub const CLOSE_ACCOUNT_DATA: [u8; 1] = [CLOSE_ACCOUNT_DISCRIMINATOR];

/// Parsed Recover accounts for recover operations
pub struct RecoverNestedAccounts<'a> {
    pub nested_associated_token_account: &'a AccountInfo,
    pub nested_mint: &'a AccountInfo,
    pub destination_associated_token_account: &'a AccountInfo,
    pub owner_associated_token_account: &'a AccountInfo,
    pub owner_mint: &'a AccountInfo,
    pub wallet: &'a AccountInfo,
    pub token_program: &'a AccountInfo,
}

/// Parse and validate the standard Recover account layout.
#[inline(always)]
pub(crate) fn parse_recover_accounts(
    accounts: &[AccountInfo],
) -> Result<RecoverNestedAccounts, ProgramError> {
    if accounts.len() < 7 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    // SAFETY: account len already checked
    unsafe {
        Ok(RecoverNestedAccounts {
            nested_associated_token_account: accounts.get_unchecked(0),
            nested_mint: accounts.get_unchecked(1),
            destination_associated_token_account: accounts.get_unchecked(2),
            owner_associated_token_account: accounts.get_unchecked(3),
            owner_mint: accounts.get_unchecked(4),
            wallet: accounts.get_unchecked(5),
            token_program: accounts.get_unchecked(6),
        })
    }
}

/// Transfer all tokens from a nested associated token account (an ATA created
/// with another ATA as the wallet) to the canonical ATA and then closes the
/// nested account to recover rent.
///
/// ## Account Layout
/// ```ignore
/// [0] nested_associated_token_account  (writable) - source account to drain
/// [1] nested_mint                                 - mint of tokens being recovered  
/// [2] destination_associated_token_account (writable) - canonical destination ATA
/// [3] owner_associated_token_account              - ATA that "owns" the nested ATA
/// [4] owner_mint                                  - mint for the owner ATA  
/// [5] wallet                           (signer)   - ultimate owner wallet
/// [6] token_program                               - token program for operations
/// [7..] multisig signer accounts       (signers)  - if wallet is multisig
/// ```
///
/// - The owner ATA must properly derive the nested ATA
/// - The wallet must properly derive the owner ATA and destination ATA
/// - The nested mint must properly derive the nested ATA and destination ATA
pub(crate) fn process_recover_nested(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let recover_accounts = parse_recover_accounts(accounts)?;

    let (owner_associated_token_address, owner_bump) = derive_canonical_ata_pda(
        recover_accounts.wallet.key(),
        recover_accounts.token_program.key(),
        recover_accounts.owner_mint.key(),
        program_id,
    );

    if owner_associated_token_address != *recover_accounts.owner_associated_token_account.key() {
        msg!("Error: Owner associated address does not match seed derivation");
        return Err(ProgramError::InvalidSeeds);
    }

    let (nested_associated_token_address, _) = derive_canonical_ata_pda(
        recover_accounts.owner_associated_token_account.key(),
        recover_accounts.token_program.key(),
        recover_accounts.nested_mint.key(),
        program_id,
    );

    if nested_associated_token_address != *recover_accounts.nested_associated_token_account.key() {
        msg!("Error: Nested associated address does not match seed derivation");
        return Err(ProgramError::InvalidSeeds);
    }

    let (destination_associated_token_address, _) = derive_canonical_ata_pda(
        recover_accounts.wallet.key(),
        recover_accounts.token_program.key(),
        recover_accounts.nested_mint.key(),
        program_id,
    );

    if destination_associated_token_address
        != *recover_accounts.destination_associated_token_account.key()
    {
        msg!("Error: Destination associated address does not match seed derivation");
        return Err(ProgramError::InvalidSeeds);
    }

    // Validate that the owner ATA exists and is a valid token account
    let _owner_token_account = get_token_account(recover_accounts.owner_associated_token_account)?;

    // Handle multisig case
    if !recover_accounts.wallet.is_signer() {
        // Multisig case: must be token-program owned
        if !recover_accounts
            .wallet
            .is_owned_by(recover_accounts.token_program.key())
        {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let wallet_data_slice = unsafe { recover_accounts.wallet.borrow_data_unchecked() };
        let multisig_state: &Multisig = unsafe {
            spl_token_interface::state::load::<Multisig>(wallet_data_slice)
                .map_err(|_| ProgramError::InvalidAccountData)?
        };

        let signer_infos = &accounts[7..];

        let mut num_signers = 0;
        let mut matched = [false; MAX_SIGNERS as usize];

        for signer in signer_infos.iter() {
            for (position, key) in multisig_state.signers[0..multisig_state.n as usize]
                .iter()
                .enumerate()
            {
                if key == signer.key() && !matched[position] {
                    if !signer.is_signer() {
                        return Err(ProgramError::MissingRequiredSignature);
                    }
                    matched[position] = true;
                    num_signers += 1;
                }
            }
        }

        if num_signers < multisig_state.m {
            return Err(ProgramError::MissingRequiredSignature);
        }
    }

    let amount_to_recover =
        get_token_account(recover_accounts.nested_associated_token_account)?.amount();

    let nested_mint_decimals = get_decimals_from_mint(recover_accounts.nested_mint)?;

    let transfer_data = build_transfer_checked_data(amount_to_recover, nested_mint_decimals);

    let transfer_metas = &[
        AccountMeta {
            pubkey: recover_accounts.nested_associated_token_account.key(),
            is_writable: true,
            is_signer: false,
        },
        AccountMeta {
            pubkey: recover_accounts.nested_mint.key(),
            is_writable: false,
            is_signer: false,
        },
        AccountMeta {
            pubkey: recover_accounts.destination_associated_token_account.key(),
            is_writable: true,
            is_signer: false,
        },
        AccountMeta {
            pubkey: recover_accounts.owner_associated_token_account.key(),
            is_writable: false,
            is_signer: true,
        },
    ];

    let ix_transfer = Instruction {
        program_id: recover_accounts.token_program.key(),
        accounts: transfer_metas,
        data: &transfer_data,
    };

    let pda_seeds_raw: &[&[u8]] = &[
        recover_accounts.wallet.key().as_ref(),
        recover_accounts.token_program.key().as_ref(),
        recover_accounts.owner_mint.key().as_ref(),
        &[owner_bump],
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
            recover_accounts.nested_associated_token_account,
            recover_accounts.nested_mint,
            recover_accounts.destination_associated_token_account,
            recover_accounts.owner_associated_token_account,
        ],
        &[pda_signer.clone()],
    )?;

    let close_metas = &[
        AccountMeta {
            pubkey: recover_accounts.nested_associated_token_account.key(),
            is_writable: true,
            is_signer: false,
        },
        AccountMeta {
            pubkey: recover_accounts.wallet.key(),
            is_writable: true,
            is_signer: false,
        },
        AccountMeta {
            pubkey: recover_accounts.owner_associated_token_account.key(),
            is_writable: false,
            is_signer: true,
        },
    ];

    let ix_close = Instruction {
        program_id: recover_accounts.token_program.key(),
        accounts: close_metas,
        data: &CLOSE_ACCOUNT_DATA,
    };

    invoke_signed(
        &ix_close,
        &[
            recover_accounts.nested_associated_token_account,
            recover_accounts.wallet,
            recover_accounts.owner_associated_token_account,
        ],
        &[pda_signer],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use {
        pinocchio::{account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey},
        std::{ptr, vec::Vec},
    };

    // Test utility - AccountLayout structure for creating test accounts
    #[repr(C)]
    struct AccountLayout {
        borrow_state: u8,
        is_signer: u8,
        is_writable: u8,
        executable: u8,
        resize_delta: u32,
        key: Pubkey,
        owner: Pubkey,
        lamports: u64,
        data_len: u64,
    }

    fn with_test_accounts<F, R>(count: usize, test_fn: F) -> R
    where
        F: FnOnce(&[AccountInfo]) -> R,
    {
        let mut account_data: Vec<AccountLayout> = (0..count)
            .map(|i| AccountLayout {
                borrow_state: 0b_1111_1111,
                is_signer: 0,
                is_writable: 0,
                executable: 0,
                resize_delta: 0,
                key: Pubkey::from([i as u8; 32]),
                owner: Pubkey::from([(i as u8).wrapping_add(1); 32]),
                lamports: 0,
                data_len: 0,
            })
            .collect();

        let account_infos: Vec<AccountInfo> = account_data
            .iter_mut()
            .map(|layout| unsafe { std::mem::transmute::<*mut AccountLayout, AccountInfo>(layout) })
            .collect();

        test_fn(&account_infos)
    }

    #[test]
    fn test_parse_recover_accounts_success() {
        with_test_accounts(7, |accounts| {
            assert_eq!(accounts.len(), 7);

            let parsed = parse_recover_accounts(accounts).unwrap();

            assert!(ptr::eq(
                parsed.nested_associated_token_account,
                &accounts[0]
            ));
            assert_eq!(
                parsed.nested_associated_token_account.key(),
                accounts[0].key()
            );
            assert!(ptr::eq(parsed.nested_mint, &accounts[1]));
            assert_eq!(parsed.nested_mint.key(), accounts[1].key());
            assert!(ptr::eq(
                parsed.destination_associated_token_account,
                &accounts[2]
            ));
            assert_eq!(
                parsed.destination_associated_token_account.key(),
                accounts[2].key()
            );
            assert!(ptr::eq(parsed.owner_associated_token_account, &accounts[3]));
            assert_eq!(
                parsed.owner_associated_token_account.key(),
                accounts[3].key()
            );
            assert!(ptr::eq(parsed.owner_mint, &accounts[4]));
            assert_eq!(parsed.owner_mint.key(), accounts[4].key());
            assert!(ptr::eq(parsed.wallet, &accounts[5]));
            assert_eq!(parsed.wallet.key(), accounts[5].key());
            assert!(ptr::eq(parsed.token_program, &accounts[6]));
            assert_eq!(parsed.token_program.key(), accounts[6].key());
        });
    }

    #[test]
    fn test_parse_recover_accounts_error_insufficient() {
        with_test_accounts(6, |accounts| {
            assert!(matches!(
                parse_recover_accounts(accounts),
                Err(ProgramError::NotEnoughAccountKeys)
            ));
        });
    }
}
