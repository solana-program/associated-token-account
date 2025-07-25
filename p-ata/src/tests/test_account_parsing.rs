use {
    crate::processor::{parse_create_accounts, parse_recover_accounts},
    pinocchio::{account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey},
};

use std::{mem, ptr, vec::Vec};

use crate::tests::test_utils::AccountLayout;

/// Produce an `AccountInfo` instance by byte-copying from an `AccountLayout`.
/// `AccountInfo` members are private, so this is the only way.
fn make_test_account(seed: u8) -> AccountInfo {
    let layout = AccountLayout {
        borrow_state: 0,
        is_signer: 0,
        is_writable: 0,
        executable: 0,
        resize_delta: 0,
        key: Pubkey::from([seed; 32]),
        owner: Pubkey::from([seed.wrapping_add(1); 32]),
        lamports: 0,
        data_len: 0,
    };

    let mut info_uninit = mem::MaybeUninit::<AccountInfo>::uninit();
    unsafe {
        let src_ptr = &layout as *const AccountLayout as *const u8;
        let dst_ptr = info_uninit.as_mut_ptr() as *mut u8;
        let copy_len = core::cmp::min(
            mem::size_of::<AccountLayout>(),
            mem::size_of::<AccountInfo>(),
        );
        ptr::copy_nonoverlapping(src_ptr, dst_ptr, copy_len);
        info_uninit.assume_init()
    }
}

#[test]
fn test_parse_create_accounts_success_without_rent() {
    // Exactly 6 accounts – rent sysvar should be `None`.
    let accounts: Vec<AccountInfo> = (0..6).map(make_test_account).collect();

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
    let accounts: Vec<AccountInfo> = (10..17).map(|s| make_test_account(s as u8)).collect();
    assert_eq!(accounts.len(), 7);

    let parsed = parse_create_accounts(&accounts).unwrap();

    assert!(parsed.rent_sysvar.is_some());
    assert!(ptr::eq(parsed.rent_sysvar.unwrap(), &accounts[6]));
}

#[test]
fn test_parse_create_accounts_error_insufficient() {
    let accounts: Vec<AccountInfo> = (0..5).map(make_test_account).collect();
    assert!(matches!(
        parse_create_accounts(&accounts),
        Err(ProgramError::NotEnoughAccountKeys)
    ));
}

#[test]
fn test_parse_recover_accounts_success() {
    let accounts: Vec<AccountInfo> = (30..37).map(|s| make_test_account(s as u8)).collect();
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
    let accounts: Vec<AccountInfo> = (0..6).map(make_test_account).collect();
    assert!(matches!(
        parse_recover_accounts(&accounts),
        Err(ProgramError::NotEnoughAccountKeys)
    ));
}
