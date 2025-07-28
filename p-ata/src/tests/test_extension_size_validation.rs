use {
    crate::processor::account_size_from_mint_inline,
    solana_program_option::COption,
    solana_pubkey::Pubkey,
    spl_token_2022::extension::{
        default_account_state::DefaultAccountState, group_pointer::GroupPointer,
        interest_bearing_mint::InterestBearingConfig, metadata_pointer::MetadataPointer,
        mint_close_authority::MintCloseAuthority, non_transferable::NonTransferable,
        pausable::PausableConfig, permanent_delegate::PermanentDelegate,
        transfer_fee::TransferFeeConfig, transfer_hook::TransferHook, BaseStateWithExtensionsMut,
        ExtensionType, PodStateWithExtensionsMut,
    },
    spl_token_2022::pod::PodMint,
    std::vec,
    std::vec::Vec,
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

/// Create mint data with specific extensions using token-2022's official methods
fn create_mint_data_with_extensions(extension_types: &[ExtensionType]) -> Vec<u8> {
    let required_size =
        ExtensionType::try_calculate_account_len::<spl_token_2022::state::Mint>(extension_types)
            .expect("Failed to calculate account length");

    let mut data = vec![0u8; required_size];

    let mut mint = PodStateWithExtensionsMut::<PodMint>::unpack_uninitialized(&mut data)
        .expect("Failed to unpack mint");

    mint.base.mint_authority = COption::None.try_into().unwrap();
    mint.base.supply = 0u64.into();
    mint.base.decimals = 6;
    mint.base.is_initialized = true.into();
    mint.base.freeze_authority = COption::None.try_into().unwrap();

    for extension_type in extension_types {
        match extension_type {
            ExtensionType::TransferFeeConfig => {
                let extension = mint
                    .init_extension::<TransferFeeConfig>(true)
                    .expect("Failed to init TransferFeeConfig");
                extension.transfer_fee_config_authority = COption::None.try_into().unwrap();
                extension.withdraw_withheld_authority = COption::None.try_into().unwrap();
                extension.withheld_amount = 0u64.into();
            }
            ExtensionType::DefaultAccountState => {
                let extension = mint
                    .init_extension::<DefaultAccountState>(true)
                    .expect("Failed to init DefaultAccountState");
                extension.state = spl_token_2022::state::AccountState::Initialized.into();
            }
            ExtensionType::InterestBearingConfig => {
                let extension = mint
                    .init_extension::<InterestBearingConfig>(true)
                    .expect("Failed to init InterestBearingConfig");
                extension.rate_authority = COption::None.try_into().unwrap();
                extension.initialization_timestamp = 0i64.into();
                extension.pre_update_average_rate = 0i16.into();
                extension.last_update_timestamp = 0i64.into();
                extension.current_rate = 0i16.into();
            }
            ExtensionType::MintCloseAuthority => {
                let extension = mint
                    .init_extension::<MintCloseAuthority>(true)
                    .expect("Failed to init MintCloseAuthority");
                extension.close_authority = COption::None.try_into().unwrap();
            }
            ExtensionType::PermanentDelegate => {
                let extension = mint
                    .init_extension::<PermanentDelegate>(true)
                    .expect("Failed to init PermanentDelegate");
                extension.delegate = COption::None.try_into().unwrap();
            }
            ExtensionType::TransferHook => {
                let extension = mint
                    .init_extension::<TransferHook>(true)
                    .expect("Failed to init TransferHook");
                extension.authority = COption::None.try_into().unwrap();
                extension.program_id = COption::None.try_into().unwrap();
            }
            ExtensionType::MetadataPointer => {
                let extension = mint
                    .init_extension::<MetadataPointer>(true)
                    .expect("Failed to init MetadataPointer");
                extension.authority = COption::None.try_into().unwrap();
                extension.metadata_address = COption::None.try_into().unwrap();
            }
            ExtensionType::GroupPointer => {
                let extension = mint
                    .init_extension::<GroupPointer>(true)
                    .expect("Failed to init GroupPointer");
                extension.authority = COption::None.try_into().unwrap();
                extension.group_address = COption::None.try_into().unwrap();
            }
            ExtensionType::Pausable => {
                let extension = mint
                    .init_extension::<PausableConfig>(true)
                    .expect("Failed to init PausableConfig");
                extension.authority = COption::Some(Pubkey::new_from_array([1; 32]))
                    .try_into()
                    .unwrap();
                extension.paused = false.into();
            }
            ExtensionType::NonTransferable => {
                let _extension = mint
                    .init_extension::<NonTransferable>(true)
                    .expect("Failed to init NonTransferable");
            }
            _ => {}
        }
    }

    data
}

/// Calculate expected account size using token-2022's official method
fn calculate_expected_account_size(mint_extensions: &[ExtensionType]) -> usize {
    let mut account_extensions =
        ExtensionType::get_required_init_account_extensions(mint_extensions);

    // ATA always includes ImmutableOwner, so include it in our comparison
    if !account_extensions.contains(&ExtensionType::ImmutableOwner) {
        account_extensions.push(ExtensionType::ImmutableOwner);
    }

    ExtensionType::try_calculate_account_len::<spl_token_2022::state::Account>(&account_extensions)
        .expect("Failed to calculate account length")
}

#[test]
fn test_no_extensions() {
    let mint_data = create_base_mint_data();
    let expected_size = calculate_expected_account_size(&[]);

    let result = account_size_from_mint_inline(&mint_data);
    assert_eq!(
        result,
        Some(expected_size),
        "No extensions should return base account size"
    );
}

#[test]
fn test_transfer_fee_config() {
    let extensions = vec![ExtensionType::TransferFeeConfig];
    let mint_data = create_mint_data_with_extensions(&extensions);
    let expected_size = calculate_expected_account_size(&extensions);

    let result = account_size_from_mint_inline(&mint_data);
    assert_eq!(
        result,
        Some(expected_size),
        "TransferFeeConfig should add TransferFeeAmount extension to account size"
    );
}

#[test]
fn test_transfer_hook() {
    let extensions = vec![ExtensionType::TransferHook];
    let mint_data = create_mint_data_with_extensions(&extensions);
    let expected_size = calculate_expected_account_size(&extensions);

    let result = account_size_from_mint_inline(&mint_data);
    assert_eq!(
        result,
        Some(expected_size),
        "TransferHook should add TransferHookAccount extension to account size"
    );
}

#[test]
fn test_pausable_config() {
    let extensions = vec![ExtensionType::Pausable];
    let mint_data = create_mint_data_with_extensions(&extensions);
    let expected_size = calculate_expected_account_size(&extensions);

    let result = account_size_from_mint_inline(&mint_data);
    assert_eq!(
        result,
        Some(expected_size),
        "PausableConfig should add PausableAccount extension to account size"
    );
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
        let expected_size = calculate_expected_account_size(&vec![extension]);

        let result = account_size_from_mint_inline(&mint_data);
        assert_eq!(
            result,
            Some(expected_size),
            "Extension {:?} should match official calculation",
            extension
        );

        let base_size_with_immutable_owner = calculate_expected_account_size(&[]);
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
    let expected_size = calculate_expected_account_size(&extensions);

    let result = account_size_from_mint_inline(&mint_data);
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
    let expected_size = calculate_expected_account_size(&extensions);

    let result = account_size_from_mint_inline(&mint_data);
    assert_eq!(
        result,
        Some(expected_size),
        "Mixed extensions should correctly calculate only account-side sizes"
    );
}

#[test]
fn test_unsupported_extensions_return_none() {
    // These extensions should cause our function to return None (fall back to CPI)
    let unsupported_extensions = vec![
        28, // token-2022 extensions end at 27
        29, 420,
    ];

    for extension in unsupported_extensions {
        let mut mint_data = create_base_mint_data();
        mint_data.resize(170, 0);

        mint_data[165] = 1; // AccountType::Mint

        let extension_type = extension as u16;
        mint_data[166..168].copy_from_slice(&extension_type.to_le_bytes());
        mint_data[168..170].copy_from_slice(&[0u8, 0u8]);

        let result = account_size_from_mint_inline(&mint_data);
        assert_eq!(
            result, None,
            "Unsupported extension {:?} should return None",
            extension
        );
    }
}

#[test]
fn test_non_transferable_extension() {
    let extensions = vec![ExtensionType::NonTransferable];
    let mint_data = create_mint_data_with_extensions(&extensions);
    let expected_size = calculate_expected_account_size(&extensions);

    let result = account_size_from_mint_inline(&mint_data);
    assert_eq!(
        result,
        Some(expected_size),
        "NonTransferable should be supported"
    );
}

#[test]
fn test_empty_extension_data() {
    let mut mint_data = create_base_mint_data();
    mint_data.extend_from_slice(&[0u8, 0u8, 0u8, 0u8]);

    let result = account_size_from_mint_inline(&mint_data);
    let expected_size = calculate_expected_account_size(&[]);
    assert_eq!(
        result,
        Some(expected_size),
        "Empty extension data should return base size"
    );
}

#[test]
fn test_extension_combinations_comprehensive() {
    // Extensions that require account-side data (should affect our calculation)
    let account_affecting_extensions = vec![
        ExtensionType::TransferFeeConfig, // +12 bytes (TLV + TransferFeeAmount)
        ExtensionType::NonTransferable,   // +4 bytes (TLV + NonTransferableAccount)
        ExtensionType::TransferHook,      // +5 bytes (TLV + TransferHookAccount)
        ExtensionType::Pausable,          // +4 bytes (TLV + PausableAccount)
    ];

    // Extensions that don't require account-side data (should not affect our calculation)
    let mint_only_extensions = vec![
        ExtensionType::DefaultAccountState,
        ExtensionType::InterestBearingConfig,
        ExtensionType::MetadataPointer,
        ExtensionType::GroupPointer,
        ExtensionType::GroupMemberPointer,
        ExtensionType::TransferFeeConfig, // affects both mint and account
    ];

    for (i, ext1) in account_affecting_extensions.iter().enumerate() {
        for ext2 in account_affecting_extensions.iter().skip(i + 1) {
            let extensions = vec![*ext1, *ext2];
            test_extension_combination(&extensions, "Two account-affecting extensions");
        }
    }

    for account_ext in &account_affecting_extensions {
        for mint_ext in &mint_only_extensions {
            if account_ext != mint_ext {
                let extensions = vec![*account_ext, *mint_ext];
                test_extension_combination(&extensions, "Account-affecting + mint-only extension");
            }
        }
    }

    let three_ext_combinations = vec![
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
            ExtensionType::NonTransferable,
            ExtensionType::TransferHook,
            ExtensionType::Pausable,
        ],
        vec![
            ExtensionType::TransferFeeConfig,
            ExtensionType::InterestBearingConfig,
            ExtensionType::MetadataPointer,
        ],
    ];

    for extensions in three_ext_combinations {
        test_extension_combination(&extensions, "Three extension combination");
    }

    let large_combinations = vec![
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

    for extensions in large_combinations {
        test_extension_combination(&extensions, "Large extension combination");
    }
}

#[test]
fn test_token_metadata_variable_length() {
    // Create a simple mint data with manually constructed TokenMetadata TLV
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

    #[cfg(feature = "test-debug")]
    eprintln!(
        "Created mock mint data with TokenMetadata TLV, length: {}",
        mint_data.len()
    );

    // Test our inline parser
    let inline_size = account_size_from_mint_inline(&mint_data);

    #[cfg(feature = "test-debug")]
    eprintln!("Inline parser result: {:?}", inline_size);

    // TokenMetadata is mint-only, so account size should be base + ImmutableOwner
    let expected_account_size = calculate_expected_account_size(&[ExtensionType::ImmutableOwner]);

    assert_eq!(
        inline_size,
        Some(expected_account_size),
        "TokenMetadata should be supported inline as a mint-only extension"
    );

    #[cfg(feature = "test-debug")]
    eprintln!("TokenMetadata test passed!");
}

fn test_extension_combination(extensions: &[ExtensionType], description: &str) {
    let mint_data = create_mint_data_with_extensions(extensions);
    let expected_size = calculate_expected_account_size(extensions);
    let result = account_size_from_mint_inline(&mint_data);

    assert_eq!(
        result,
        Some(expected_size),
        "Extension combination failed: {}. Extensions: {:?}",
        description,
        extensions
    );
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
        let result = account_size_from_mint_inline(&mint_data);

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
                let expected_size = calculate_expected_account_size(&extensions);
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
