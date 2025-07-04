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

const IMMUTABLE_OWNER_HEADER: [u8; 8] = [
    6, 0, // type = 6 (ImmutableOwner) in little-endian
    0, 0, // length
    0, 0, 0, 0, // padding
];

const TOKEN_2022_PROGRAM_ID: Pubkey =
    pinocchio_pubkey::pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
const SYSTEM_PROGRAM_ID: Pubkey = pinocchio_pubkey::pubkey!("11111111111111111111111111111111");

/// Stamp the ImmutableOwner extension header into an account's data buffer.
#[inline(always)]
fn stamp_immutable_owner_extension(account: &AccountInfo) -> ProgramResult {
    let mut data = account.try_borrow_mut_data()?;
    let base = TokenAccount::LEN; // 165
    data[base..base + 8].copy_from_slice(&IMMUTABLE_OWNER_HEADER);
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

    let should_stamp_immutable_owner = *target_program_owner == TOKEN_2022_PROGRAM_ID;
    if should_stamp_immutable_owner {
        // Tell compiler this is always false for Token-2022 ATA accounts
        // Saves 39 CUs on non-2022 create paths.
        if space <= TokenAccount::LEN {
            unsafe { core::hint::unreachable_unchecked() }
        }
    }

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
                stamp_immutable_owner_extension(pda)?;
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
                    stamp_immutable_owner_extension(pda)?;
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
            stamp_immutable_owner_extension(pda)?;

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
