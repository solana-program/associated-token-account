use {
    pinocchio::{
        account_info::AccountInfo,
        instruction::{AccountMeta, Instruction, Seed, Signer},
        program::invoke,
        program_error::ProgramError,
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
        let initial_owner = target_program_owner;

        CreateAccount {
            from: payer,
            to: pda,
            lamports: rent.minimum_balance(space).max(1),
            space: space as u64,
            owner: initial_owner,
        }
        .invoke_signed(&[signer.clone()])?;
    }
    Ok(())
}

#[inline(always)]
pub fn get_account_len(
    mint: &AccountInfo,
    _token_program: &AccountInfo,
) -> Result<usize, ProgramError> {
    let _ = mint; // Suppress unused warning in no-std build.
    Ok(TokenAccount::LEN)
}

/// Conditionally initialize the ImmutableOwner extension for a freshly created
/// Token2022 account (no-op if not Token2022).
#[inline(always)]
pub fn initialize_immutable_owner(
    token_program: &AccountInfo,
    ata_account: &AccountInfo,
) -> ProgramResult {
    // Token-2022 program ID.
    const TOKEN_2022_PROGRAM_ID: Pubkey =
        pinocchio_pubkey::pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");

    if token_program.key() != &TOKEN_2022_PROGRAM_ID {
        return Ok(());
    }

    // Discriminator for TokenInstruction::InitializeImmutableOwner (22).
    let ix_data = [22u8];
    let metas = &[AccountMeta {
        pubkey: ata_account.key(),
        is_writable: true,
        is_signer: false,
    }];

    let ix = Instruction {
        program_id: token_program.key(),
        accounts: metas,
        data: &ix_data,
    };

    invoke(&ix, &[ata_account])
}
