#[allow(unexpected_cfgs)]
use {
    crate::processor::calculate_account_size_from_mint_extensions,
    spl_token_2022::extension::ExtensionType, std::vec::Vec,
};

#[cfg(feature = "test-debug")]
use std::eprintln;

/// Create mint data with specific extensions using token-2022's official methods
fn create_mint_data_with_extensions(extension_types: &[ExtensionType]) -> Vec<u8> {
    super::test_utils::create_mint_data_with_extensions(extension_types)
}

/// Categorize extension types for testing
#[derive(Debug, PartialEq)]
enum ExtensionCategory {
    Include,
    AccountOnly,
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
        ExtensionType::ConfidentialMintBurn => ExtensionCategory::Include,

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
    }
}

/// Get all mint ExtensionType variants by discovering them automatically.
fn get_all_extension_types() -> Vec<ExtensionType> {
    let mut result = Vec::new();
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
        let mut combo = Vec::new();
        for (idx, ext) in extensions.iter().enumerate() {
            if (mask >> idx) & 1 == 1 {
                combo.push(*ext);
            }
        }

        if !is_valid_extension_combination(&combo) {
            continue;
        }

        #[cfg(feature = "test-debug")]
        {
            eprintln!("combo: {:?}", combo);
        }

        // Create mint data
        let mint_data = create_mint_data_with_extensions(&combo);
        let inline_size = calculate_account_size_from_mint_extensions(&mint_data);

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

        #[cfg(feature = "test-debug")]
        {
            eprintln!("inline_size: {:?}", inline_size);
        }

        assert_eq!(
            inline_size,
            Some(expected_size),
            "Mismatch for extensions {:?}",
            combo
        );
    }
}
