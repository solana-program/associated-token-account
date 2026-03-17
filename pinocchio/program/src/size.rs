use {
    pinocchio::{
        cpi::{self, get_return_data},
        error::ProgramError,
        instruction::{InstructionAccount, InstructionView},
        AccountView, Address,
    },
    pinocchio_log::log,
    pinocchio_token_2022::{
        instructions::GetAccountDataSize,
        state::{ExtensionType, Mint, TokenAccount},
    },
};

/// Token-2022 account data size when the mint has no extensions.
/// The only account extension is `ImmutableOwner` (zero-length value), so the
/// total is `TokenAccount::BASE_LEN (165) + TLV_START_INDEX (1) + TLV_HEADER_LEN (4)`.
///
/// Reference: https://github.com/anza-xyz/pinocchio/blob/b2c1d325d7da5ca7e4d7283c99690504c015b860/programs/token-2022/src/state/extension/mod.rs#L29-L30
const TLV_START_INDEX: usize = 1;
/// Reference: https://github.com/anza-xyz/pinocchio/blob/b2c1d325d7da5ca7e4d7283c99690504c015b860/programs/token-2022/src/state/extension/mod.rs#L31
const TLV_HEADER_LEN: usize = 4;
const TOKEN_2022_BASE_ACCOUNT_DATA_SIZE: u64 =
    TokenAccount::BASE_LEN as u64 + TLV_START_INDEX as u64 + TLV_HEADER_LEN as u64;

/// Get the required account data size for an ATA with the `ImmutableOwner` extension.
/// Short-circuits for the two common cases (SPL Token, Token-2022 with no mint
/// extensions) and falls back to a `GetAccountDataSize` CPI for everything else.
#[inline(always)]
pub(crate) fn get_account_data_size(
    mint: &AccountView,
    token_program: &AccountView,
) -> Result<u64, ProgramError> {
    if !token_program.executable() {
        return Err(ProgramError::IncorrectProgramId);
    }

    // SPL Token accounts are always exactly 165 bytes.
    if *token_program.address() == pinocchio_token::ID {
        return Ok(TokenAccount::BASE_LEN as u64);
    }

    // Associated token accounts for Token-2022 always enable `ImmutableOwner`.
    // If the mint is exactly Mint::BASE_LEN, it has no mint extensions. In that case,
    // the new account only needs the base token account layout plus the
    // zero-length `ImmutableOwner` extension.
    if *token_program.address() == pinocchio_token_2022::ID && mint.data_len() == Mint::BASE_LEN {
        return Ok(TOKEN_2022_BASE_ACCOUNT_DATA_SIZE);
    }

    // Fallback to token program CPI
    get_account_data_size_cpi(mint, token_program)
}

fn get_account_data_size_cpi(
    mint: &AccountView,
    token_program: &AccountView,
) -> Result<u64, ProgramError> {
    let mint_address = mint.address();
    let token_program_address = token_program.address();

    // TODO: pinocchio's `GetAccountDataSize` builder does not yet support `ImmutableOwner`
    let mut get_account_data_size_ix = [0; 3];
    get_account_data_size_ix[0] = GetAccountDataSize::DISCRIMINATOR;
    get_account_data_size_ix[1..]
        .copy_from_slice(&(ExtensionType::ImmutableOwner as u16).to_le_bytes());
    let cpi_result = cpi::invoke(
        &InstructionView {
            program_id: token_program_address,
            accounts: &[InstructionAccount::readonly(mint_address)],
            data: &get_account_data_size_ix,
        },
        &[mint],
    );

    let return_data = get_return_data();
    let return_data_unpacked = return_data
        .as_ref()
        .map(|return_data| (return_data.program_id(), return_data.as_slice()));

    resolve_account_data_size_cpi_result(cpi_result, token_program_address, return_data_unpacked)
}

// Uses dependency injection so tests can exercise all CPI result paths
pub(crate) fn resolve_account_data_size_cpi_result(
    cpi_result: Result<(), ProgramError>,
    token_program_address: &Address,
    return_data: Option<(&Address, &[u8])>,
) -> Result<u64, ProgramError> {
    cpi_result?;

    let (actual_program_id, return_data_bytes) = return_data.ok_or_else(|| {
        log!("Error: token program returned no account size data");
        ProgramError::InvalidInstructionData
    })?;

    if actual_program_id != token_program_address {
        log!("Error: account size return data came from the wrong program");
        return Err(ProgramError::IncorrectProgramId);
    }

    let result = u64::from_le_bytes(return_data_bytes.try_into().map_err(|_| {
        log!(
            "Error: invalid account size return data length: {}",
            return_data_bytes.len()
        );
        ProgramError::InvalidInstructionData
    })?);
    Ok(result)
}
