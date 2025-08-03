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

pub const GET_ACCOUNT_DATA_SIZE_DISCRIMINATOR: u8 = 21;
pub const MINT_BASE_SIZE: usize = 82;

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
        return Ok(TokenAccount::LEN);
    }

    // Token mint has no extensions other than ImmutableOwner
    // Note: This assumes future token programs include ImmutableOwner extension.
    if !token_mint_has_extensions(mint_account) {
        return Ok(TokenAccount::LEN + 5);
    }

    if is_spl_token_2022_program(token_program.key()) {
        // Try inline parsing first for Token-2022
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

#[cfg(test)]
mod tests {
    use super::*;
    use spl_token_2022::extension::ExtensionType;

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
}
