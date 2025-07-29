use {
    crate::processor::{parse_create_accounts, parse_recover_accounts},
    pinocchio::{account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey},
};

use std::{ptr, vec::Vec};

use crate::tests::test_utils::AccountLayout;

/// Create test AccountInfo instances.
fn make_test_accounts(count: usize) -> Vec<AccountInfo> {
    let mut account_data: Vec<AccountLayout> = Vec::with_capacity(count);

    for i in 0..count {
        account_data.push(AccountLayout {
            borrow_state: 0b_1111_1111,
            is_signer: 0,
            is_writable: 0,
            executable: 0,
            resize_delta: 0,
            key: Pubkey::from([i as u8; 32]),
            owner: Pubkey::from([(i as u8).wrapping_add(1); 32]),
            lamports: 0,
            data_len: 0,
        });
    }

    // Leak the data to ensure it lives for the duration of the test
    let leaked_data = account_data.leak();

    // Create AccountInfo instances using safe transmute
    // This is safe because:
    // 1. AccountLayout is designed to have identical layout to internal Account struct
    // 2. We're just changing the pointer type, not the data representation
    // 3. We verify the sizes match at compile time in test_utils.rs
    leaked_data
        .iter_mut()
        .map(|layout| unsafe {
            // Convert AccountLayout pointer to AccountInfo
            // This transmutes &mut AccountLayout -> AccountInfo (which contains *mut Account)
            std::mem::transmute::<*mut AccountLayout, AccountInfo>(layout)
        })
        .collect()
}

#[test]
fn test_parse_create_accounts_success_without_rent() {
    // Exactly 6 accounts – rent sysvar should be `None`.
    let accounts = make_test_accounts(6);

    let parsed = parse_create_accounts(&accounts).unwrap();

    assert!(ptr::eq(parsed.payer, &accounts[0]));
    assert!(ptr::eq(
        parsed.associated_token_account_to_create,
        &accounts[1]
    ));
    assert!(ptr::eq(parsed.wallet, &accounts[2]));
    assert!(ptr::eq(parsed.mint, &accounts[3]));
    assert!(ptr::eq(parsed.system_program, &accounts[4]));
    assert!(ptr::eq(parsed.token_program, &accounts[5]));
    assert!(parsed.rent_sysvar.is_none());
}

#[test]
fn test_parse_create_accounts_success_with_rent() {
    // 7 accounts – index 6 is rent sysvar.
    let accounts = make_test_accounts(7);
    assert_eq!(accounts.len(), 7);

    let parsed = parse_create_accounts(&accounts).unwrap();

    assert!(parsed.rent_sysvar.is_some());
    assert!(ptr::eq(parsed.rent_sysvar.unwrap(), &accounts[6]));
}

#[test]
fn test_parse_create_accounts_error_insufficient() {
    let accounts = make_test_accounts(5);
    assert!(matches!(
        parse_create_accounts(&accounts),
        Err(ProgramError::NotEnoughAccountKeys)
    ));
}

#[test]
fn test_parse_recover_accounts_success() {
    let accounts = make_test_accounts(7);
    assert_eq!(accounts.len(), 7);

    let parsed = parse_recover_accounts(&accounts).unwrap();

    assert!(ptr::eq(
        parsed.nested_associated_token_account,
        &accounts[0]
    ));
    assert!(ptr::eq(parsed.nested_mint, &accounts[1]));
    assert!(ptr::eq(
        parsed.destination_associated_token_account,
        &accounts[2]
    ));
    assert!(ptr::eq(parsed.owner_associated_token_account, &accounts[3]));
    assert!(ptr::eq(parsed.owner_mint, &accounts[4]));
    assert!(ptr::eq(parsed.wallet, &accounts[5]));
    assert!(ptr::eq(parsed.token_program, &accounts[6]));
}

#[test]
fn test_parse_recover_accounts_error_insufficient() {
    let accounts = make_test_accounts(6);
    assert!(matches!(
        parse_recover_accounts(&accounts),
        Err(ProgramError::NotEnoughAccountKeys)
    ));
}
