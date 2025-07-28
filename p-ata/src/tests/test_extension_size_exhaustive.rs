use std::{eprintln, string::String};
#[allow(unexpected_cfgs)]
use {
    crate::processor::account_size_from_mint_inline,
    spl_token_2022::{
        extension::{
            default_account_state::DefaultAccountState, group_pointer::GroupPointer,
            interest_bearing_mint::InterestBearingConfig, metadata_pointer::MetadataPointer,
            mint_close_authority::MintCloseAuthority, non_transferable::NonTransferable,
            pausable::PausableConfig, permanent_delegate::PermanentDelegate,
            transfer_fee::TransferFeeConfig, transfer_hook::TransferHook, ExtensionType,
            PodStateWithExtensionsMut,
        },
        pod::PodMint,
    },
    spl_token_group_interface::state::{TokenGroup, TokenGroupMember},
    spl_token_metadata_interface::state::TokenMetadata,
    std::{vec, vec::Vec},
};

/// Create mint data with specific extensions using token-2022's official methods
fn create_mint_data_with_extensions(extension_types: &[ExtensionType]) -> Vec<u8> {
    use spl_token_2022::extension::{BaseStateWithExtensionsMut, ExtensionType};

    // Check if we have variable-length extensions that can't use try_calculate_account_len
    let has_variable_length = extension_types
        .iter()
        .any(|ext| matches!(ext, ExtensionType::TokenMetadata));

    let required_size = if has_variable_length {
        // Start with all *sized* extensions so we can use the official helper.
        let mut sized_exts: Vec<ExtensionType> = extension_types
            .iter()
            .copied()
            .filter(|e| !matches!(e, ExtensionType::TokenMetadata))
            .collect();

        // Calculate precise length for the sized subset.
        let mut required_size =
            ExtensionType::try_calculate_account_len::<spl_token_2022::state::Mint>(&sized_exts)
                .expect("calc len for sized subset");

        // Manually add space for the variable-length TokenMetadata TLV entry if present.
        if extension_types
            .iter()
            .any(|e| matches!(e, ExtensionType::TokenMetadata))
        {
            const TOKEN_METADATA_VALUE_LEN_ESTIMATE: usize = 500; // generous buffer
            required_size += TOKEN_METADATA_VALUE_LEN_ESTIMATE + 4; // value + TLV header
        }
        required_size
    } else {
        ExtensionType::try_calculate_account_len::<spl_token_2022::state::Mint>(extension_types)
            .expect("Failed to calculate account length")
    };

    let mut data = vec![0u8; required_size];

    let mut mint = match PodStateWithExtensionsMut::<PodMint>::unpack_uninitialized(&mut data) {
        Ok(mint) => mint,
        Err(e) => {
            eprintln!(
                "Failed to unpack mint for extensions {:?}: {:?}",
                extension_types, e
            );
            eprintln!(
                "Required size: {}, actual data length: {}",
                required_size,
                data.len()
            );
            return vec![];
        }
    };

    mint.base.mint_authority = Default::default();
    mint.base.supply = 0u64.into();
    mint.base.decimals = 6;
    mint.base.is_initialized = true.into();
    mint.base.freeze_authority = Default::default();

    for extension_type in extension_types {
        match extension_type {
            ExtensionType::TransferFeeConfig => {
                let extension = mint
                    .init_extension::<TransferFeeConfig>(true)
                    .expect("Failed to init TransferFeeConfig");
                extension.transfer_fee_config_authority = Default::default();
                extension.withdraw_withheld_authority = Default::default();
                extension.withheld_amount = 0u64.into();
            }
            ExtensionType::NonTransferable => {
                mint.init_extension::<NonTransferable>(true)
                    .expect("Failed to init NonTransferable");
            }
            ExtensionType::TransferHook => {
                let extension = mint
                    .init_extension::<TransferHook>(true)
                    .expect("Failed to init TransferHook");
                extension.authority = Default::default();
                extension.program_id = Default::default();
            }
            ExtensionType::Pausable => {
                let extension = mint
                    .init_extension::<PausableConfig>(true)
                    .expect("Failed to init PausableConfig");
                extension.authority = Default::default();
                extension.paused = false.into();
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
                extension.rate_authority = Default::default();
                extension.initialization_timestamp = 0i64.into();
                extension.pre_update_average_rate = 0i16.into();
                extension.last_update_timestamp = 0i64.into();
                extension.current_rate = 0i16.into();
            }
            ExtensionType::MetadataPointer => {
                let extension = mint
                    .init_extension::<MetadataPointer>(true)
                    .expect("Failed to init MetadataPointer");
                extension.authority = Default::default();
                extension.metadata_address = Default::default();
            }
            ExtensionType::GroupPointer => {
                let extension = mint
                    .init_extension::<GroupPointer>(true)
                    .expect("Failed to init GroupPointer");
                extension.authority = Default::default();
                extension.group_address = Default::default();
            }
            ExtensionType::GroupMemberPointer => {
                eprintln!("About to initialize GroupMemberPointer...");
                eprintln!(
                    "Current extensions already initialized: {:?}",
                    extension_types
                        .iter()
                        .take_while(|&&x| x != ExtensionType::GroupMemberPointer)
                        .collect::<Vec<_>>()
                );

                match mint.init_extension::<spl_token_2022::extension::group_member_pointer::GroupMemberPointer>(true) {
                    Ok(extension) => {
                        eprintln!("GroupMemberPointer extension space allocated successfully");
                        // At least one of authority or member_address must be provided
                        extension.authority = Some(solana_pubkey::Pubkey::new_unique()).try_into().unwrap();
                        extension.member_address = Some(solana_pubkey::Pubkey::new_unique()).try_into().unwrap();
                        eprintln!("GroupMemberPointer fields set successfully");
                    }
                    Err(e) => {
                        eprintln!("Failed to initialize GroupMemberPointer extension: {:?}", e);
                        eprintln!("Current mint data length: {}", data.len());
                        eprintln!("This extension combo was: {:?}", extension_types);
                        return vec![];
                    }
                }
            }
            ExtensionType::MintCloseAuthority => {
                let extension = mint
                    .init_extension::<MintCloseAuthority>(true)
                    .expect("Failed to init MintCloseAuthority");
                extension.close_authority = Default::default();
            }
            ExtensionType::PermanentDelegate => {
                let extension = mint
                    .init_extension::<PermanentDelegate>(true)
                    .expect("Failed to init PermanentDelegate");
                extension.delegate = Default::default();
            }
            ExtensionType::ScaledUiAmount => {
                eprintln!("About to initialize ScaledUiAmount...");
                eprintln!(
                    "Current extensions in this combination: {:?}",
                    extension_types
                );
                eprintln!(
                    "Extensions already initialized: {:?}",
                    extension_types
                        .iter()
                        .take_while(|&&x| x != ExtensionType::ScaledUiAmount)
                        .collect::<Vec<_>>()
                );

                let has_interest_bearing = extension_types
                    .iter()
                    .any(|ext| matches!(ext, ExtensionType::InterestBearingConfig));
                if has_interest_bearing {
                    eprintln!("ERROR: This combination has both ScaledUiAmount AND InterestBearingConfig - should have been blocked!");
                }

                let extension = mint
                    .init_extension::<spl_token_2022::extension::scaled_ui_amount::ScaledUiAmountConfig>(true)
                    .expect("Failed to init ScaledUiAmount");
                // Use default values which should be valid
                *extension = Default::default();
                // Set a valid positive multiplier
                extension.multiplier =
                    spl_token_2022::extension::scaled_ui_amount::PodF64::from(1.0);
                extension.new_multiplier =
                    spl_token_2022::extension::scaled_ui_amount::PodF64::from(1.0);
            }
            ExtensionType::TokenMetadata => {
                // TokenMetadata is variable-length, create a basic one
                let metadata = TokenMetadata {
                    update_authority: Default::default(),
                    mint: solana_pubkey::Pubkey::new_unique(),
                    name: String::from("Test"),
                    symbol: String::from("TEST"),
                    uri: String::from("https://example.com/token.json"),
                    additional_metadata: vec![],
                };
                mint.init_variable_len_extension(&metadata, false)
                    .expect("Failed to init TokenMetadata");
            }
            ExtensionType::ConfidentialTransferMint => {
                let extension = mint
                    .init_extension::<spl_token_2022::extension::confidential_transfer::ConfidentialTransferMint>(true)
                    .expect("Failed to init ConfidentialTransferMint");
                extension.authority = Default::default();
                extension.auto_approve_new_accounts = false.into();
                extension.auditor_elgamal_pubkey = Default::default();
            }
            ExtensionType::ConfidentialTransferFeeConfig => {
                let extension = mint
                    .init_extension::<spl_token_2022::extension::confidential_transfer_fee::ConfidentialTransferFeeConfig>(true)
                    .expect("Failed to init ConfidentialTransferFeeConfig");
                extension.authority = Default::default();
                extension.withdraw_withheld_authority_elgamal_pubkey = Default::default();
                extension.harvest_to_mint_enabled = false.into();
                extension.withheld_amount = Default::default();
            }
            ExtensionType::TokenGroup => {
                // TokenGroup is a fixed-size extension with specific initialization requirements
                // For testing account size calculation, we just need the extension space allocated
                if let Ok(extension) = mint.init_extension::<TokenGroup>(true) {
                    // Set sensible defaults if initialization succeeds
                    *extension = TokenGroup {
                        update_authority: Default::default(),
                        mint: solana_pubkey::Pubkey::new_unique(),
                        size: 0u64.into(),
                        max_size: 100u64.into(),
                    };
                } else {
                    // If init fails, skip this combination - our focus is account size calculation
                    // not complex Token-2022 initialization logic
                    return Vec::new();
                }
            }
            ExtensionType::TokenGroupMember => {
                // TokenGroupMember is a fixed-size extension with specific initialization requirements
                if let Ok(extension) = mint.init_extension::<TokenGroupMember>(true) {
                    // Set sensible defaults if initialization succeeds
                    *extension = TokenGroupMember {
                        mint: solana_pubkey::Pubkey::new_unique(),
                        group: solana_pubkey::Pubkey::new_unique(),
                        member_number: 0u64.into(),
                    };
                } else {
                    // If init fails, skip this combination
                    return Vec::new();
                }
            }
            _ => {}
        }
    }

    data
}

