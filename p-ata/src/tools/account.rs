use {
    pinocchio::{
        account_info::AccountInfo,
        instruction::{Seed, Signer},
        program_error::ProgramError,
        pubkey::Pubkey,
        sysvars::rent::Rent,
        ProgramResult,
    },
    pinocchio_system::instructions::CreateAccount,
};

#[cfg(feature = "create-account-prefunded")]
use pinocchio_system::instructions::CreateAccountPrefunded;

#[cfg(not(feature = "create-account-prefunded"))]
use pinocchio_system::instructions::{Allocate, Assign, Transfer};

/// Create a PDA account, given:
/// - payer: Account to deduct SOL from
/// - rent: Rent sysvar account
/// - space: size of the data field
/// - owner: the program that will own the new account
/// - pda: the address of the account to create (pre-derived by the caller)
/// - pda_signer_seeds: full seed slice including the bump (wallet, token_program, mint, bump)
#[inline(always)]
pub fn create_pda_account(
    payer: &AccountInfo,
    rent: &Rent,
    space: usize,
    target_program_owner: &Pubkey,
    pda: &AccountInfo,
    pda_signer_seeds: &[&[u8]],
) -> ProgramResult {
    let current_lamports = pda.lamports();

    debug_assert!(pda_signer_seeds.len() == 4, "Expected 4 seeds for PDA");
    let seed_array: [Seed; 4] = [
        Seed::from(pda_signer_seeds[0]),
        Seed::from(pda_signer_seeds[1]),
        Seed::from(pda_signer_seeds[2]),
        Seed::from(pda_signer_seeds[3]),
    ];
    let signer = Signer::from(&seed_array);

    if current_lamports > 0 {
        let required_lamports = rent.minimum_balance(space).max(1);
        #[cfg(feature = "create-account-prefunded")]
        {
            CreateAccountPrefunded {
                from: payer,
                to: pda,
                lamports: required_lamports.saturating_sub(current_lamports),
                space: space as u64,
                owner: target_program_owner,
            }
            .invoke_signed(&[signer])?;
        }
        #[cfg(not(feature = "create-account-prefunded"))]
        {
            if required_lamports > current_lamports {
                Transfer {
                    from: payer,
                    to: pda,
                    lamports: required_lamports.saturating_sub(current_lamports),
                }
                .invoke()?;
            }

            let current_data_len = pda.data_len();
            let current_owner = unsafe { pda.owner() };

            if current_data_len != space {
                Allocate {
                    account: pda,
                    space: space as u64,
                }
                .invoke_signed(&[signer.clone()])?;
            } else if current_data_len > 0 {
                // Allocate ensures account is empty
                return Err(ProgramError::AccountAlreadyInitialized);
            }

            if current_owner != target_program_owner {
                Assign {
                    account: pda,
                    owner: target_program_owner,
                }
                .invoke_signed(&[signer.clone()])?;
            }
        }
    } else {
        // Create account directly with target owner
        CreateAccount {
            from: payer,
            to: pda,
            lamports: rent.minimum_balance(space).max(1),
            space: space as u64,
            owner: target_program_owner,
        }
        .invoke_signed(&[signer])?;
    }
    Ok(())
}
