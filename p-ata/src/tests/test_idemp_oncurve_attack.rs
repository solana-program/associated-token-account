use {
    crate::tests::test_utils::{
        build_create_ata_instruction, create_mollusk_token_account_data, CreateAtaInstructionType,
    },
    mollusk_svm::{program::loader_keys::LOADER_V3, result::Check, Mollusk},
    solana_program,
    solana_pubkey::Pubkey,
    solana_sdk::{
        account::Account, program_error::ProgramError, signature::Keypair, signer::Signer,
    },
    solana_sdk_ids::{system_program, sysvar},
    std::vec,
    std::vec::Vec,
};

/// Find a wallet where find_program_address returns canonical_bump,
/// meaning all bumps > canonical_bump are on-curve.
/// Then derive an on-curve address at canonical_bump + 1.
/// Returns: (wallet, canonical_address, on_curve_address_at_attack_bump)
fn find_wallet_with_on_curve_attack_opportunity(
    first_off_curve_bump: u8,
    token_program: &Pubkey,
    mint: &Pubkey,
    ata_program_id: &Pubkey,
) -> Option<(Pubkey, Pubkey, Pubkey, u8)> {
    const MAX_FIND_ATTEMPTS: u32 = 100_000;
    let attack_bump = first_off_curve_bump + 1;

    for _ in 0..MAX_FIND_ATTEMPTS {
        let wallet = Pubkey::new_unique();

        let (canonical_addr, found_bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()],
            ata_program_id,
        );

        // We need find_program_address to return exactly first_off_curve_bump
        // This means attack_bump and all higher bumps are on-curve
        if found_bump != first_off_curve_bump {
            continue;
        }

        // same logic as pinocchio_pubkey::derive_address::<3>
        let seeds: &[&[u8]; 3] = &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()];

        // Replicate pinocchio's derive_address logic exactly
        let bump_bytes = [attack_bump];
        let pda_marker = b"ProgramDerivedAddress"; // This is the PDA_MARKER constant

        let mut hasher = solana_program::hash::Hasher::default();
        for seed in seeds {
            hasher.hash(seed);
        }
        hasher.hash(&bump_bytes);
        hasher.hash(ata_program_id.as_ref());
        hasher.hash(pda_marker);

        let hash = hasher.result();
        let attack_addr = Pubkey::from(hash.to_bytes());
        return Some((wallet, canonical_addr, attack_addr, attack_bump));
    }
    None
}

/// Manually create a token account at a given address with proper token account data.
/// This simulates an attacker manually creating an account outside of the ATA program.
fn create_manual_token_account(mint: Pubkey, owner: Pubkey, token_program: Pubkey) -> Account {
    let token_account_data = create_mollusk_token_account_data(&mint, &owner, 0);

    Account {
        lamports: 2_039_280, // Enough lamports for a token account
        data: token_account_data,
        owner: token_program,
        executable: false,
        rent_epoch: 0,
    }
}

#[test]
fn test_rejects_on_curve_address_in_idempotent_check() {
    let ata_program_id = spl_associated_token_account::id();
    let token_program_id = spl_token::id();
    let mint_pubkey = Pubkey::new_unique();
    let payer = Keypair::new();

    // Find a wallet where find_program_address returns bump 253
    // This means bump 254 and 255 are both on curve
    let first_off_curve_bump = 253u8;

    let (wallet, _, on_curve_attack_address, attack_bump) =
        match find_wallet_with_on_curve_attack_opportunity(
            first_off_curve_bump,
            &token_program_id,
            &mint_pubkey,
            &ata_program_id,
        ) {
            Some(result) => result,
            None => {
                panic!(
                    "Could not find wallet with canonical bump {} and on-curve attack opportunity",
                    first_off_curve_bump
                );
            }
        };

    let mut mollusk = Mollusk::default();
    mollusk.add_program(
        &ata_program_id,
        "target/deploy/pinocchio_ata_program",
        &LOADER_V3,
    );
    mollusk.add_program(
        &token_program_id,
        "programs/token/target/deploy/pinocchio_token_program",
        &LOADER_V3,
    );

    // Step 1: Manually create a token account at the on-curve address
    // This simulates the attack where someone creates an account at an on-curve (invalid PDA) address
    let manual_token_account = create_manual_token_account(mint_pubkey, wallet, token_program_id);

    let accounts = vec![
        (
            payer.pubkey(),
            Account::new(1_000_000_000, 0, &system_program::id()),
        ),
        (on_curve_attack_address, manual_token_account), // Pre-existing account at on-curve address
        (wallet, Account::new(0, 0, &system_program::id())),
        (
            mint_pubkey,
            Account {
                lamports: 1_461_600,
                data: crate::tests::test_utils::create_mollusk_mint_data(6),
                owner: token_program_id,
                executable: false,
                rent_epoch: 0,
            },
        ),
        (
            system_program::id(),
            Account {
                lamports: 0,
                data: Vec::new(),
                owner: crate::tests::test_utils::NATIVE_LOADER_ID,
                executable: true,
                rent_epoch: 0,
            },
        ),
        (
            token_program_id,
            Account {
                lamports: 0,
                data: Vec::new(),
                owner: LOADER_V3,
                executable: true,
                rent_epoch: 0,
            },
        ),
        (sysvar::rent::id(), Account::new(1009200, 17, &sysvar::id())),
    ];

    // Step 2: Try to validate the account with CreateIdempotent using the on-curve address
    let idempotent_instruction = build_create_ata_instruction(
        ata_program_id,
        payer.pubkey(),
        on_curve_attack_address,
        wallet,
        mint_pubkey,
        token_program_id,
        CreateAtaInstructionType::CreateIdempotent {
            bump: Some(attack_bump), // The on-curve bump we're attacking with
        },
    );

    // This should fail with InvalidSeeds because the address is on-curve (invalid PDA)
    // The is_off_curve check in check_idempotent_account prevents this attack
    mollusk.process_and_validate_instruction(
        &idempotent_instruction,
        &accounts,
        &[Check::err(ProgramError::InvalidSeeds)],
    );
}
