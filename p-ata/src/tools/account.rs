use {
    pinocchio::{
        account_info::AccountInfo,
        instruction::{AccountMeta, Instruction, Seed, Signer},
        program::{invoke, get_return_data},
        program_error::ProgramError,
        pubkey::Pubkey,
        sysvars::rent::Rent,
        ProgramResult,
    },
    pinocchio_system::instructions::{Allocate, Assign, CreateAccount, Transfer},
};

/// Create a PDA account, given:
/// - payer: Account to deduct SOL from
/// - rent: Rent sysvar account
/// - space: size of the data field
/// - owner: the program that will own the new account
/// - pda: the address of the account to create (pre-derived by the caller)
/// - pda_signer_seeds: seeds (without the bump already appended), needed for invoke_signed
pub fn create_pda_account(
    payer: &AccountInfo,
    rent: &Rent,
    space: usize,
    owner: &Pubkey,
    pda: &AccountInfo,
    pda_signer_seeds: &[&[u8]],
) -> ProgramResult {
    let current_lamports = pda.lamports();

    // Convert seeds to Seed array - assuming we always have 4 seeds for PDAs in this program
    assert_eq!(pda_signer_seeds.len(), 4, "Expected 4 seeds for PDA");
    let seed_array: [Seed; 4] = [
        Seed::from(pda_signer_seeds[0]),
        Seed::from(pda_signer_seeds[1]),
        Seed::from(pda_signer_seeds[2]),
        Seed::from(pda_signer_seeds[3]),
    ];
    let signer = Signer::from(&seed_array);

    if current_lamports > 0 {
        let required_lamports = rent.minimum_balance(space).max(1); // make sure balance is at least 1

        if required_lamports > current_lamports {
            let transfer_amount = required_lamports - current_lamports;

            Transfer {
                from: payer,
                to: pda,
                lamports: transfer_amount,
            }
            .invoke()?;
        }

        Allocate {
            account: pda,
            space: space as u64,
        }
        .invoke_signed(&[signer.clone()])?;

        Assign {
            account: pda,
            owner,
        }
        .invoke_signed(&[signer.clone()])?;
    } else {
        CreateAccount {
            from: payer,
            to: pda,
            lamports: rent.minimum_balance(space).max(1),
            space: space as u64,
            owner,
        }
        .invoke_signed(&[signer])?;
    }
    Ok(())
}

/// Determines the required initial data length for a new token account based on
/// the extensions initialized on the Mint
pub fn get_account_len(
    mint: &AccountInfo,
    token_program: &AccountInfo,
) -> Result<usize, ProgramError> {
    // Instruction data for GetAccountDataSize (discriminator 21) with ImmutableOwner extension
    // Format: [discriminator (1 byte), extension_type (2 bytes)]
    // ImmutableOwner extension type = 7 (as u16 little-endian)
    let get_size_data = [21u8, 7u8, 0u8]; // 21 = discriminator, [7, 0] = ImmutableOwner as u16 LE
    
    let get_size_metas = &[
        AccountMeta {
            pubkey: mint.key(),
            is_writable: false,
            is_signer: false,
        },
    ];

    let get_size_ix = Instruction {
        program_id: token_program.key(),
        accounts: get_size_metas,
        data: &get_size_data,
    };

    invoke(&get_size_ix, &[mint])?;
    
    get_return_data()
        .ok_or(ProgramError::InvalidInstructionData)
        .and_then(|return_data| {
            if return_data.program_id() != token_program.key() {
                return Err(ProgramError::IncorrectProgramId);
            }
            if return_data.as_slice().len() != 8 {
                return Err(ProgramError::InvalidInstructionData);
            }
            // Convert little-endian u64 to usize
            let size_bytes: [u8; 8] = return_data.as_slice().try_into().map_err(|_| ProgramError::InvalidInstructionData)?;
            Ok(usize::from_le_bytes(size_bytes))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio::{account_info::AccountInfo, pubkey::Pubkey, sysvars::rent::Rent};

    #[test]
    #[should_panic(expected = "Expected 4 seeds for PDA")]
    fn test_create_pda_account_panic_on_invalid_seed_length() {
        // For this panic test, AccountInfo contents are not dereferenced before the seed length check.
        // Using uninitialized AccountInfo via MaybeUninit to satisfy type signatures.
        #[allow(invalid_value)]
        let payer_account: AccountInfo = unsafe { core::mem::MaybeUninit::uninit().assume_init() };
        #[allow(invalid_value)]
        let acct_account: AccountInfo = unsafe { core::mem::MaybeUninit::uninit().assume_init() };

        let owner_key = Pubkey::default();
        let rent = Rent::default();
        let space = 100;
        let seeds_too_few: &[&[u8]] = &[&[1], &[2], &[3]];

        let _ = create_pda_account(
            &payer_account,
            &rent,
            space,
            &owner_key,
            &acct_account,
            seeds_too_few,
        );
    }
}
