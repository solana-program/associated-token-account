use {
    pinocchio::pubkey::Pubkey,
    pinocchio_pubkey::pubkey,
    spl_token_interface::state::{
        account::Account as TokenAccount, multisig::Multisig, Transmutable,
    },
};

use std::{vec, vec::Vec};

pub const SPL_TOKEN_PROGRAM_ID: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

/// Matches the pinocchio Account struct.
/// Account fields are private, so this struct allows more readable
/// use of them in tests.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AccountLayout {
    pub borrow_state: u8,
    pub is_signer: u8,
    pub is_writable: u8,
    pub executable: u8,
    pub resize_delta: i32,
    pub key: Pubkey,
    pub owner: Pubkey,
    pub lamports: u64,
    pub data_len: u64,
}

/// Create valid token account data for testing
pub fn create_token_account_data(mint: &Pubkey, owner: &Pubkey, amount: u64) -> Vec<u8> {
    let mut data = vec![0u8; TokenAccount::LEN];

    // Set mint (first 32 bytes)
    data[0..32].copy_from_slice(mint.as_ref());
    // Set owner (bytes 32-64)
    data[32..64].copy_from_slice(owner.as_ref());
    // Set amount (bytes 64-72)
    data[64..72].copy_from_slice(&amount.to_le_bytes());
    // Set state to initialized (byte 108)
    data[108] = 1;

    data
}

/// Create valid multisig data for testing
pub fn create_multisig_data(m: u8, n: u8, signers: &[Pubkey]) -> Vec<u8> {
    let mut data = vec![0u8; Multisig::LEN];
    data[0] = m;
    data[1] = n;
    data[2] = 1; // initialized

    // Add signers (starting at byte 12, each signer is 32 bytes)
    for (i, signer) in signers.iter().take(n as usize).enumerate() {
        let start = 12 + (i * 32);
        let end = start + 32;
        data[start..end].copy_from_slice(signer.as_ref());
    }

    data
}

/// Create rent sysvar data for testing
pub fn create_rent_data(
    lamports_per_byte_year: u64,
    exemption_threshold: f64,
    burn_percent: u8,
) -> Vec<u8> {
    // This is a simplified version - in real tests you'd use proper serialization
    let mut data = Vec::new();
    data.extend_from_slice(&lamports_per_byte_year.to_le_bytes());
    data.extend_from_slice(&exemption_threshold.to_le_bytes());
    data.push(burn_percent);
    data
}

/// Test helper to verify token account structure
pub fn validate_token_account_structure(
    data: &[u8],
    expected_mint: &Pubkey,
    expected_owner: &Pubkey,
) -> bool {
    if data.len() < TokenAccount::LEN {
        return false;
    }

    // Check mint
    if &data[0..32] != expected_mint.as_ref() {
        return false;
    }

    // Check owner
    if &data[32..64] != expected_owner.as_ref() {
        return false;
    }

    // Check initialized state
    data[108] != 0
}

#[cfg(test)]
mod tests {
    use crate::processor::is_spl_token_program;

    use super::*;

    #[test]
    fn test_validate_token_account_structure() {
        let mint = Pubkey::from([1u8; 32]);
        let owner = Pubkey::from([2u8; 32]);
        let data = create_token_account_data(&mint, &owner, 1000);

        assert!(validate_token_account_structure(&data, &mint, &owner));

        let wrong_mint = Pubkey::from([99u8; 32]);
        assert!(!validate_token_account_structure(
            &data,
            &wrong_mint,
            &owner
        ));
    }

    #[test]
    fn test_fn_is_spl_token_program() {
        assert!(is_spl_token_program(&SPL_TOKEN_PROGRAM_ID));

        let token_2022_id = pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
        assert!(!is_spl_token_program(&token_2022_id));
    }

    /* -- Tests of Test Helpers -- */

    #[test]
    fn test_create_multisig_data() {
        let signers = vec![
            Pubkey::from([1u8; 32]),
            Pubkey::from([2u8; 32]),
            Pubkey::from([3u8; 32]),
        ];

        let data = create_multisig_data(2, 3, &signers);

        assert_eq!(data.len(), Multisig::LEN);
        assert_eq!(data[0], 2); // m
        assert_eq!(data[1], 3); // n
        assert_eq!(data[2], 1); // initialized

        assert_eq!(&data[12..44], signers[0].as_ref());
    }

    /// Test that the token account data is created correctly
    /// in the test utility function
    #[test]
    fn test_token_account_data_creation() {
        let mint = Pubkey::from([1u8; 32]);
        let owner = Pubkey::from([2u8; 32]);
        let amount = 1000u64;

        let data = create_token_account_data(&mint, &owner, amount);

        // Verify the data structure
        assert_eq!(data.len(), TokenAccount::LEN);
        assert_eq!(&data[0..32], mint.as_ref());
        assert_eq!(&data[32..64], owner.as_ref());

        let stored_amount = u64::from_le_bytes(data[64..72].try_into().unwrap());
        assert_eq!(stored_amount, amount);

        // Verify initialized state
        assert_eq!(data[108], 1);
    }

    /// Test that the rent data is created correctly
    /// in the test utility function
    #[test]
    fn test_rent_data_creation() {
        let lamports_per_byte_year = 1000u64;
        let exemption_threshold = 2.0f64;
        let burn_percent = 50u8;

        let data = create_rent_data(lamports_per_byte_year, exemption_threshold, burn_percent);

        // Verify basic structure (simplified test)
        assert!(!data.is_empty());
        assert_eq!(data.len(), 8 + 8 + 1); // u64 + f64 + u8
    }

    #[test]
    fn test_account_layout_compatibility() {
        unsafe {
            let test_header = AccountLayout {
                borrow_state: 42,
                is_signer: 1,
                is_writable: 1,
                executable: 0,
                resize_delta: 100,
                key: [1u8; 32],
                owner: [2u8; 32],
                lamports: 1000,
                data_len: 256,
            };

            let account_ptr = &test_header as *const AccountLayout;
            let account_ref = &*account_ptr;
            assert_eq!(
                account_ref.borrow_state, 42,
                "borrow_state field should be accessible and match"
            );
            assert_eq!(
                account_ref.data_len, 256,
                "data_len field should be accessible and match"
            );
        }
    }
}
