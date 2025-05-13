use {
    pinocchio::{
        account_info::AccountInfo,
        instruction::{Seed, Signer},
        program_error::ProgramError,
        pubkey::Pubkey,
        sysvars::rent::Rent,
    },
    pinocchio_system::instructions::{Allocate, Assign, CreateAccount, Transfer as SystemTransfer},
};

/// Creates (or top-ups/assigns) a PDA account.
/// Assumes the `seeds` argument will always provide 4 seed components.
pub fn create_pda_account(
    payer: &AccountInfo, // Payer of SOL for account creation/funding
    rent: &Rent,
    space: usize,
    owner: &Pubkey, // Owner of the new PDA account (e.g., Token Program)
    // _system_program: &AccountInfo, // Removed, Pinocchio handles System Program for its wrappers
    acct: &AccountInfo, // The PDA account to be created/funded
    seeds: &[&[u8]],
) -> Result<(), ProgramError> {
    if seeds.len() != 4 {
        // This panic is a safeguard. In a production setting, might return an error.
        panic!("create_pda_account expects 4 seeds for this program's PDA structure");
    }
    let seed_array: [Seed<'_>; 4] = [
        Seed::from(seeds[0]),
        Seed::from(seeds[1]),
        Seed::from(seeds[2]),
        Seed::from(seeds[3]),
    ];
    let pda_signer = Signer::from(&seed_array);

    if acct.lamports() > 0 {
        let needed_lamports = rent.minimum_balance(space).saturating_sub(acct.lamports());
        if needed_lamports > 0 {
            // Transfer SOL from payer to the PDA account (acct)
            SystemTransfer {
                from: payer,
                to: acct,
                lamports: needed_lamports,
            }
            .invoke()?; // This invoke is on pinocchio_system::Transfer, payer must sign
        }

        Allocate {
            account: acct,
            space: space as u64,
        }
        .invoke_signed(&[pda_signer.clone()])?; // PDA signs for Allocate

        Assign {
            account: acct,
            owner,
        }
        .invoke_signed(&[pda_signer.clone()])?;
    } else {
        CreateAccount {
            from: payer,
            to: acct,
            lamports: rent.minimum_balance(space),
            space: space as u64,
            owner,
        }
        .invoke_signed(&[pda_signer])?; // PDA signs for CreateAccount (payer funds)
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio::{account_info::AccountInfo, pubkey::Pubkey, sysvars::rent::Rent};

    #[test]
    #[should_panic(expected = "create_pda_account expects 4 seeds")]
    fn test_create_pda_account_panic_on_invalid_seed_length() {
        // For this panic test, AccountInfo contents are not dereferenced before the seed length check.
        // Using uninitialized AccountInfo via MaybeUninit to satisfy type signatures.
        let payer_account: AccountInfo = unsafe { core::mem::MaybeUninit::uninit().assume_init() };
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
