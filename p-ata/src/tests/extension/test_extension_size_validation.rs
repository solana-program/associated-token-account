use {
    crate::size::calculate_account_size_from_mint_extensions,
    spl_token_2022::extension::ExtensionType, std::vec, std::vec::Vec,
};

#[cfg(feature = "test-debug")]
use std::eprintln;

/// Create a basic mint with no extensions for testing
fn create_base_mint_data() -> Vec<u8> {
    const MINT_BASE_SIZE: usize = 82;
    let mut data = vec![0u8; MINT_BASE_SIZE + 5];

    data[0..4].copy_from_slice(&1u32.to_le_bytes());
    data[MINT_BASE_SIZE] = 1;

    data
}

use crate::tests::extension::test_extension_utils::{
    calculate_expected_ata_data_size, create_mint_data_with_extensions,
};

#[test]
fn test_no_extensions() {
    let mint_data = create_base_mint_data();
    let expected_size = calculate_expected_ata_data_size(&[]);

    let result = calculate_account_size_from_mint_extensions(&mint_data);
    assert_eq!(
        result,
        Some(expected_size),
        "No extensions should return base account size"
    );
}

#[test]
fn test_single_extensions() {
    let extensions_to_test = vec![
        (
            ExtensionType::TransferFeeConfig,
            "TransferFeeConfig should add TransferFeeAmount extension to account size",
        ),
        (
            ExtensionType::TransferHook,
            "TransferHook should add TransferHookAccount extension to account size",
        ),
        (
            ExtensionType::Pausable,
            "PausableConfig should add PausableAccount extension to account size",
        ),
        (
            ExtensionType::NonTransferable,
            "NonTransferable should be supported",
        ),
    ];

    for (extension, description) in extensions_to_test {
        let extensions = vec![extension];
        let mint_data = create_mint_data_with_extensions(&extensions);
        let expected_size = calculate_expected_ata_data_size(&extensions);

        let result = calculate_account_size_from_mint_extensions(&mint_data);
        assert_eq!(result, Some(expected_size), "{}", description);
    }
}

#[test]
fn test_extensions_without_account_data() {
    let extensions = vec![
        ExtensionType::DefaultAccountState,
        ExtensionType::InterestBearingConfig,
        ExtensionType::MintCloseAuthority,
        ExtensionType::PermanentDelegate,
        ExtensionType::MetadataPointer,
        ExtensionType::GroupPointer,
    ];

    for extension in extensions {
        let mint_data = create_mint_data_with_extensions(&vec![extension]);
        let expected_size = calculate_expected_ata_data_size(&vec![extension]);

        let result = calculate_account_size_from_mint_extensions(&mint_data);
        assert_eq!(
            result,
            Some(expected_size),
            "Extension {:?} should match official calculation",
            extension
        );

        let base_size_with_immutable_owner = calculate_expected_ata_data_size(&[]);
        assert_eq!(
            expected_size, base_size_with_immutable_owner,
            "Extension {:?} should not add to account size",
            extension
        );
    }
}

#[test]
fn test_multiple_extensions_with_account_data() {
    let extensions = vec![
        ExtensionType::TransferFeeConfig,
        ExtensionType::TransferHook,
        ExtensionType::Pausable,
    ];

    let mint_data = create_mint_data_with_extensions(&extensions);
    let expected_size = calculate_expected_ata_data_size(&extensions);

    let result = calculate_account_size_from_mint_extensions(&mint_data);
    assert_eq!(
        result,
        Some(expected_size),
        "Multiple extensions should correctly sum account sizes"
    );
}

#[test]
fn test_mixed_extensions() {
    let extensions = vec![
        ExtensionType::TransferFeeConfig,
        ExtensionType::DefaultAccountState,
        ExtensionType::TransferHook,
        ExtensionType::InterestBearingConfig,
        ExtensionType::MetadataPointer,
    ];

    let mint_data = create_mint_data_with_extensions(&extensions);
    let expected_size = calculate_expected_ata_data_size(&extensions);

    let result = calculate_account_size_from_mint_extensions(&mint_data);
    assert_eq!(
        result,
        Some(expected_size),
        "Mixed extensions should correctly calculate only account-side sizes"
    );
}