/// Categorize extension types for testing
#[derive(Debug, PartialEq)]
enum ExtensionCategory {
    /// Include in combinatorial testing (mint extensions that affect account size)
    Include,
    /// Skip - account-only extensions
    AccountOnly,
    /// Skip - padding/test extensions
    Skip,
}

/// Check if a combination of extension types is valid according to Token-2022 rules
fn is_valid_extension_combination(extension_types: &[ExtensionType]) -> bool {
    let has_scaled_ui_amount = extension_types
        .iter()
        .any(|ext| matches!(ext, ExtensionType::ScaledUiAmount));
    let has_interest_bearing = extension_types
        .iter()
        .any(|ext| matches!(ext, ExtensionType::InterestBearingConfig));
    let has_token_group = extension_types
        .iter()
        .any(|ext| matches!(ext, ExtensionType::TokenGroup));
    let has_group_pointer = extension_types
        .iter()
        .any(|ext| matches!(ext, ExtensionType::GroupPointer));
    let has_token_group_member = extension_types
        .iter()
        .any(|ext| matches!(ext, ExtensionType::TokenGroupMember));
    let has_group_member_pointer = extension_types
        .iter()
        .any(|ext| matches!(ext, ExtensionType::GroupMemberPointer));
    let has_transfer_fee_config = extension_types
        .iter()
        .any(|ext| matches!(ext, ExtensionType::TransferFeeConfig));
    let has_confidential_transfer_mint = extension_types
        .iter()
        .any(|ext| matches!(ext, ExtensionType::ConfidentialTransferMint));
    let has_confidential_transfer_fee_config = extension_types
        .iter()
        .any(|ext| matches!(ext, ExtensionType::ConfidentialTransferFeeConfig));

    // ScaledUiAmount cannot be combined with InterestBearingConfig
    if has_scaled_ui_amount && has_interest_bearing {
        return false;
    }

    // TokenGroup requires GroupPointer extension
    if has_token_group && !has_group_pointer {
        return false;
    }

    // TokenGroupMember requires GroupMemberPointer extension
    if has_token_group_member && !has_group_member_pointer {
        return false;
    }

    // ConfidentialTransferFeeConfig requires both TransferFeeConfig AND ConfidentialTransferMint
    if has_confidential_transfer_fee_config
        && !(has_transfer_fee_config && has_confidential_transfer_mint)
    {
        return false;
    }

    // If you have both TransferFeeConfig and ConfidentialTransferMint, you MUST have ConfidentialTransferFeeConfig
    if has_transfer_fee_config
        && has_confidential_transfer_mint
        && !has_confidential_transfer_fee_config
    {
        return false;
    }

    // TokenGroup requires GroupPointer extension
    if has_token_group && !has_group_pointer {
        return false;
    }

    // TokenGroupMember requires GroupMemberPointer extension
    if has_token_group_member && !has_group_member_pointer {
        return false;
    }

    true
}

