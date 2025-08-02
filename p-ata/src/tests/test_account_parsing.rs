#![cfg(test)]

use pinocchio::program_error::ProgramError;

use pinocchio::{account_info::AccountInfo, pubkey::Pubkey};

use std::{ptr, vec::Vec};

use crate::{
    processor::parse_create_accounts, recover::parse_recover_accounts,
    tests::test_utils::AccountLayout,
};

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
fn test_parse_create_accounts_success_without_rent() {
    // Exactly 6 accounts – rent sysvar should be `None`.
    with_test_accounts(6, |accounts| {
        let parsed = parse_create_accounts(accounts).unwrap();

        assert!(ptr::eq(parsed.payer, &accounts[0]));
        assert_eq!(parsed.payer.key(), accounts[0].key());
        assert!(ptr::eq(
            parsed.associated_token_account_to_create,
            &accounts[1]
        ));
        assert_eq!(
            parsed.associated_token_account_to_create.key(),
            accounts[1].key()
        );
        assert!(ptr::eq(parsed.wallet, &accounts[2]));
        assert_eq!(parsed.wallet.key(), accounts[2].key());
        assert!(ptr::eq(parsed.mint, &accounts[3]));
        assert_eq!(parsed.mint.key(), accounts[3].key());
        assert!(ptr::eq(parsed.system_program, &accounts[4]));
        assert_eq!(parsed.system_program.key(), accounts[4].key());
        assert!(ptr::eq(parsed.token_program, &accounts[5]));
        assert_eq!(parsed.token_program.key(), accounts[5].key());
        assert!(parsed.rent_sysvar.is_none());
    });
}

#[test]
fn test_parse_create_accounts_success_with_rent() {
    // 7 accounts – index 6 is rent sysvar.
    with_test_accounts(7, |accounts| {
        assert_eq!(accounts.len(), 7);

        let parsed = parse_create_accounts(accounts).unwrap();

        assert!(parsed.rent_sysvar.is_some());
        assert!(ptr::eq(parsed.rent_sysvar.unwrap(), &accounts[6]));
        assert_eq!(parsed.rent_sysvar.unwrap().key(), accounts[6].key());
    });
}

#[test]
fn test_parse_create_accounts_error_insufficient() {
    with_test_accounts(5, |accounts| {
        assert!(matches!(
            parse_create_accounts(accounts),
            Err(ProgramError::NotEnoughAccountKeys)
        ));
    });
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
