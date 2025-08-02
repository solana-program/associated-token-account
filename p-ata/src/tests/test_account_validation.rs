#![cfg(test)]

use {
    crate::{
        processor::{
            valid_token_account_data, validate_token_account_mint, validate_token_account_owner,
        },
        tests::test_utils::{create_token_account_data, validate_token_account_structure},
    },
    pinocchio::{program_error::ProgramError, pubkey::Pubkey},
    spl_token_interface::state::{
        account::Account as TokenAccount, multisig::Multisig, Transmutable,
    },
};

use std::vec;

#[test]
fn test_valid_token_account_data() {
    // Case 1: Regular, initialized account
    let mut data1 = [0u8; TokenAccount::LEN];
    data1[108] = 1; // initialized state
    assert!(
        valid_token_account_data(&data1),
        "Regular initialized account should be valid"
    );

    // Case 2: Uninitialized account
    let mut data2 = [0u8; TokenAccount::LEN];
    data2[108] = 0; // uninitialized state
    assert!(
        !valid_token_account_data(&data2),
        "Uninitialized account should be invalid"
    );

    // Case 3: Data too short
    let data3 = [0u8; TokenAccount::LEN - 1];
    assert!(
        !valid_token_account_data(&data3),
        "Data shorter than TokenAccount::LEN should be invalid"
    );

    // Case 4: Extended, correctly typed account
    let mut data4 = vec![0u8; TokenAccount::LEN + 10];
    data4[108] = 1; // initialized
    data4[TokenAccount::LEN] = 2; // AccountType::Account
    assert!(
        valid_token_account_data(&data4),
        "Extended, correctly typed account should be valid"
    );

    // Case 5: Extended, incorrectly typed account
    let mut data5 = vec![0u8; TokenAccount::LEN + 10];
    data5[108] = 1; // initialized
    data5[TokenAccount::LEN] = 1; // Wrong account type
    assert!(
        !valid_token_account_data(&data5),
        "Extended, incorrectly typed account should be invalid"
    );

    // Case 6: Multisig collision
    let mut data6 = [0u8; Multisig::LEN];
    data6[0] = 2; // valid multisig m
    data6[1] = 3; // valid multisig n
    data6[2] = 1; // initialized
    data6[108] = 1;
    assert!(
        !valid_token_account_data(&data6),
        "Multisig data should be invalid"
    );
}

#[test]
fn test_validate_token_account_owner() {
    let owner1 = Pubkey::from([1u8; 32]);
    let owner2 = Pubkey::from([2u8; 32]);
    let mint = Pubkey::from([3u8; 32]);
    let data = create_token_account_data(&mint, &owner1, 1000);
    let account: &TokenAccount = unsafe { &*(data.as_ptr() as *const TokenAccount) };

    assert!(validate_token_account_owner(account, &owner1).is_ok());
    assert_eq!(
        validate_token_account_owner(account, &owner2).unwrap_err(),
        ProgramError::IllegalOwner
    );
}

#[test]
fn test_validate_token_account_mint() {
    let mint1 = Pubkey::from([1u8; 32]);
    let mint2 = Pubkey::from([2u8; 32]);
    let owner = Pubkey::from([3u8; 32]);
    let data = create_token_account_data(&mint1, &owner, 1000);
    let account: &TokenAccount = unsafe { &*(data.as_ptr() as *const TokenAccount) };

    assert!(validate_token_account_mint(account, &mint1).is_ok());
    assert_eq!(
        validate_token_account_mint(account, &mint2).unwrap_err(),
        ProgramError::InvalidAccountData
    );
}

#[test]
fn test_create_token_account_data_structure() {
    let mint = Pubkey::from([1u8; 32]);
    let owner = Pubkey::from([2u8; 32]);
    let amount = 1000u64;

    let data = create_token_account_data(&mint, &owner, amount);

    assert!(validate_token_account_structure(&data, &mint, &owner));
    assert!(valid_token_account_data(&data));
}
