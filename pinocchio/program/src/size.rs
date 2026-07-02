use {
    crate::error::{
        invalid_account_size_return_data_len, no_account_size_return_data,
        unexpected_return_data_program,
    },
    pinocchio::{AccountView, cpi::get_return_data, error::ProgramError},
    pinocchio_token_2022::{
        instructions::GetAccountDataSize,
        state::{Account, ExtensionType, Mint},
    },
    spl_token_2022_interface::extension::{
        ExtensionType as SplExtensionType, account_len::try_calculate_account_len_from_mint_data,
    },
};

/// Token-2022 account data size when the mint has no extensions.
/// The only account extension is `ImmutableOwner` (zero-length value), so the
/// total is `TokenAccount::BASE_LEN (165) + ACCOUNT_TYPE_SIZE (1) + TLV_HEADER_LEN (4)`.
///
/// Reference: https://github.com/anza-xyz/pinocchio/blob/0ca7555836700b31dae01ef6da37ef66df1831b8/programs/token-2022/src/state/extension/mod.rs#L29-L30
const ACCOUNT_TYPE_SIZE: usize = 1;
/// Reference: https://github.com/anza-xyz/pinocchio/blob/0ca7555836700b31dae01ef6da37ef66df1831b8/programs/token-2022/src/state/extension/mod.rs#L31
const TLV_HEADER_LEN: usize = 4;
const TOKEN_2022_BASE_ACCOUNT_DATA_SIZE: u64 =
    Account::BASE_LEN as u64 + ACCOUNT_TYPE_SIZE as u64 + TLV_HEADER_LEN as u64;

/// Get the required Token-2022 account data size when no account length hint was supplied.
/// Short-circuits when size is known and falls back to `GetAccountDataSize` CPI for
/// everything else.
#[inline(always)]
pub(crate) fn get_token_2022_account_data_size(
    mint: &AccountView,
    token_program: &AccountView,
) -> Result<u64, ProgramError> {
    // Associated token accounts for Token-2022 always enable `ImmutableOwner`.
    // If the mint is exactly Mint::BASE_LEN, it has no mint extensions. In that case,
    // the new account only needs the base token account layout plus the
    // zero-length `ImmutableOwner` extension.
    if mint.data_len() == Mint::BASE_LEN {
        return Ok(TOKEN_2022_BASE_ACCOUNT_DATA_SIZE);
    }

    // Avoid the CPI when the mint data can be used to derive the account size locally.
    // If there is failure, fallback to a normal CPI to the token program.
    if let Ok(len) = mint.try_borrow().and_then(|mint_data| {
        try_calculate_account_len_from_mint_data(&mint_data, &[SplExtensionType::ImmutableOwner])
    }) {
        return Ok(len as u64);
    }

    get_account_data_size_cpi(mint, token_program)
}

fn get_account_data_size_cpi(
    mint: &AccountView,
    token_program: &AccountView,
) -> Result<u64, ProgramError> {
    let token_program_address = token_program.address();

    GetAccountDataSize {
        mint,
        extensions: &[ExtensionType::ImmutableOwner],
        token_program: token_program_address,
    }
    .invoke()?;

    get_return_data()
        .ok_or_else(no_account_size_return_data)
        .and_then(|return_data| {
            if return_data.program_id() != token_program_address {
                return Err(unexpected_return_data_program());
            }

            let bytes = return_data.as_slice();
            bytes
                .try_into()
                .map(u64::from_le_bytes)
                .map_err(|_| invalid_account_size_return_data_len(bytes.len()))
        })
}
