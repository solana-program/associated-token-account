use {
    pinocchio::{
        AccountView,
        cpi::{self, get_return_data},
        error::ProgramError,
        instruction::{InstructionAccount, InstructionView},
    },
    pinocchio_log::log,
    pinocchio_token_2022::{
        instructions::GetAccountDataSize,
        state::{ExtensionType, try_calculate_account_len_from_mint},
    },
};

/// Get the required Token-2022 account data size when no account length hint
/// was supplied. Attempts to computes the size locally first and falls back to
/// `GetAccountDataSize` CPI for mints that have an extension this program doesn't recognize.
#[inline(always)]
pub(crate) fn get_token_2022_account_data_size(
    mint: &AccountView,
    token_program: &AccountView,
) -> Result<u64, ProgramError> {
    match try_calculate_account_len_from_mint(mint, &[ExtensionType::ImmutableOwner])? {
        Some(len) => Ok(len as u64),
        None => get_account_data_size_cpi(mint, token_program),
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
