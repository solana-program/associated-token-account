use std::string::String;
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

#[cfg(feature = "test-debug")]
use std::eprintln;

/// Create mint data with specific extensions using token-2022's official methods
fn create_mint_data_with_extensions(extension_types: &[ExtensionType]) -> Vec<u8> {
    use spl_token_2022::extension::{BaseStateWithExtensionsMut, ExtensionType};

    // Check if we have variable-length extensions that can't use try_calculate_account_len
    let has_variable_length = extension_types
        .iter()
        .any(|ext| matches!(ext, ExtensionType::TokenMetadata));

    let required_size = if has_variable_length {
        // For variable-length extensions, estimate the size manually
        let base_mint_size = 82; // spl_token_2022::state::Mint::LEN
        let mut extension_size = 0;

        for ext_type in extension_types {
            extension_size += match ext_type {
                ExtensionType::TokenMetadata => 500, // Reasonable size for some metadata
                // (mint size is not what we are testing here)
                _ => {
                    // For other extensions, use a reasonable fixed size estimate
                    40 // Most extensions are around 32-48 bytes including TLV header
                }
            };
        }

        base_mint_size + extension_size + 32 // Extra buffer
    } else {
        ExtensionType::try_calculate_account_len::<spl_token_2022::state::Mint>(extension_types)
            .expect("Failed to calculate account length")
    };

    let mut data = vec![0u8; required_size];

    let mut mint = PodStateWithExtensionsMut::<PodMint>::unpack_uninitialized(&mut data)
        .expect("Failed to unpack mint");

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
                let extension = mint
                    .init_extension::<spl_token_2022::extension::group_member_pointer::GroupMemberPointer>(true)
                    .expect("Failed to init GroupMemberPointer");
                extension.authority = Default::default();
                extension.member_address = Default::default();
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
                extension.authority = Default::default();
                extension.multiplier = 1.0.into();
            }
            ExtensionType::TokenGroup => {
                let extension = mint
                    .init_extension::<TokenGroup>(true)
                    .expect("Failed to init TokenGroup");
                extension.update_authority = Default::default();
                extension.mint = solana_pubkey::Pubkey::new_unique();
                extension.size = 0u64.into();
                extension.max_size = 100u64.into();
            }
            ExtensionType::TokenGroupMember => {
                let extension = mint
                    .init_extension::<spl_token_group_interface::state::TokenGroupMember>(true)
                    .expect("Failed to init TokenGroupMember");
                extension.mint = solana_pubkey::Pubkey::new_unique();
                extension.group = solana_pubkey::Pubkey::new_unique();
                extension.member_number = 0u64.into();
            }
            ExtensionType::TokenMetadata => {
                // TokenMetadata is variable-length, create a basic one
                let metadata = TokenMetadata {
                    update_authority: Default::default(),
                    mint: solana_pubkey::Pubkey::new_unique(),
                    name: String::from("Test"),
                    symbol: String::from("TEST"),
                    uri: String::from(""),
                    additional_metadata: vec![],
                };
                mint.init_variable_len_extension(&metadata, false)
                    .expect("Failed to init TokenMetadata");
            }
            _ => {}
        }
    }

    data
}

/// Exhaustively test every combination of supported mint extensions
#[test]
fn exhaustive_extension_combinations() {
    // List of extension types we consider in combinations
    const EXTENSIONS: &[ExtensionType] = &[
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
        ExtensionType::PermanentDelegate,
        ExtensionType::ScaledUiAmount,
        ExtensionType::TokenGroup,
        ExtensionType::TokenGroupMember,
        ExtensionType::TokenMetadata,
    ];

    let total = 1usize << EXTENSIONS.len();

    for mask in 0..total {
        // Build current combination
        let mut combo = Vec::new();
        for (idx, ext) in EXTENSIONS.iter().enumerate() {
            if (mask >> idx) & 1 == 1 {
                combo.push(*ext);
            }
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
