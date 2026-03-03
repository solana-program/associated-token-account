//! Processor and helpers related only to `RecoverNested` operations.

use {
    crate::processor::{
        build_transfer_checked_data, derive_canonical_ata_pda, get_decimals_from_mint,
        load_token_account,
    },
    pinocchio::{
        cpi::{invoke_signed, Seed, Signer},
        error::ProgramError,
        instruction::{InstructionAccount, InstructionView},
        AccountView, Address, ProgramResult,
    },
    pinocchio_log::log,
    solana_program_pack::Pack,
    spl_token_interface::{instruction::MAX_SIGNERS, state::Multisig},
};

pub const CLOSE_ACCOUNT_DISCRIMINATOR: u8 = 9;
pub const CLOSE_ACCOUNT_DATA: [u8; 1] = [CLOSE_ACCOUNT_DISCRIMINATOR];

/// Parsed Recover accounts for recover operations
pub struct RecoverNestedAccounts<'a> {
    pub nested_associated_token_account: &'a AccountView,
    pub nested_mint: &'a AccountView,
    pub destination_associated_token_account: &'a AccountView,
    pub owner_associated_token_account: &'a AccountView,
    pub owner_mint: &'a AccountView,
    pub wallet: &'a AccountView,
    pub token_program: &'a AccountView,
}

