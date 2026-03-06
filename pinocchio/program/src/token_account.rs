//! Temporary `no_std` compatibility shim for Token-2022 account parsing.
//!
//! This file exists because `StateWithExtensions::<Account>::unpack(data)` from
//! `spl-token-2022-interface` is not currently `no_std` friendly. At the moment,
//! the blocking chain appears to run through `spl-pod`,
//! `spl-token-confidential-transfer-proof-extraction`, and `spl-token-group-interface`,
//! which in turn depend on `solana-pubkey`.
//!
//! The logic here mirrors the Token-2022 account validation rules:
//! - require an initialized base token account when length is exactly 165 bytes
//! - reject multisig-sized accounts
//! - require the Token-2022 account-type marker for extended accounts

use core::mem::size_of;

use pinocchio::Address;

// https://github.com/solana-program/token-2022/blob/8d6664f99d5016b0f4da7aca9a0fa59c5e72518c/interface/src/state.rs#L146-L151
const TOKEN_ACCOUNT_LEN: usize = 165;

// https://github.com/solana-program/token-2022/blob/8d6664f99d5016b0f4da7aca9a0fa59c5e72518c/interface/src/state.rs#L235-L238
const TOKEN_ACCOUNT_MULTISIG_LEN: usize = 355;

// https://github.com/solana-program/token-2022/blob/8d6664f99d5016b0f4da7aca9a0fa59c5e72518c/interface/src/generic_token_account.rs#L8-L9
const TOKEN_ACCOUNT_MINT_OFFSET: usize = 0;
const TOKEN_ACCOUNT_OWNER_OFFSET: usize = TOKEN_ACCOUNT_MINT_OFFSET + size_of::<Address>();
const TOKEN_ACCOUNT_MINT_END: usize = TOKEN_ACCOUNT_OWNER_OFFSET;
const TOKEN_ACCOUNT_OWNER_END: usize = TOKEN_ACCOUNT_OWNER_OFFSET + size_of::<Address>();

// https://github.com/solana-program/token-2022/blob/8d6664f99d5016b0f4da7aca9a0fa59c5e72518c/interface/src/generic_token_account.rs#L55-L64
const TOKEN_ACCOUNT_INITIALIZED_STATE_OFFSET: usize = 108;

// The account-type marker is immediately after the 165-byte base account layout for extended accounts:
// https://github.com/solana-program/token-2022/blob/8d6664f99d5016b0f4da7aca9a0fa59c5e72518c/interface/src/state.rs#L318-L324
const TOKEN_ACCOUNT_TYPE_OFFSET: usize = TOKEN_ACCOUNT_LEN;

// https://github.com/solana-program/token-2022/blob/8d6664f99d5016b0f4da7aca9a0fa59c5e72518c/interface/src/state.rs#L315-L316
// https://github.com/solana-program/token-2022/blob/8d6664f99d5016b0f4da7aca9a0fa59c5e72518c/interface/src/extension/mod.rs#L1038-L1045
const TOKEN_2022_ACCOUNT_TYPE_ACCOUNT: u8 = 2;

#[inline(always)]
fn is_initialized_token_account(data: &[u8]) -> bool {
    data[TOKEN_ACCOUNT_INITIALIZED_STATE_OFFSET] != 0
}

// Copied from Token-2022's `Account::valid_account_data`
// https://github.com/solana-program/token-2022/blob/8d6664f99d5016b0f4da7aca9a0fa59c5e72518c/interface/src/state.rs#L318-L324
#[inline(always)]
fn valid_token_account_data(data: &[u8]) -> bool {
    (data.len() == TOKEN_ACCOUNT_LEN && is_initialized_token_account(data))
        || (data.len() > TOKEN_ACCOUNT_LEN
            && data.len() != TOKEN_ACCOUNT_MULTISIG_LEN
            && data[TOKEN_ACCOUNT_TYPE_OFFSET] == TOKEN_2022_ACCOUNT_TYPE_ACCOUNT
            && is_initialized_token_account(data))
}

/// Parse the mint and owner from token-account data that matches the Token-2022 rules.
#[inline(always)]
pub(crate) fn parse_token_account_mint_and_owner(data: &[u8]) -> Option<(Address, Address)> {
    if !valid_token_account_data(data) {
        return None;
    }

    let mint = Address::new_from_array(
        data[TOKEN_ACCOUNT_MINT_OFFSET..TOKEN_ACCOUNT_MINT_END]
            .try_into()
            .ok()?,
    );
    let owner = Address::new_from_array(
        data[TOKEN_ACCOUNT_OWNER_OFFSET..TOKEN_ACCOUNT_OWNER_END]
            .try_into()
            .ok()?,
    );

    Some((mint, owner))
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_program_pack::Pack;
    use spl_token_2022_interface::{
        extension::AccountType,
        generic_token_account::ACCOUNT_INITIALIZED_INDEX,
        state::{Account, Multisig},
    };

    #[test]
    fn token_account_layout_constants_match_token_2022_interface() {
        assert_eq!(TOKEN_ACCOUNT_LEN, Account::LEN);
        assert_eq!(TOKEN_ACCOUNT_MULTISIG_LEN, Multisig::LEN);
        assert_eq!(TOKEN_ACCOUNT_MINT_OFFSET, 0);
        assert_eq!(TOKEN_ACCOUNT_OWNER_OFFSET, size_of::<pinocchio::Address>());
        assert_eq!(
            TOKEN_ACCOUNT_INITIALIZED_STATE_OFFSET,
            ACCOUNT_INITIALIZED_INDEX
        );
        assert_eq!(TOKEN_ACCOUNT_TYPE_OFFSET, Account::LEN);
        assert_eq!(TOKEN_2022_ACCOUNT_TYPE_ACCOUNT, AccountType::Account as u8);
    }

    #[test]
    fn valid_token_account_data_accepts_initialized_token_account() {
        let mut data = [0u8; TOKEN_ACCOUNT_LEN];
        data[TOKEN_ACCOUNT_INITIALIZED_STATE_OFFSET] = 1;

        assert!(valid_token_account_data(&data));
    }

    #[test]
    fn valid_token_account_data_rejects_uninitialized_token_account() {
        let data = [0u8; TOKEN_ACCOUNT_LEN];

        assert!(!valid_token_account_data(&data));
    }

    #[test]
    fn valid_token_account_data_accepts_initialized_extended_token_account() {
        let mut data = [0u8; TOKEN_ACCOUNT_LEN + 1];
        data[TOKEN_ACCOUNT_INITIALIZED_STATE_OFFSET] = 1;
        data[TOKEN_ACCOUNT_LEN] = TOKEN_2022_ACCOUNT_TYPE_ACCOUNT;

        assert!(valid_token_account_data(&data));
    }

    #[test]
    fn valid_token_account_data_rejects_extended_account_with_wrong_type_marker() {
        let mut data = [0u8; TOKEN_ACCOUNT_LEN + 1];
        data[TOKEN_ACCOUNT_INITIALIZED_STATE_OFFSET] = 1;
        data[TOKEN_ACCOUNT_LEN] = 0;

        assert!(!valid_token_account_data(&data));
    }

    #[test]
    fn valid_token_account_data_rejects_multisig_sized_account() {
        let mut data = [0u8; TOKEN_ACCOUNT_MULTISIG_LEN];
        data[TOKEN_ACCOUNT_INITIALIZED_STATE_OFFSET] = 1;
        data[TOKEN_ACCOUNT_LEN] = TOKEN_2022_ACCOUNT_TYPE_ACCOUNT;

        assert!(!valid_token_account_data(&data));
    }
}