fn create_mint_with_unsupported_extension(extension_type: u16) -> Vec<u8> {
    let mut mint_data = create_base_mint_data();
    mint_data.resize(170, 0);
    mint_data[165] = 1; // AccountType::Mint
    mint_data[166..168].copy_from_slice(&extension_type.to_le_bytes());
    mint_data[168..170].copy_from_slice(&[0u8, 0u8]);
    mint_data
}

#[test]
fn test_unsupported_extensions_return_none() {
    // These extensions should cause our function to return None (fall back to CPI)
    let unsupported_extensions = vec![
        29, // token-2022 extensions end at 27, plus we support the upcoming 28
        30, 420,
    ];

    for &extension_type in &unsupported_extensions {
        let mint_data = create_mint_with_unsupported_extension(extension_type);
        let result = calculate_account_size_from_mint_extensions(&mint_data);
        assert_eq!(
            result, None,
            "Unsupported extension {:?} should return None",
            extension_type
        );
    }
}

#[test]
fn test_empty_extension_data() {
    let mut mint_data = create_base_mint_data();
    mint_data.extend_from_slice(&[0u8, 0u8, 0u8, 0u8]);

    let result = calculate_account_size_from_mint_extensions(&mint_data);
    let expected_size = calculate_expected_ata_data_size(&[]);
    assert_eq!(
        result,
        Some(expected_size),
        "Empty extension data should return base size"
    );
}

#[test]
fn test_extension_combinations_comprehensive() {
    let account_affecting_extensions = [
        ExtensionType::TransferFeeConfig,
        ExtensionType::NonTransferable,
        ExtensionType::TransferHook,
        ExtensionType::Pausable,
    ];

    let mint_only_extensions = [
        ExtensionType::DefaultAccountState,
        ExtensionType::InterestBearingConfig,
        ExtensionType::MetadataPointer,
        ExtensionType::GroupPointer,
        ExtensionType::GroupMemberPointer,
    ];

    // Test combinations of two account-affecting extensions
    for i in 0..account_affecting_extensions.len() {
        for j in i + 1..account_affecting_extensions.len() {
            let extensions = vec![
                account_affecting_extensions[i],
                account_affecting_extensions[j],
            ];
            test_extension_combination(&extensions, "Two account-affecting extensions");
        }
    }

    // Test combinations of one account-affecting and one mint-only extension
    for &account_ext in &account_affecting_extensions {
        for &mint_ext in &mint_only_extensions {
            let extensions = vec![account_ext, mint_ext];
            test_extension_combination(&extensions, "Account-affecting + mint-only extension");
        }
    }

    // Test a few larger combinations
    let large_combinations = [
        vec![
            ExtensionType::TransferFeeConfig,
            ExtensionType::NonTransferable,
            ExtensionType::TransferHook,
        ],
        vec![
            ExtensionType::TransferFeeConfig,
            ExtensionType::Pausable,
            ExtensionType::DefaultAccountState,
        ],
        vec![
            ExtensionType::TransferFeeConfig,
            ExtensionType::NonTransferable,
            ExtensionType::TransferHook,
            ExtensionType::Pausable,
        ],
        vec![
            ExtensionType::TransferFeeConfig,
            ExtensionType::NonTransferable,
            ExtensionType::DefaultAccountState,
            ExtensionType::InterestBearingConfig,
            ExtensionType::MetadataPointer,
        ],
    ];

    for extensions in &large_combinations {
        test_extension_combination(extensions, "Large extension combination");
    }
}

