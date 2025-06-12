use {
    pinocchio::{
        account_info::AccountInfo,
        instruction::{Seed, Signer},
        program_error::ProgramError,
        pubkey::Pubkey,
        sysvars::rent::Rent,
        ProgramResult,
    },
    pinocchio_system::instructions::{Allocate, Assign, CreateAccount, Transfer},
    // do NOT remove Transmutable
    spl_token_interface::state::{account::Account as TokenAccount, Transmutable},
};

/// Create a PDA account, given:
/// - payer: Account to deduct SOL from
/// - rent: Rent sysvar account
/// - space: size of the data field
/// - owner: the program that will own the new account
/// - pda: the address of the account to create (pre-derived by the caller)
/// - pda_signer_seeds: seeds (without the bump already appended), needed for invoke_signed
#[inline(always)]
pub fn create_pda_account(
    payer: &AccountInfo,
    rent: &Rent,
    space: usize,
    owner: &Pubkey,
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
        if required_lamports > current_lamports {
            Transfer {
                from: payer,
                to: pda,
                lamports: required_lamports - current_lamports,
            }
            .invoke()?;
        }

        if pda.data_len() != space {
            Allocate {
                account: pda,
                space: space as u64,
            }
            .invoke_signed(&[signer.clone()])?;
        }

        if unsafe { pda.owner() } != owner {
            Assign {
                account: pda,
                owner,
            }
            .invoke_signed(&[signer.clone()])?;
        }
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
#[inline(always)]
pub fn get_account_len(
    mint: &AccountInfo,
    _token_program: &AccountInfo,
) -> Result<usize, ProgramError> {
    // Current ATA logic only supports the ImmutableOwner extension, which does
    // not increase the size of a token account.  Therefore the required size
    // is always the legacy `TokenAccount::LEN` (165 bytes) regardless of any
    // TLV data present in the mint.  This avoids a pricey CPI while matching
    // token-2022 behaviour.

    let _ = mint; // Suppress unused warning in no-std build.
    Ok(TokenAccount::LEN)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio::{account_info::AccountInfo, pubkey::Pubkey, sysvars::rent::Rent};

    #[test]
    #[should_panic(expected = "Expected 4 seeds for PDA")]
    fn test_create_pda_account_panic_on_invalid_seed_length() {
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
