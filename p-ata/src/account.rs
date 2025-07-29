use {
    pinocchio::{
        account_info::AccountInfo,
        instruction::{Seed, Signer},
        pubkey::Pubkey,
        sysvars::rent::Rent,
        ProgramResult,
    },
    pinocchio_system::instructions::CreateAccount,
};

#[cfg(feature = "create-prefunded-account")]
use pinocchio_system::instructions::CreatePrefundedAccount;

#[cfg(not(feature = "create-prefunded-account"))]
use pinocchio_system::instructions::{Allocate, Assign, Transfer};

/// Create a PDA account, given:
/// - payer: Account to deduct SOL from
/// - rent: Rent sysvar account
/// - space: size of the data field
/// - owner: the program that will own the new account
/// - pda: the address of the account to create (pre-derived by the caller)
/// - pda_signer_seeds: full seed slice including the bump (wallet, token_program, mint, bump)
#[inline(always)]
pub(crate) fn create_pda_account(
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

    let required_lamports = rent.minimum_balance(space).max(1);

    if current_lamports > 0 {
        #[cfg(feature = "create-prefunded-account")]
        {
            CreatePrefundedAccount {
                from: payer,
                to: pda,
                lamports: required_lamports.saturating_sub(current_lamports),
                space: space as u64,
                owner: target_program_owner,
            }
            .invoke_signed(&[signer])?;
        }
        #[cfg(not(feature = "create-prefunded-account"))]
        {
            if required_lamports > current_lamports {
                Transfer {
                    from: payer,
                    to: pda,
                    lamports: required_lamports - current_lamports,
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
                owner: target_program_owner,
            }
            .invoke_signed(&[signer.clone()])?;
        }
    } else {
        CreateAccount {
            from: payer,
            to: pda,
            lamports: required_lamports,
            space: space as u64,
            owner: target_program_owner,
        }
        .invoke_signed(&[signer])?;
    }
    Ok(())
}
