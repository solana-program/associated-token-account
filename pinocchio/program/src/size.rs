use {
    pinocchio::{
        cpi::{self, get_return_data},
        error::ProgramError,
        instruction::{InstructionAccount, InstructionView},
        AccountView,
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
/// Reference: https://github.com/anza-xyz/pinocchio/blob/0ca7555836700b31dae01ef6da37ef66df1831b8/programs/token-2022/src/state/extension/mod.rs#L29-L30
const TLV_START_INDEX: usize = 1;
/// Reference: https://github.com/anza-xyz/pinocchio/blob/0ca7555836700b31dae01ef6da37ef66df1831b8/programs/token-2022/src/state/extension/mod.rs#L31
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
    // SPL Token accounts are always exactly 165 bytes.
    if *token_program.address() == pinocchio_token::ID {
        return Ok(TokenAccount::BASE_LEN as u64);
    }

    if *token_program.address() == pinocchio_token_2022::ID {
        // Associated token accounts for Token-2022 always enable `ImmutableOwner`.
        // If the mint is exactly Mint::BASE_LEN, it has no mint extensions. In that case,
        // the new account only needs the base token account layout plus the
        // zero-length `ImmutableOwner` extension.
        if mint.data_len() == Mint::BASE_LEN {
            return Ok(TOKEN_2022_BASE_ACCOUNT_DATA_SIZE);
        }

        return get_account_data_size_cpi(mint, token_program);
    }

    // Only SPL Token and Token-2022 are allowed
    Err(ProgramError::IncorrectProgramId)
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

    cpi::invoke(
        &InstructionView {
            program_id: token_program_address,
            accounts: &[InstructionAccount::readonly(mint_address)],
            data: &get_account_data_size_ix,
        },
        &[mint],
    )?;

    get_return_data()
        .ok_or_else(|| {
            log!("Error: token program returned no account size data");
            ProgramError::InvalidInstructionData
        })
        .and_then(|return_data| {
            let bytes = return_data.as_slice();
            bytes.try_into().map(u64::from_le_bytes).map_err(|_| {
                log!(
                    "Error: invalid account size return data length: {}",
                    bytes.len()
                );
                ProgramError::InvalidInstructionData
            })
        })
}
