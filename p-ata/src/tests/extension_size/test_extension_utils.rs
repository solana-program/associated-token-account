use spl_token_2022::extension::{
    default_account_state::DefaultAccountState, group_pointer::GroupPointer,
    interest_bearing_mint::InterestBearingConfig, metadata_pointer::MetadataPointer,
    mint_close_authority::MintCloseAuthority, non_transferable::NonTransferable,
    pausable::PausableConfig, permanent_delegate::PermanentDelegate,
    transfer_fee::TransferFeeConfig, transfer_hook::TransferHook, ExtensionType,
    PodStateWithExtensionsMut,
};
#[cfg(test)]
use spl_token_2022::pod::PodMint;
use spl_token_group_interface::state::TokenGroup;
use spl_token_group_interface::state::TokenGroupMember;
use spl_token_metadata_interface::state::TokenMetadata;
use std::string::String;
use std::vec::Vec;

#[derive(Debug, PartialEq)]
pub enum ExtensionCategory {
    Include,
    AccountOnly,
    Skip,
}

/// If new variants are added to ExtensionType, this function will fail to compile
/// until the new variants are explicitly handled. Note this ignores the program's
/// anticipated "`PlannedZeroAccountDataLengthExtension`"
pub fn categorize_extension(ext: ExtensionType) -> ExtensionCategory {
    match ext {
        // Skip padding/uninitialized
        ExtensionType::Uninitialized => ExtensionCategory::Skip,

        // Simple mint extensions that can be initialized independently
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

/// Create mint data with specific extensions using token-2022's official methods
pub fn create_mint_data_with_extensions(extension_types: &[ExtensionType]) -> Vec<u8> {
    use spl_token_2022::extension::{BaseStateWithExtensionsMut, ExtensionType};
    use std::string::String;
    use std::{vec, vec::Vec};

    // Check for variable-length extensions we must size manually
    let has_variable_length = extension_types
        .iter()
        .any(|ext| matches!(ext, ExtensionType::TokenMetadata));

    let required_size = if has_variable_length {
        // Calculate length for all sized extensions first
        let mut sized_exts: Vec<ExtensionType> = extension_types
            .iter()
            .copied()
            .filter(|e| !matches!(e, ExtensionType::TokenMetadata))
            .collect();

        let mut required_size =
            ExtensionType::try_calculate_account_len::<spl_token_2022::state::Mint>(&sized_exts)
                .expect("calc len for sized subset");

        // Add a generous buffer for the variable-length TokenMetadata TLV entry
        if extension_types
            .iter()
            .any(|e| matches!(e, ExtensionType::TokenMetadata))
        {
            const TOKEN_METADATA_VALUE_LEN_ESTIMATE: usize = 500;
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
        Err(e) => panic!(
            "Failed to unpack mint for extensions {:?}: {:?}",
            extension_types, e
        ),
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
                if let Ok(extension) = mint.init_extension::<spl_token_2022::extension::group_member_pointer::GroupMemberPointer>(true) {
                    extension.authority = Some(solana_pubkey::Pubkey::new_unique()).try_into().unwrap();
                    extension.member_address = Some(solana_pubkey::Pubkey::new_unique()).try_into().unwrap();
                } else {
                    panic!("Failed to init GroupMemberPointer");
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
                let extension = mint
                    .init_extension::<spl_token_2022::extension::scaled_ui_amount::ScaledUiAmountConfig>(true)
                    .expect("Failed to init ScaledUiAmount");
                *extension = Default::default();
                extension.multiplier = spl_token_2022::extension::scaled_ui_amount::PodF64::from(1.0);
                extension.new_multiplier = spl_token_2022::extension::scaled_ui_amount::PodF64::from(1.0);
            }
            ExtensionType::TokenMetadata => {
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
                if let Ok(extension) = mint.init_extension::<TokenGroup>(true) {
                    *extension = TokenGroup {
                        update_authority: Default::default(),
                        mint: solana_pubkey::Pubkey::new_unique(),
                        size: 0u64.into(),
                        max_size: 100u64.into(),
                    };
                } else {
                    panic!("Failed to init TokenGroup");
                }
            }
            ExtensionType::TokenGroupMember => {
                if let Ok(extension) = mint.init_extension::<TokenGroupMember>(true) {
                    *extension = TokenGroupMember {
                        mint: solana_pubkey::Pubkey::new_unique(),
                        group: solana_pubkey::Pubkey::new_unique(),
                        member_number: 0u64.into(),
                    };
                } else {
                    panic!("Failed to init TokenGroupMember");
                }
            }
            _ => {}
        }
    }

    data
}

/// Get all extension types by category by discovering them automatically
pub fn get_extensions_by_category(category: ExtensionCategory) -> Vec<ExtensionType> {
    let mut result = Vec::new();
    for i in 0..=u16::MAX {
        if let Ok(ext) = ExtensionType::try_from(i) {
            if categorize_extension(ext) == category {
                result.push(ext);
            }
        }
    }
    result
}

/// Check if a combination of extension types is valid according to Token-2022 rules
pub fn is_valid_extension_combination(extension_types: &[ExtensionType]) -> bool {
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

    true
}

/// Calculate expected ATA account size using token-2022 method
/// Adds `ImmutableOwner` as it is always included in ATA accounts
pub fn calculate_expected_ata_data_size(mint_extensions: &[ExtensionType]) -> usize {
    let mut account_extensions =
        ExtensionType::get_required_init_account_extensions(mint_extensions);

    // ATA always includes ImmutableOwner, so include it in our comparison
    if !account_extensions.contains(&ExtensionType::ImmutableOwner) {
        account_extensions.push(ExtensionType::ImmutableOwner);
    }

    ExtensionType::try_calculate_account_len::<spl_token_2022::state::Account>(&account_extensions)
        .expect("Failed to calculate account length")
}

#[cfg(test)]
pub fn test_extension_combination_helper(
    extensions: &[ExtensionType],
    description: &str,
) -> Result<(), String> {
    use crate::{
        size::calculate_account_size_from_mint_extensions,
        tests::extension_size::test_extension_utils::create_mint_data_with_extensions,
    };

    let mint_data = create_mint_data_with_extensions(extensions);
    let expected_size = calculate_expected_ata_data_size(extensions);
    let result = calculate_account_size_from_mint_extensions(&mint_data);

    if result != Some(expected_size) {
        let mut error_msg = String::from("Extension combination failed: ");
        error_msg.push_str(description);
        error_msg.push_str(". Extensions: [");
        error_msg.push_str("...extensions...]");
        return Err(error_msg);
    }

    Ok(())
}