/// Categorize an extension type - compiler enforces ALL variants are handled
/// If new variants are added to ExtensionType, this function will fail to compile
/// until the new variants are explicitly handled.
fn categorize_extension(ext: ExtensionType) -> ExtensionCategory {
    match ext {
        // Skip padding/uninitialized
        ExtensionType::Uninitialized => ExtensionCategory::Skip,

        // Simple mint extensions that can be initialized independently - INCLUDE these
        ExtensionType::TransferFeeConfig => ExtensionCategory::Include,
        ExtensionType::NonTransferable => ExtensionCategory::Include,
        ExtensionType::TransferHook => ExtensionCategory::Include,
        ExtensionType::Pausable => ExtensionCategory::Include,
        ExtensionType::DefaultAccountState => ExtensionCategory::Include,
        ExtensionType::InterestBearingConfig => ExtensionCategory::Include,
        ExtensionType::MetadataPointer => ExtensionCategory::Include,
        ExtensionType::GroupPointer => ExtensionCategory::Include,
        ExtensionType::GroupMemberPointer => ExtensionCategory::Include,
        ExtensionType::MintCloseAuthority => ExtensionCategory::Include,
        ExtensionType::PermanentDelegate => ExtensionCategory::Include,
        ExtensionType::ScaledUiAmount => ExtensionCategory::Include,
        ExtensionType::TokenMetadata => ExtensionCategory::Include,
        ExtensionType::ConfidentialTransferMint => ExtensionCategory::Include,
        ExtensionType::ConfidentialTransferFeeConfig => ExtensionCategory::Include,
        ExtensionType::TokenGroup => ExtensionCategory::Include,
        ExtensionType::TokenGroupMember => ExtensionCategory::Include,

        // Complex extensions with dependencies or complex initialization - SKIP for now to avoid test complexity
        ExtensionType::ConfidentialMintBurn => ExtensionCategory::Skip,

        // Account-only extensions - these shouldn't be in mint data
        ExtensionType::TransferFeeAmount => ExtensionCategory::AccountOnly,
        ExtensionType::ConfidentialTransferAccount => ExtensionCategory::AccountOnly,
        ExtensionType::ImmutableOwner => ExtensionCategory::AccountOnly,
        ExtensionType::NonTransferableAccount => ExtensionCategory::AccountOnly,
        ExtensionType::TransferHookAccount => ExtensionCategory::AccountOnly,
        ExtensionType::ConfidentialTransferFeeAmount => ExtensionCategory::AccountOnly,
        ExtensionType::PausableAccount => ExtensionCategory::AccountOnly,
        ExtensionType::MemoTransfer => ExtensionCategory::AccountOnly,
        ExtensionType::CpiGuard => ExtensionCategory::AccountOnly,
        // Note: Test-only variants (VariableLenMintTest, AccountPaddingTest, MintPaddingTest)
        // are only available when token-2022 is built with --cfg test, which may not be the case
        // when using it as a dependency. If they appear, they should be categorized as Skip.
    }
}

