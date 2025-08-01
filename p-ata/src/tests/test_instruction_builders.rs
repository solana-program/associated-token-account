#![cfg(test)]

use {
    crate::{
        processor::{
            build_initialize_account3_data, build_transfer_checked_data,
            INITIALIZE_ACCOUNT_3_DISCM, INITIALIZE_IMMUTABLE_OWNER_DISCM, TRANSFER_CHECKED_DISCM,
        },
        recover::CLOSE_ACCOUNT_DISCM,
    },
    pinocchio::pubkey::Pubkey,
    rstest::rstest,
    std::collections::HashSet,
    test_case::test_case,
};

#[rstest]
#[case([1u8; 32])] // owner_1
#[case([2u8; 32])] // owner_2
#[case([42u8; 32])] // owner_42
#[case([255u8; 32])] // owner_255
fn test_build_initialize_account3_data(#[case] owner_bytes: [u8; 32]) {
    let owner = Pubkey::from(owner_bytes);
    let data = build_initialize_account3_data(&owner);

    assert_eq!(data.len(), 33);
    assert_eq!(data[0], INITIALIZE_ACCOUNT_3_DISCM);
    assert_eq!(&data[1..33], owner.as_ref());
}

#[test]
fn test_build_initialize_account3_data_different_owners() {
    let owner1 = Pubkey::from([1u8; 32]);
    let owner2 = Pubkey::from([2u8; 32]);

    let data1 = build_initialize_account3_data(&owner1);
    let data2 = build_initialize_account3_data(&owner2);

    assert_eq!(data1[0], data2[0]); // Same discriminator
    assert_ne!(&data1[1..], &data2[1..]); // Different owner bytes
}

#[test_case(0, 0; "zero_amount_zero_decimals")]
#[test_case(1000, 6; "typical_amount")]
#[test_case(u64::MAX, 18; "max_amount_max_decimals")]
#[test_case(u64::MAX, u8::MAX; "max_amount_max_decimals_u8")]
#[test_case(123456789, 9; "random_values")]
fn test_build_transfer_data(amount: u64, decimals: u8) {
    let data = build_transfer_checked_data(amount, decimals);

    assert_eq!(data.len(), 10);
    assert_eq!(data[0], TRANSFER_CHECKED_DISCM);

    let parsed_amount = u64::from_le_bytes([
        data[1], data[2], data[3], data[4], data[5], data[6], data[7], data[8],
    ]);
    assert_eq!(parsed_amount, amount);
    assert_eq!(data[9], decimals);
}

#[rstest]
#[case(0x0123456789abcdef_u64, 6)] // big_endian_value
#[case(0x1234_u64, 9)] // small_value
#[case(u64::MAX, 18)] // max_value
fn test_build_transfer_data_endianness(#[case] amount: u64, #[case] decimals: u8) {
    let data = build_transfer_checked_data(amount, decimals);

    // Verify little-endian encoding
    let expected_bytes = amount.to_le_bytes();
    assert_eq!(&data[1..9], &expected_bytes);
}

#[rstest]
#[case([42u8; 32])] // owner_test
#[case([0u8; 32])] // owner_zeros
#[case([1u8; 32])] // owner_ones
fn test_instruction_data_deterministic_owner(#[case] owner_bytes: [u8; 32]) {
    let owner = Pubkey::from(owner_bytes);

    let data1 = build_initialize_account3_data(&owner);
    let data2 = build_initialize_account3_data(&owner);
    assert_eq!(data1, data2);
}

#[rstest]
#[case(1000, 6)] // transfer_1
#[case(500, 9)] // transfer_2
#[case(u64::MAX, 18)] // transfer_3
fn test_instruction_data_deterministic_transfer(#[case] amount: u64, #[case] decimals: u8) {
    let transfer1 = build_transfer_checked_data(amount, decimals);
    let transfer2 = build_transfer_checked_data(amount, decimals);
    assert_eq!(transfer1, transfer2);
}

#[test]
fn test_discriminator_uniqueness() {
    let discriminators = [
        INITIALIZE_ACCOUNT_3_DISCM,
        INITIALIZE_IMMUTABLE_OWNER_DISCM,
        TRANSFER_CHECKED_DISCM,
        CLOSE_ACCOUNT_DISCM,
    ];

    let mut unique_discriminators = HashSet::new();
    for &d in &discriminators {
        unique_discriminators.insert(d);
    }

    assert_eq!(
        discriminators.len(),
        unique_discriminators.len(),
        "All discriminators must be unique"
    );
}
