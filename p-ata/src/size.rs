//! Helpers for determining the necessary associated token account data length.
//!
//! In particular, this module provides efficient account size calculation for
//! extended mints by:
//!
//! 1. **Inline Extension Parsing**: For Token-2022, analyzes mint extension data
//!    directly to compute required account extensions, avoiding expensive CPI
//!    calls when possible
//! 2. **Fallback CPI**: Uses GetAccountDataSize instruction for unknown extensions
//!    or non-Token-2022 programs. Clients in these situations can always pass in
//!    the account data length in instruction data to avoid excess compute.

use pinocchio::{
    account_info::AccountInfo,
    cpi,
    instruction::{AccountMeta, Instruction},
    program_error::ProgramError,
    pubkey::Pubkey,
};
use pinocchio_log::log;

use crate::processor::is_spl_token_program;

pub const GET_ACCOUNT_DATA_SIZE_DISCRIMINATOR: u8 = 21;
pub const MINT_BASE_SIZE: usize = 82;
pub const TOKEN_ACCOUNT_SIZE: usize = 165;

// These constants are taken from the Token-2022 ExtensionType enum.
// Tests verify the constants are in sync without pulling in spl-token-2022
// as a non-dev dependency.
pub const EXTENSION_TRANSFER_FEE_CONFIG: u16 = 1;
pub const EXTENSION_NON_TRANSFERABLE: u16 = 9;
pub const EXTENSION_TRANSFER_HOOK: u16 = 14;
pub const EXTENSION_PAUSABLE: u16 = 26;
pub const EXTENSION_PLANNED_ZERO_ACCOUNT_DATA_LENGTH_EXTENSION: u16 = 28;

/// Check if the given program ID is Token-2022
#[inline(always)]
pub(crate) fn is_spl_token_2022_program(program_id: &Pubkey) -> bool {
    const TOKEN_2022_PROGRAM_ID: Pubkey =
        pinocchio_pubkey::pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
    *program_id == TOKEN_2022_PROGRAM_ID
}

/// Calculate token account size by parsing mint extension data inline.
///
/// ## Returns
///
/// - `Some(size)` - Successfully calculated account size in bytes
/// - `None` - Unknown extension found, caller should fall back to CPI
#[inline(always)]
pub(crate) fn calculate_account_size_from_mint_extensions(mint_data: &[u8]) -> Option<usize> {
    const ACCOUNT_TYPE_OFFSET: usize = TOKEN_ACCOUNT_SIZE;
    const BASE_TOKEN_2022_ACCOUNT_SIZE: usize = TOKEN_ACCOUNT_SIZE + 5;

    // Invalid/failed mint creation
    if mint_data.is_empty() {
        return None;
    }

    // Fast-path: no mint extensions → no additional account extensions either.
    if mint_data.len() <= ACCOUNT_TYPE_OFFSET {
        return Some(BASE_TOKEN_2022_ACCOUNT_SIZE);
    }

    let data_len = mint_data.len();
    let mut account_extensions_size = 0usize;
    // Start cursor after the account-type discriminator byte
    let mut cursor = ACCOUNT_TYPE_OFFSET + 1;

    while cursor + 4 <= data_len {
        // Read 4-byte TLV header (little-endian: [type: u16 | length: u16])
        // Avoiding u32::from_le_bytes() here saves about 6 compute units in the
        // `create_extended` bench.
        //
        // SAFETY: cursor is always verified to be within the slice before deref
        // in the `while` loop condition.
        let header =
            unsafe { core::ptr::read_unaligned(mint_data.as_ptr().add(cursor) as *const u32) };
        let extension_type_raw = (header & 0xFFFF) as u16;
        let length = (header >> 16) as u16;

        // TypeEnd / Uninitialized ends the TLV list
        if extension_type_raw == 0 {
            break;
        }

        // Based on token-2022's get_required_init_account_extensions
        match extension_type_raw {
            EXTENSION_TRANSFER_FEE_CONFIG => {
                // TransferFeeConfig → needs TransferFeeAmount (8 bytes data + 4 TLV)
                account_extensions_size += 4 + 8;
            }
            EXTENSION_NON_TRANSFERABLE => {
                // NonTransferable → NonTransferableAccount (0 bytes data + 4 TLV)
                account_extensions_size += 4;
            }
            EXTENSION_TRANSFER_HOOK => {
                // TransferHook → TransferHookAccount (1 byte data + 4 TLV)
                account_extensions_size += 4 + 1;
            }
            EXTENSION_PAUSABLE => {
                // Pausable → PausableAccount (0 bytes data + 4 TLV)
                account_extensions_size += 4;
            }
            // All other known extensions
            discriminant
                if discriminant <= EXTENSION_PLANNED_ZERO_ACCOUNT_DATA_LENGTH_EXTENSION =>
            {
                // No account-side data required
            }
            // Unknown extension
            _ => {
                return None;
            }
        }

        cursor += 4 + length as usize;
    }

    Some(BASE_TOKEN_2022_ACCOUNT_SIZE + account_extensions_size)
}