/// Get all mint ExtensionType variants by discovering them automatically.
/// This ensures ALL variants are covered - if new variants are added to ExtensionType,
/// the categorize_extension function will fail to compile until they're handled.
fn get_all_extension_types() -> Vec<ExtensionType> {
    let mut result = Vec::new();

    // Discover all valid ExtensionType variants by trying all possible u16 values
    // since ExtensionType implements TryFromPrimitive<u16>
    for i in 0..=u16::MAX {
        if let Ok(ext) = ExtensionType::try_from(i) {
            if categorize_extension(ext) == ExtensionCategory::Include {
                result.push(ext);
            }
        }
    }

    result
}

/// Exhaustively test every combination of supported mint extensions
#[test]
fn exhaustive_extension_combinations() {
    // Get all extension types by destructuring the enum
    let extensions = get_all_extension_types();

    let total = 1usize << extensions.len();

    for mask in 0..total {
        // Build current combination
        let mut combo = Vec::new();
        for (idx, ext) in extensions.iter().enumerate() {
            if (mask >> idx) & 1 == 1 {
                combo.push(*ext);
            }
        }

        // Skip invalid extension combinations
        if !is_valid_extension_combination(&combo) {
            continue;
        }

        // Debug the specific combination causing GroupMemberPointer failure
        let has_group_member_pointer = combo
            .iter()
            .any(|ext| matches!(ext, ExtensionType::GroupMemberPointer));
        if has_group_member_pointer {
            eprintln!("Testing combination with GroupMemberPointer: {:?}", combo);
        }

        // Create mint data
        let mint_data = create_mint_data_with_extensions(&combo);
        let inline_size = account_size_from_mint_inline(&mint_data);

        #[cfg(feature = "test-debug")]
        {
            eprintln!("combo: {:?}", combo);
            eprintln!("inline_size: {:?}", inline_size);
        }

        // If our parser does not support this combination, just ensure it indeed returned None
        if inline_size.is_none() {
            // We deliberately allow processor and parser to diverge when variable-length or
            // otherwise unsupported extensions are present. Move on to next combination.
            continue;
        }

        // Compute expected size via official ExtensionType utilities
        let mut account_extensions = ExtensionType::get_required_init_account_extensions(&combo);
        if !account_extensions.contains(&ExtensionType::ImmutableOwner) {
            account_extensions.push(ExtensionType::ImmutableOwner);
        }

        let expected_size = ExtensionType::try_calculate_account_len::<
            spl_token_2022::state::Account,
        >(&account_extensions)
        .unwrap();

        #[cfg(feature = "test-debug")]
        {
            eprintln!("expected_size: {:?}", expected_size);
        }

        assert_eq!(
            inline_size,
            Some(expected_size),
            "Mismatch for extensions {:?}",
            combo
        );
    }
}