fn create_mint_with_mock_token_metadata() -> Vec<u8> {
    let mut mint_data = std::vec![0u8; 82 + 32]; // Base mint (82) + some extension space

    // Set up basic mint structure
    mint_data[0..4].copy_from_slice(&[0, 0, 0, 0]); // mint_authority (None)
    mint_data[4..12].copy_from_slice(&[0; 8]); // supply
    mint_data[12] = 6; // decimals
    mint_data[13] = 1; // is_initialized = true
    mint_data[14..18].copy_from_slice(&[0, 0, 0, 0]); // freeze_authority (None)

    // Add account type at the end (for extensions)
    let account_type_offset = 82;
    mint_data[account_type_offset] = 1; // AccountType::Mint

    // Add a mock TokenMetadata TLV entry
    let tlv_offset = account_type_offset + 1;
    mint_data[tlv_offset..tlv_offset + 2].copy_from_slice(&[19u8, 0]); // Type: TokenMetadata (19)
    mint_data[tlv_offset + 2..tlv_offset + 4].copy_from_slice(&[20u8, 0]); // Length: 20 bytes
                                                                           // Mock 20 bytes of metadata content
    mint_data[tlv_offset + 4..tlv_offset + 24].copy_from_slice(&[1u8; 20]);
    mint_data
}

#[test]
fn test_token_metadata_variable_length() {
    // Create a simple mint data with manually constructed TokenMetadata TLV
    let mint_data = create_mint_with_mock_token_metadata();

    #[cfg(feature = "test-debug")]
    eprintln!(
        "Created mock mint data with TokenMetadata TLV, length: {}",
        mint_data.len()
    );

    // Test our inline parser
    let inline_size = calculate_account_size_from_mint_extensions(&mint_data);

    #[cfg(feature = "test-debug")]
    eprintln!("Inline parser result: {:?}", inline_size);

    // TokenMetadata is mint-only, so account size should be base + ImmutableOwner
    let expected_account_size = calculate_expected_ata_data_size(&[ExtensionType::ImmutableOwner]);

    assert_eq!(
        inline_size,
        Some(expected_account_size),
        "TokenMetadata should be supported inline as a mint-only extension"
    );

    #[cfg(feature = "test-debug")]
    eprintln!("TokenMetadata test passed!");
}

fn test_extension_combination(extensions: &[ExtensionType], description: &str) {
    crate::tests::extension::test_extension_utils::test_extension_combination_helper(
        extensions,
        description,
    )
    .unwrap();
}

#[test]
fn test_systematic_extension_verification() {
    let supported_extensions = vec![
        ExtensionType::TransferFeeConfig,
        ExtensionType::NonTransferable,
        ExtensionType::TransferHook,
        ExtensionType::Pausable,
        ExtensionType::DefaultAccountState,
        ExtensionType::InterestBearingConfig,
        ExtensionType::MetadataPointer,
        ExtensionType::GroupPointer,
        ExtensionType::GroupMemberPointer,
        ExtensionType::MintCloseAuthority,
        ExtensionType::TransferFeeAmount,
        ExtensionType::ImmutableOwner,
    ];

    for extension in &supported_extensions {
        match extension {
            ExtensionType::TransferFeeAmount | ExtensionType::ImmutableOwner => continue,
            _ => {}
        }

        let extensions = vec![*extension];
        let mint_data = create_mint_data_with_extensions(&extensions);
        let result = calculate_account_size_from_mint_extensions(&mint_data);

        match extension {
            ExtensionType::TransferFeeConfig
            | ExtensionType::NonTransferable
            | ExtensionType::TransferHook
            | ExtensionType::Pausable
            | ExtensionType::DefaultAccountState
            | ExtensionType::InterestBearingConfig
            | ExtensionType::MetadataPointer
            | ExtensionType::GroupPointer
            | ExtensionType::GroupMemberPointer
            | ExtensionType::MintCloseAuthority => {
                let expected_size = calculate_expected_ata_data_size(&extensions);
                assert_eq!(
                    result,
                    Some(expected_size),
                    "Extension {:?} should be supported but calculation differs. Expected: {}, Got: {:?}",
                    extension,
                    expected_size,
                    result
                );
            }
            _ => {}
        }
    }
}