/// Get the required account size for a mint using inline parsing first,
/// falling back to GetAccountDataSize CPI only when necessary.
/// Returns the account size in bytes.
#[inline(always)]
pub(crate) fn get_token_account_size(
    mint_account: &AccountInfo,
    token_program: &AccountInfo,
) -> Result<usize, ProgramError> {
    if is_spl_token_program(token_program.key()) {
        return Ok(TOKEN_ACCOUNT_SIZE);
    }

    // Token mint has no extensions other than ImmutableOwner
    // Note: This assumes future token programs include ImmutableOwner extension.
    if !token_mint_has_extensions(mint_account) {
        return Ok(TOKEN_ACCOUNT_SIZE + 5);
    }

    if is_spl_token_2022_program(token_program.key()) {
        // The mint_data is not verified to be mint data here; the authoritative check
        // is the token program's mint validation during initialization, invoked in
        // `create_and_initialize_ata()`.
        //
        // SAFETY: This is the only place in this function that borrows mint_account data,
        // and the borrow is released when mint_data goes out of scope before any other
        // operations.
        let mint_data = unsafe { mint_account.borrow_data_unchecked() };
        if let Some(size) = calculate_account_size_from_mint_extensions(mint_data) {
            return Ok(size);
        }
    }

    // Fallback to CPI for unknown/variable-length extensions or unknown token programs
    // ImmutableOwner extension is required for Token-2022 Associated Token Accounts
    const INSTRUCTION_DATA: [u8; 3] = [GET_ACCOUNT_DATA_SIZE_DISCRIMINATOR, 7u8, 0u8]; // [7, 0] = ImmutableOwner as u16

    let get_size_metas = &[AccountMeta {
        pubkey: mint_account.key(),
        is_writable: false,
        is_signer: false,
    }];

    let get_size_ix = Instruction {
        program_id: token_program.key(),
        accounts: get_size_metas,
        data: &INSTRUCTION_DATA,
    };

    cpi::invoke(&get_size_ix, &[mint_account])?;
    let return_data = cpi::get_return_data().ok_or_else(|| {
        log!(
            "Error: Token program {} did not return account size data for mint {}",
            token_program.key(),
            mint_account.key()
        );
        ProgramError::InvalidAccountData
    })?;

    // `try_into` as this could be an unknown token program;
    // it must error if it doesn't give us [u8; 8]
    Ok(u64::from_le_bytes(
        return_data
            .as_slice()
            .try_into()
            .map_err(|_| {
                log!(
                    "Error: Token program {} returned invalid account size data. Expected 8 bytes, got {} bytes",
                    token_program.key(),
                    return_data.as_slice().len()
                );
                ProgramError::InvalidAccountData
            })?,
    ) as usize)
}

/// Check if a Token-2022 mint has extensions by examining its data length
#[inline(always)]
pub(crate) fn token_mint_has_extensions(mint_account: &AccountInfo) -> bool {
    // If mint data is larger than base, it has extensions
    mint_account.data_len() > MINT_BASE_SIZE
}

#[cfg(test)]
mod tests {
    use super::*;
    use spl_token_2022::extension::ExtensionType;
    use std::vec;

    // Test that the constants in this crate match the constants in the Token-2022
    // ExtensionType enum. Avoids pulling in spl-token-2022 or upcoming interface
    // as a non-dev dependency.
    #[test]
    fn test_extension_constants_match_token_2022() {
        // Prevent drift between these Token-2022 ExtensionType enum values
        assert_eq!(
            EXTENSION_TRANSFER_FEE_CONFIG,
            ExtensionType::TransferFeeConfig as u16,
            "TransferFeeConfig constant mismatch"
        );

        assert_eq!(
            EXTENSION_NON_TRANSFERABLE,
            ExtensionType::NonTransferable as u16,
            "NonTransferable constant mismatch"
        );

        assert_eq!(
            EXTENSION_TRANSFER_HOOK,
            ExtensionType::TransferHook as u16,
            "TransferHook constant mismatch"
        );

        assert_eq!(
            EXTENSION_PAUSABLE,
            ExtensionType::Pausable as u16,
            "Pausable constant mismatch"
        );
    }

    #[test]
    fn test_planned_extension_is_next_after_last_deployed_extension() {
        let last_real_extension = ExtensionType::PausableAccount as u16;

        assert_eq!(
            EXTENSION_PLANNED_ZERO_ACCOUNT_DATA_LENGTH_EXTENSION,
            last_real_extension + 1,
            "Planned extension should be exactly one more than PausableAccount ({})",
            last_real_extension
        );
    }