/// Parse and validate the standard Recover account layout.
#[inline(always)]
pub(crate) fn parse_recover_accounts(
    accounts: &[AccountView],
) -> Result<RecoverNestedAccounts<'_>, ProgramError> {
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
    program_id: &Address,
    accounts: &[AccountView],
) -> ProgramResult {
    let recover_accounts = parse_recover_accounts(accounts)?;

    let (owner_associated_token_address, owner_bump) = derive_canonical_ata_pda(
        recover_accounts.wallet.address(),
        recover_accounts.token_program.address(),
        recover_accounts.owner_mint.address(),
        program_id,
    );

    if owner_associated_token_address != *recover_accounts.owner_associated_token_account.address()
    {
        log!("Error: Owner associated address does not match seed derivation");
        return Err(ProgramError::InvalidSeeds);
    }

    let (nested_associated_token_address, _) = derive_canonical_ata_pda(
        recover_accounts.owner_associated_token_account.address(),
        recover_accounts.token_program.address(),
        recover_accounts.nested_mint.address(),
        program_id,
    );

    if nested_associated_token_address
        != *recover_accounts.nested_associated_token_account.address()
    {
        log!("Error: Nested associated address does not match seed derivation");
        return Err(ProgramError::InvalidSeeds);
    }

    let (destination_associated_token_address, _) = derive_canonical_ata_pda(
        recover_accounts.wallet.address(),
        recover_accounts.token_program.address(),
        recover_accounts.nested_mint.address(),
        program_id,
    );

    if destination_associated_token_address
        != *recover_accounts
            .destination_associated_token_account
            .address()
    {
        log!("Error: Destination associated address does not match seed derivation");
        return Err(ProgramError::InvalidSeeds);
    }

    // Validate that the owner ATA exists and is a valid token account
    let _owner_token_account = load_token_account(recover_accounts.owner_associated_token_account)?;

    // Handle multisig case
    if !recover_accounts.wallet.is_signer() {
        // Multisig case: must be token-program owned
        if !recover_accounts
            .wallet
            .owned_by(recover_accounts.token_program.address())
        {
            log!(
                "Error: Multisig wallet {} is not owned by token program {}",
                recover_accounts.wallet.address().as_ref(),
                recover_accounts.token_program.address().as_ref()
            );
            return Err(ProgramError::MissingRequiredSignature);
        }

        let wallet_data_slice = unsafe { recover_accounts.wallet.borrow_unchecked() };
        let multisig_state =
            Multisig::unpack(wallet_data_slice).map_err(|_| ProgramError::InvalidAccountData)?;

        let signer_infos = &accounts[7..];

        let mut num_signers: u8 = 0;
        let mut matched = [false; MAX_SIGNERS];

        for signer in signer_infos.iter() {
            for (position, key) in multisig_state.signers[0..multisig_state.n as usize]
                .iter()
                .enumerate()
            {
                if key.to_bytes() == signer.address().to_bytes() && !matched[position] {
                    if !signer.is_signer() {
                        log!(
                            "Error: Multisig member account {} is not a signer",
                            signer.address().as_ref()
                        );
                        return Err(ProgramError::MissingRequiredSignature);
                    }
                    matched[position] = true;
                    num_signers = num_signers.checked_add(1u8).expect("num_signers overflow");
                }
            }
        }

        if num_signers < multisig_state.m {
            log!(
                "Error: Insufficient multisig signatures. Required: {}, Found: {}",
                multisig_state.m,
                num_signers
            );
            return Err(ProgramError::MissingRequiredSignature);
        }
    }

    let amount_to_recover =
        load_token_account(recover_accounts.nested_associated_token_account)?.amount;

    let nested_mint_decimals = get_decimals_from_mint(recover_accounts.nested_mint)?;

    let transfer_data = build_transfer_checked_data(amount_to_recover, nested_mint_decimals);

    let transfer_metas = &[
        InstructionAccount::writable(recover_accounts.nested_associated_token_account.address()),
        InstructionAccount::readonly(recover_accounts.nested_mint.address()),
        InstructionAccount::writable(
            recover_accounts
                .destination_associated_token_account
                .address(),
        ),
        InstructionAccount::readonly_signer(
            recover_accounts.owner_associated_token_account.address(),
        ),
    ];

    let ix_transfer = InstructionView {
        program_id: recover_accounts.token_program.address(),
        accounts: transfer_metas,
        data: &transfer_data,
    };

    let owner_bump_slice = [owner_bump];
    let pda_seed_array: [Seed<'_>; 4] = [
        Seed::from(recover_accounts.wallet.address().as_ref()),
        Seed::from(recover_accounts.token_program.address().as_ref()),
        Seed::from(recover_accounts.owner_mint.address().as_ref()),
        Seed::from(&owner_bump_slice[..]),
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
        core::slice::from_ref(&pda_signer),
    )?;

    let close_metas = &[
        InstructionAccount::writable(recover_accounts.nested_associated_token_account.address()),
        InstructionAccount::writable(recover_accounts.wallet.address()),
        InstructionAccount::readonly_signer(
            recover_accounts.owner_associated_token_account.address(),
        ),
    ];

    let ix_close = InstructionView {
        program_id: recover_accounts.token_program.address(),
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
        pinocchio::{account::RuntimeAccount, error::ProgramError, AccountView, Address},
        std::{ptr, vec::Vec},
    };

    fn with_test_accounts<F, R>(count: usize, test_fn: F) -> R
    where
        F: FnOnce(&[AccountView]) -> R,
    {
        let mut account_data: Vec<RuntimeAccount> = (0..count)
            .map(|i| RuntimeAccount {
                borrow_state: 0b_1111_1111,
                is_signer: 0,
                is_writable: 0,
                executable: 0,
                resize_delta: 0,
                address: Address::from([i as u8; 32]),
                owner: Address::from([(i as u8).wrapping_add(1); 32]),
                lamports: 0,
                data_len: 0,
            })
            .collect();

        let account_infos: Vec<AccountView> = account_data
            .iter_mut()
            .map(|layout| unsafe { AccountView::new_unchecked(layout as *mut RuntimeAccount) })
            .collect();

        test_fn(&account_infos)
    }

    #[test]
    fn test_parse_recover_accounts_success() {
        with_test_accounts(7, |accounts: &[AccountView]| {
            assert_eq!(accounts.len(), 7);

            let parsed = parse_recover_accounts(accounts).unwrap();

            assert!(ptr::eq(
                parsed.nested_associated_token_account,
                &accounts[0]
            ));
            assert_eq!(
                parsed.nested_associated_token_account.address(),
                accounts[0].address()
            );
            assert!(ptr::eq(parsed.nested_mint, &accounts[1]));
            assert_eq!(parsed.nested_mint.address(), accounts[1].address());
            assert!(ptr::eq(
                parsed.destination_associated_token_account,
                &accounts[2]
            ));
            assert_eq!(
                parsed.destination_associated_token_account.address(),
                accounts[2].address()
            );
            assert!(ptr::eq(parsed.owner_associated_token_account, &accounts[3]));
            assert_eq!(
                parsed.owner_associated_token_account.address(),
                accounts[3].address()
            );
            assert!(ptr::eq(parsed.owner_mint, &accounts[4]));
            assert_eq!(parsed.owner_mint.address(), accounts[4].address());
            assert!(ptr::eq(parsed.wallet, &accounts[5]));
            assert_eq!(parsed.wallet.address(), accounts[5].address());
            assert!(ptr::eq(parsed.token_program, &accounts[6]));
            assert_eq!(parsed.token_program.address(), accounts[6].address());
        });
    }

    #[test]
    fn test_parse_recover_accounts_error_insufficient() {
        with_test_accounts(6, |accounts: &[AccountView]| {
            assert!(matches!(
                parse_recover_accounts(accounts),
                Err(ProgramError::NotEnoughAccountKeys)
            ));
        });
    }
}
