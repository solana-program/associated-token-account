use {
    core::mem::size_of,
    pinocchio::{
        AccountView,
        cpi::{self, get_return_data},
        error::ProgramError,
        instruction::{InstructionAccount, InstructionView},
    },
    pinocchio_log::log,
    pinocchio_token_2022::{
        instructions::GetAccountDataSize,
        state::{
            ExtensionType, Mint, StateWithExtensions, TokenAccount, TransferHookAccountExtension,
        },
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
    TokenAccount::BASE_LEN as u64 + ACCOUNT_TYPE_SIZE as u64 + TLV_HEADER_LEN as u64;
/// Reference: https://github.com/solana-program/token-2022/blob/interface%40v3.0.0/interface/src/extension/transfer_fee/mod.rs#L178-L181
const TOKEN_2022_TRANSFER_FEE_ACCOUNT_DATA_SIZE: u64 =
    TOKEN_2022_BASE_ACCOUNT_DATA_SIZE + TLV_HEADER_LEN as u64 + size_of::<u64>() as u64;
/// Reference: https://github.com/solana-program/token-2022/blob/interface%40v3.0.0/interface/src/extension/non_transferable.rs#L15-L21
const TOKEN_2022_NON_TRANSFERABLE_ACCOUNT_DATA_SIZE: u64 =
    TOKEN_2022_BASE_ACCOUNT_DATA_SIZE + TLV_HEADER_LEN as u64;
/// Reference: https://github.com/anza-xyz/pinocchio/blob/0ca7555836700b31dae01ef6da37ef66df1831b8/programs/token-2022/src/state/extension/transfer_hook_account.rs#L9-L14
const TOKEN_2022_TRANSFER_HOOK_ACCOUNT_DATA_SIZE: u64 = TOKEN_2022_BASE_ACCOUNT_DATA_SIZE
    + TLV_HEADER_LEN as u64
    + TransferHookAccountExtension::LEN as u64;
/// Reference: https://github.com/solana-program/token-2022/blob/interface%40v3.0.0/interface/src/extension/pausable/mod.rs#L35-L42
const TOKEN_2022_PAUSABLE_ACCOUNT_DATA_SIZE: u64 =
    TOKEN_2022_BASE_ACCOUNT_DATA_SIZE + TLV_HEADER_LEN as u64;

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

    // Attempt to avoid CPI by matching with known account data sizes
    if let Some(account_data_size) = get_token_2022_known_account_data_size(mint)? {
        return Ok(account_data_size);
    }

    // Fall back to CPI for everything else
    get_account_data_size_cpi(mint, token_program)
}

#[inline(always)]
fn get_token_2022_known_account_data_size(mint: &AccountView) -> Result<Option<u64>, ProgramError> {
    let mint_data = mint.try_borrow()?;
    let Ok(state) = StateWithExtensions::<Mint>::from_bytes(&mint_data) else {
        return Ok(None);
    };

    // Short-circuit only when the mint has exactly one extension we recognize.
    // Multi-extension, variable length, or unknown types fallback to CPI.
    let Some(extension_type) = single_mint_extension(&state) else {
        return Ok(None);
    };

    match extension_type {
        ExtensionType::TransferFeeConfig => Ok(Some(TOKEN_2022_TRANSFER_FEE_ACCOUNT_DATA_SIZE)),
        ExtensionType::NonTransferable => Ok(Some(TOKEN_2022_NON_TRANSFERABLE_ACCOUNT_DATA_SIZE)),
        ExtensionType::TransferHook => Ok(Some(TOKEN_2022_TRANSFER_HOOK_ACCOUNT_DATA_SIZE)),
        ExtensionType::Pausable => Ok(Some(TOKEN_2022_PAUSABLE_ACCOUNT_DATA_SIZE)),
        ExtensionType::MintCloseAuthority
        | ExtensionType::ConfidentialTransferMint
        | ExtensionType::DefaultAccountState
        | ExtensionType::InterestBearingConfig
        | ExtensionType::PermanentDelegate
        | ExtensionType::ConfidentialTransferFeeConfig
        | ExtensionType::MetadataPointer
        | ExtensionType::GroupPointer
        | ExtensionType::TokenGroup
        | ExtensionType::GroupMemberPointer
        | ExtensionType::TokenGroupMember
        | ExtensionType::ConfidentialMintBurn
        // Known to Token-2022 but not yet to the pinned Pinocchio `ExtensionType`.
        // Uncomment when the Pinocchio pin bumps to recognize them so the short-circuit
        // can cover the case instead of falling through to CPI.
        // |   ExtensionType::PermissionedBurn
        | ExtensionType::ScaledUiAmount => Ok(Some(TOKEN_2022_BASE_ACCOUNT_DATA_SIZE)),
        _ => Ok(None),
    }
}

/// Returns the single extension type present on the mint or `None` if there
/// are zero, multiple, or unparseable extensions.
#[inline(always)]
fn single_mint_extension(state: &StateWithExtensions<'_, Mint>) -> Option<ExtensionType> {
    // `get_extension_types()` overwrites slot 0 on success
    let mut buf = [ExtensionType::Uninitialized; 1];

    // Three outcomes:
    //  - `Ok(0)`: no extensions present
    //  - `Ok(1)`: exactly one extension (returns it)
    //  - `Err(_)`: 2+ extensions (AccountDataTooSmall) or malformed
    match state.get_extension_types(&mut buf) {
        Ok(1) => Some(buf[0]),
        _ => None,
    }
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
            if return_data.program_id() != token_program_address {
                log!("Error: return data came from unexpected program");
                return Err(ProgramError::IncorrectProgramId);
            }

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