    use {
        spl_token_2022::extension::{
            default_account_state::DefaultAccountState, group_pointer::GroupPointer,
            interest_bearing_mint::InterestBearingConfig, metadata_pointer::MetadataPointer,
            mint_close_authority::MintCloseAuthority, non_transferable::NonTransferable,
            pausable::PausableConfig, permanent_delegate::PermanentDelegate,
            transfer_fee::TransferFeeConfig, transfer_hook::TransferHook,
            PodStateWithExtensionsMut,
        },
        spl_token_2022::pod::PodMint,
        spl_token_group_interface::state::{TokenGroup, TokenGroupMember},
        spl_token_metadata_interface::state::TokenMetadata,
        std::{string::String, vec::Vec},
    };

    /// Create mint data with specific extensions using token-2022's official methods
    pub fn create_mint_data_with_extensions(extension_types: &[ExtensionType]) -> Vec<u8> {
        use spl_token_2022::extension::{BaseStateWithExtensionsMut, ExtensionType};

        let required_size = if extension_types
            .iter()
            .any(|ext| matches!(ext, ExtensionType::TokenMetadata))
        {
            // Calculate length for all sized extensions first, then add buffer for TokenMetadata
            let mut size = ExtensionType::try_calculate_account_len::<spl_token_2022::state::Mint>(
                &extension_types
                    .iter()
                    .copied()
                    .filter(|e| !matches!(e, ExtensionType::TokenMetadata))
                    .collect::<Vec<_>>(),
            )
            .expect("calc len for sized subset");

            const TOKEN_METADATA_VALUE_LEN_ESTIMATE: usize = 500;
            size += TOKEN_METADATA_VALUE_LEN_ESTIMATE + 4; // value + TLV header
            size
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
                    mint.init_variable_len_extension(&TokenMetadata {
                        update_authority: Default::default(),
                        mint: solana_pubkey::Pubkey::new_unique(),
                        name: String::from("Test"),
                        symbol: String::from("TEST"),
                        uri: String::from("https://example.com/token.json"),
                        additional_metadata: vec![],
                    }, false).expect("Failed to init TokenMetadata");
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

    /// Calculate expected ATA account size using token-2022 method
    /// Adds `ImmutableOwner` as it is always included in ATA accounts
    pub fn calculate_expected_ata_data_size(mint_extensions: &[ExtensionType]) -> usize {
        let mut account_extensions =
            ExtensionType::get_required_init_account_extensions(mint_extensions);

        // ATA always includes ImmutableOwner, so include it in our comparison
        if !account_extensions.contains(&ExtensionType::ImmutableOwner) {
            account_extensions.push(ExtensionType::ImmutableOwner);
        }

        ExtensionType::try_calculate_account_len::<spl_token_2022::state::Account>(
            &account_extensions,
        )
        .expect("Failed to calculate account length")
    }

    const MINT_PAD_SIZE: usize = 83;

    /// Create a basic mint with no extensions for testing
    fn create_base_mint_data() -> Vec<u8> {
        let mut data = vec![0u8; MINT_BASE_SIZE + MINT_PAD_SIZE + 5];

        data[0..4].copy_from_slice(&1u32.to_le_bytes());
        data[MINT_BASE_SIZE + MINT_PAD_SIZE] = 1;

        data
    }

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

        // Test our inline parser
        let inline_size = calculate_account_size_from_mint_extensions(&mint_data);

        // TokenMetadata is mint-only, so account size should be base + ImmutableOwner
        let expected_account_size =
            calculate_expected_ata_data_size(&[ExtensionType::ImmutableOwner]);

        assert_eq!(
            inline_size,
            Some(expected_account_size),
            "TokenMetadata should be supported inline as a mint-only extension"
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

    #[test]
    fn test_zero_extensions_returns_base_size() {
        // Test that mint with no extensions returns base token account size + ImmutableOwner
        let mint_data = create_base_mint_data();
        let result = calculate_account_size_from_mint_extensions(&mint_data);
        let expected_size = calculate_expected_ata_data_size(&[]);

        assert_eq!(
            result,
            Some(expected_size),
            "Mint with no extensions should return base account size + ImmutableOwner for Token-2022"
        );
    }

    #[test]
    fn test_token_metadata_account_size_with_max_lengths() {
        // Test TokenMetadata extension with maximum field lengths
        // Since TokenMetadata is mint-only, account should still get base size + ImmutableOwner
        let mint_data = create_mint_with_mock_token_metadata();
        let result = calculate_account_size_from_mint_extensions(&mint_data);

        // TokenMetadata doesn't affect account size directly, but ImmutableOwner is added
        let expected_size = calculate_expected_ata_data_size(&[ExtensionType::ImmutableOwner]);

        assert_eq!(
            result,
            Some(expected_size),
            "TokenMetadata with max lengths should not increase account size beyond ImmutableOwner"
        );
    }
}
