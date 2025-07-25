use {
    crate::{
        processor::{
            valid_token_account_data, validate_token_account_mint, validate_token_account_owner,
        },
        tests::test_utils::{
            create_multisig_data, create_token_account_data, validate_token_account_structure,
        },
    },
    pinocchio::{program_error::ProgramError, pubkey::Pubkey},
    spl_token_interface::state::{
        account::Account as TokenAccount, multisig::Multisig, Transmutable,
    },
    test_case::test_case,
};

use std::vec;

#[test]
fn test_valid_token_account_data_regular_account() {
    let mut data = [0u8; TokenAccount::LEN];
    data[108] = 1; // initialized state

    assert!(valid_token_account_data(&data));
}

#[test]
fn test_valid_token_account_data_uninitialized() {
    let mut data = [0u8; TokenAccount::LEN];
    data[108] = 0; // uninitialized state

    assert!(!valid_token_account_data(&data));
}

#[test]
fn test_valid_token_account_data_too_short() {
    let data = [0u8; TokenAccount::LEN - 1];
    assert!(!valid_token_account_data(&data));
}

#[test]
fn test_valid_token_account_data_extended_account() {
    let mut data = vec![0u8; TokenAccount::LEN + 10];
    data[108] = 1; // initialized
    data[TokenAccount::LEN] = 2; // ACCOUNTTYPE_ACCOUNT

    assert!(valid_token_account_data(&data));
}

#[test]
fn test_valid_token_account_data_extended_account_wrong_type() {
    let mut data = vec![0u8; TokenAccount::LEN + 10];
    data[108] = 1; // initialized
    data[TokenAccount::LEN] = 1; // wrong account type

    assert!(!valid_token_account_data(&data));
}

#[test]
fn test_valid_token_account_data_multisig_collision() {
    let mut data = [0u8; Multisig::LEN];
    data[0] = 2; // valid multisig m
    data[1] = 3; // valid multisig n
    data[2] = 1; // initialized
    data[108] = 1;

    assert!(!valid_token_account_data(&data));
}

#[test]
fn test_validate_token_account_owner_valid() {
    let owner = Pubkey::from([1u8; 32]);
    let mint = Pubkey::from([2u8; 32]);
    let data = create_token_account_data(&mint, &owner, 1000);

    let account: &TokenAccount = unsafe { &*(data.as_ptr() as *const TokenAccount) };
    assert!(validate_token_account_owner(account, &owner).is_ok());
}

#[test]
fn test_validate_token_account_owner_invalid() {
    let owner1 = Pubkey::from([1u8; 32]);
    let owner2 = Pubkey::from([2u8; 32]);
    let mint = Pubkey::from([3u8; 32]);
    let data = create_token_account_data(&mint, &owner1, 1000);

    let account: &TokenAccount = unsafe { &*(data.as_ptr() as *const TokenAccount) };
    assert_eq!(
        validate_token_account_owner(account, &owner2).unwrap_err(),
        ProgramError::IllegalOwner
    );
}

#[test]
fn test_validate_token_account_mint_valid() {
    let mint = Pubkey::from([1u8; 32]);
    let owner = Pubkey::from([2u8; 32]);
    let data = create_token_account_data(&mint, &owner, 1000);

    let account: &TokenAccount = unsafe { &*(data.as_ptr() as *const TokenAccount) };
    assert!(validate_token_account_mint(account, &mint).is_ok());
}

#[test]
fn test_validate_token_account_mint_invalid() {
    let mint1 = Pubkey::from([1u8; 32]);
    let mint2 = Pubkey::from([2u8; 32]);
    let owner = Pubkey::from([3u8; 32]);
    let data = create_token_account_data(&mint1, &owner, 1000);

    let account: &TokenAccount = unsafe { &*(data.as_ptr() as *const TokenAccount) };
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
