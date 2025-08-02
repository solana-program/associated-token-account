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
use pinocchio_token::state::TokenAccount;

use crate::processor::is_spl_token_program;

pub const GET_ACCOUNT_DATA_SIZE_DISCM: u8 = 21;
pub const MINT_BASE_SIZE: usize = 82;

/// ExtensionType, exactly as Token-2022, with additional planned extension.
#[repr(u16)]
#[allow(dead_code)]
pub enum ExtensionType {
    /// Used as padding if the account size would otherwise be 355, same as a
    /// multisig
    Uninitialized,
    /// Includes transfer fee rate info and accompanying authorities to withdraw
    /// and set the fee
    TransferFeeConfig,
    /// Includes withheld transfer fees
    TransferFeeAmount,
    /// Includes an optional mint close authority
    MintCloseAuthority,
    /// Auditor configuration for confidential transfers
    ConfidentialTransferMint,
    /// State for confidential transfers
    ConfidentialTransferAccount,
    /// Specifies the default Account::state for new Accounts
    DefaultAccountState,
    /// Indicates that the Account owner authority cannot be changed
    ImmutableOwner,
    /// Require inbound transfers to have memo
    MemoTransfer,
    /// Indicates that the tokens from this mint can't be transferred
    NonTransferable,
    /// Tokens accrue interest over time,
    InterestBearingConfig,
    /// Locks privileged token operations from happening via CPI
    CpiGuard,
    /// Includes an optional permanent delegate
    PermanentDelegate,
    /// Indicates that the tokens in this account belong to a non-transferable
    /// mint
    NonTransferableAccount,
    /// Mint requires a CPI to a program implementing the "transfer hook"
    /// interface
    TransferHook,
    /// Indicates that the tokens in this account belong to a mint with a
    /// transfer hook
    TransferHookAccount,
    /// Includes encrypted withheld fees and the encryption public that they are
    /// encrypted under
    ConfidentialTransferFeeConfig,
    /// Includes confidential withheld transfer fees
    ConfidentialTransferFeeAmount,
    /// Mint contains a pointer to another account (or the same account) that
    /// holds metadata
    MetadataPointer,
    /// Mint contains token-metadata
    TokenMetadata,
    /// Mint contains a pointer to another account (or the same account) that
    /// holds group configurations
    GroupPointer,
    /// Mint contains token group configurations
    TokenGroup,
    /// Mint contains a pointer to another account (or the same account) that
    /// holds group member configurations
    GroupMemberPointer,
    /// Mint contains token group member configurations
    TokenGroupMember,
    /// Mint allowing the minting and burning of confidential tokens
    ConfidentialMintBurn,
    /// Tokens whose UI amount is scaled by a given amount
    ScaledUiAmount,
    /// Tokens where minting / burning / transferring can be paused
    Pausable,
    /// Indicates that the account belongs to a pausable mint
    PausableAccount,
    /// PLANNED next Token-2022 extension (0 account length)
    PlannedZeroAccountDataLengthExtension,
}

/// Check if the given program ID is Token-2022
#[inline(always)]
pub(crate) fn is_token_2022_program(program_id: &Pubkey) -> bool {
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
    const ACCOUNT_TYPE_OFFSET: usize = TokenAccount::LEN;
    const BASE_TOKEN_2022_ACCOUNT_SIZE: usize = TokenAccount::LEN + 5;

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

    // SAFETY: cursor is always verified to be within the slice before deref
    while cursor + 4 <= data_len {
        // Read 4-byte TLV header (little-endian: [type: u16 | length: u16])
        let header =
            unsafe { core::ptr::read_unaligned(mint_data.as_ptr().add(cursor) as *const u32) };
        let extension_type_raw = (header & 0xFFFF) as u16;
        let length = (header >> 16) as u16;

        // TypeEnd / Uninitialized ends the TLV list
        if extension_type_raw == 0 {
            break;
        }

        let extension_type =
            if extension_type_raw <= ExtensionType::PlannedZeroAccountDataLengthExtension as u16 {
                // SAFETY: ExtensionType is repr(u16) and we've validated the value is in range
                unsafe { core::mem::transmute::<u16, ExtensionType>(extension_type_raw) }
            } else {
                // Unknown extension type - fall back to CPI
                return None;
            };

        // Based on token-2022's get_required_init_account_extensions
        match extension_type {
            ExtensionType::TransferFeeConfig => {
                // TransferFeeConfig → needs TransferFeeAmount (8 bytes data + 4 TLV)
                account_extensions_size += 4 + 8;
            }
            ExtensionType::NonTransferable => {
                // NonTransferable → NonTransferableAccount (0 bytes data + 4 TLV)
                account_extensions_size += 4;
            }
            ExtensionType::TransferHook => {
                // TransferHook → TransferHookAccount (1 byte data + 4 TLV)
                account_extensions_size += 4 + 1;
            }
            ExtensionType::Pausable => {
                // Pausable → PausableAccount (0 bytes data + 4 TLV)
                account_extensions_size += 4;
            }
            // All other known extensions
            _ => {
                // No account-side data required
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
        return Ok(TokenAccount::LEN);
    }

    // Token mint has no extensions other than ImmutableOwner
    // Note: This assumes future token programs include ImmutableOwner extension.
    if !token_mint_has_extensions(mint_account) {
        return Ok(TokenAccount::LEN + 5);
    }

    if is_token_2022_program(token_program.key()) {
        // Try inline parsing first for Token-2022
        let mint_data = unsafe { mint_account.borrow_data_unchecked() };
        if let Some(size) = calculate_account_size_from_mint_extensions(mint_data) {
            return Ok(size);
        }
    }

    // Fallback to CPI for unknown/variable-length extensions or unknown token programs
    // ImmutableOwner extension is required for Token-2022 Associated Token Accounts
    let instruction_data = [GET_ACCOUNT_DATA_SIZE_DISCM, 7u8, 0u8]; // [7, 0] = ImmutableOwner as u16

    let get_size_metas = &[AccountMeta {
        pubkey: mint_account.key(),
        is_writable: false,
        is_signer: false,
    }];

    let get_size_ix = Instruction {
        program_id: token_program.key(),
        accounts: get_size_metas,
        data: &instruction_data,
    };

    cpi::invoke(&get_size_ix, &[mint_account])?;
    let return_data = cpi::get_return_data().ok_or(ProgramError::InvalidAccountData)?;

    // `try_into` as this could be an unknown token program;
    // it must error if it doesn't give us [u8; 8]
    Ok(u64::from_le_bytes(
        return_data
            .as_slice()
            .try_into()
            .map_err(|_| ProgramError::InvalidAccountData)?,
    ) as usize)
}

/// Check if a Token-2022 mint has extensions by examining its data length
#[inline(always)]
pub(crate) fn token_mint_has_extensions(mint_account: &AccountInfo) -> bool {
    // If mint data is larger than base, it has extensions
    mint_account.data_len() > MINT_BASE_SIZE
}
