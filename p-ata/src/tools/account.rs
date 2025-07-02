use {
    pinocchio::{
        account_info::AccountInfo,
        instruction::{Seed, Signer},
        pubkey::Pubkey,
        sysvars::rent::Rent,
        ProgramResult,
    },
    pinocchio_system::instructions::{Assign, CreateAccount},
    spl_token_interface::state::{account::Account as TokenAccount, Transmutable},
};

#[cfg(feature = "create-account-prefunded")]
use pinocchio_system::instructions::CreateAccountPrefunded;

#[cfg(not(feature = "create-account-prefunded"))]
use pinocchio_system::instructions::{Allocate, Transfer};

/// Stamp the ImmutableOwner extension header into an account's data buffer.
#[inline(always)]
fn stamp_immutable_owner_extension(account: &AccountInfo, space: usize) -> ProgramResult {
    // Only stamp if we have enough space for the extension
    if space > TokenAccount::LEN {
        let mut data = account.try_borrow_mut_data()?;
        let base = TokenAccount::LEN; // 165

        // Write ImmutableOwner TLV header (type=6, len=0)
        // ImmutableOwner extension type is 6
        let tag: u16 = 6;
        data[base..base + 2].copy_from_slice(&tag.to_le_bytes()); // type
        data[base + 2..base + 4].copy_from_slice(&0u16.to_le_bytes()); // len = 0
        data[base + 4..base + 8].copy_from_slice(&0u32.to_le_bytes()); // sentinel
    }
    Ok(())
}

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

    // spl_token_interface::program::ID is the original SPL Token: TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA
    // Token-2022 program ID is: TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb
    const TOKEN_2022_PROGRAM_ID: Pubkey =
        pinocchio_pubkey::pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
    // System program ID: 11111111111111111111111111111111
    const SYSTEM_PROGRAM_ID: Pubkey = pinocchio_pubkey::pubkey!("11111111111111111111111111111111");
    let is_token_2022_account = *target_program_owner == TOKEN_2022_PROGRAM_ID;
    let should_stamp_immutable_owner = is_token_2022_account && space > TokenAccount::LEN;

    if current_lamports > 0 {
        #[cfg(feature = "create-account-prefunded")]
        {
            CreateAccountPrefunded {
                from: payer,
                to: pda,
                lamports: rent.minimum_balance(space).max(1),
                space: space as u64,
                owner: target_program_owner,
            }
            .invoke_signed(&[signer])?;

            // Stamp ImmutableOwner extension for token accounts with extensions
            if should_stamp_immutable_owner {
                stamp_immutable_owner_extension(pda, space)?;
            }
        }
        #[cfg(not(feature = "create-account-prefunded"))]
        {
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

                // Stamp ImmutableOwner extension after allocation but before assign
                if should_stamp_immutable_owner {
                    stamp_immutable_owner_extension(pda, space)?;
                }
            }

            if unsafe { pda.owner() } != target_program_owner {
                Assign {
                    account: pda,
                    owner: target_program_owner,
                }
                .invoke_signed(&[signer.clone()])?;
            }
        }
    } else {
        // Create as system-owned first if we need to stamp extension data, otherwise create with target owner
        let initial_owner = if should_stamp_immutable_owner {
            &SYSTEM_PROGRAM_ID
        } else {
            target_program_owner
        };

        CreateAccount {
            from: payer,
            to: pda,
            lamports: rent.minimum_balance(space).max(1),
            space: space as u64,
            owner: initial_owner,
        }
        .invoke_signed(&[signer.clone()])?;

        if should_stamp_immutable_owner {
            // Stamp ImmutableOwner extension after creation but before assigning to token program
            stamp_immutable_owner_extension(pda, space)?;

            // Now assign to the token program
            Assign {
                account: pda,
                owner: target_program_owner,
            }
            .invoke_signed(&[signer])?;
        }
    }
    Ok(())
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

        let target_program_owner = Pubkey::default();
        let rent = Rent::default();
        let space = 100;
        let seeds_too_few: &[&[u8]] = &[&[1], &[2], &[3]];

        let _ = create_pda_account(
            &payer_account,
            &rent,
            space,
            &target_program_owner,
            &acct_account,
            seeds_too_few,
        );
    }
}
