//! Core account creation utilities for Program Derived Addresses (PDAs).
//!
//! This is the only code that uses the `CreatePrefundedAccount` instruction.
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

/// Create a Program Derived Address (PDA) account.
///
/// ## Arguments
///
/// * `payer` - Account to deduct SOL from (must be signer and writable)
/// * `rent` - Rent sysvar reference for minimum balance calculation  
/// * `space` - Size of the account data field in bytes
/// * `target_program_owner` - Program that will own the new account
/// * `pda` - Pre-derived PDA address (account to create)
/// * `pda_signer_seeds` - Exactly 4 seeds [wallet, token_program, mint, bump]
///
/// ## Behavior
///
/// - **Account has 0 lamports**: Uses `CreateAccount` instruction
/// - **Account has >0 lamports**: Uses `CreatePrefundedAccount` instruction
/// - **Account has >0 lamports (legacy)**: Uses `Transfer` + `Allocate` + `Assign` sequence
///
/// `pda_signer_seeds` must correctly derive `pda`, and `pda` must be empty
/// and owned by the system program, or the system program instructions called
/// by this function will fail.
#[inline(always)]
pub(crate) fn create_pda_account(
    payer: &AccountInfo,
    rent: &Rent,
    space: usize,
    target_program_owner: &Pubkey,
    pda: &AccountInfo,
    pda_signer_seeds: &[Seed; 4],
) -> ProgramResult {
    let current_lamports = pda.lamports();

    let signer = Signer::from(pda_signer_seeds);

    let required_lamports = rent.minimum_balance(space);

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
            .invoke_signed(&[signer])
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
            .invoke_signed(&[signer])
        }
    } else {
        CreateAccount {
            from: payer,
            to: pda,
            lamports: required_lamports,
            space: space as u64,
            owner: target_program_owner,
        }
        .invoke_signed(&[signer])
    }
}
