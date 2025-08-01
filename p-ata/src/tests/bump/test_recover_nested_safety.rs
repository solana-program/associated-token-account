use {
    super::test_bump_utils::{
        find_wallet_with_non_canonical_opportunity, setup_mollusk_for_bump_tests,
    },
    crate::tests::{account_builder::AccountBuilder, test_utils::NATIVE_LOADER_ID},
    mollusk_svm::{program::loader_keys::LOADER_V3, result::Check},
    solana_program,
    solana_pubkey::Pubkey,
    solana_sdk::{
        account::Account,
        instruction::{AccountMeta, Instruction},
        program_error::ProgramError,
        signature::Keypair,
        signer::Signer,
    },
    solana_sdk_ids::{system_program, sysvar},
    std::{vec, vec::Vec},
};

/// Helper function to manually derive a PDA address with specific bump
fn derive_address_with_bump(seeds: &[&[u8]; 3], bump: u8, program_id: &Pubkey) -> Pubkey {
    let mut hasher = solana_program::hash::Hasher::default();
    for seed in seeds {
        hasher.hash(seed);
    }
    hasher.hash(&[bump]);
    hasher.hash(program_id.as_ref());
    hasher.hash(b"ProgramDerivedAddress");
    Pubkey::from(hasher.result().to_bytes())
}

/// Helper function to derive ATA address
fn derive_ata_address(
    wallet: &Pubkey,
    token_program: &Pubkey,
    mint: &Pubkey,
    ata_program_id: &Pubkey,
) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()],
        ata_program_id,
    )
}

/// Find a wallet where the destination ATA can have an on-curve address at a specific bump
/// Returns: (wallet, owner_mint, nested_mint, canonical_bump, on_curve_bump)
fn find_wallet_with_on_curve_opportunity(
    first_off_curve_bump: u8,
    token_program: &Pubkey,
    ata_program_id: &Pubkey,
) -> Option<(Pubkey, Pubkey, Pubkey, u8)> {
    const MAX_FIND_ATTEMPTS: u32 = 50_000;
    let attack_bump = first_off_curve_bump + 1;

    for _ in 0..MAX_FIND_ATTEMPTS {
        let wallet = Pubkey::new_unique();
        let owner_mint = Pubkey::new_unique();
        let nested_mint = Pubkey::new_unique();

        let (_, found_bump) =
            derive_ata_address(&wallet, token_program, &nested_mint, ata_program_id);

        // We need find_program_address to return exactly first_off_curve_bump
        if found_bump != first_off_curve_bump {
            continue;
        }

        return Some((wallet, owner_mint, nested_mint, attack_bump));
    }
    None
}

/// Build a RecoverNested instruction with provided bumps
fn build_recover_nested_instruction(
    ata_program_id: Pubkey,
    nested_ata: Pubkey,
    nested_mint: Pubkey,
    destination_ata: Pubkey,
    owner_ata: Pubkey,
    owner_mint: Pubkey,
    wallet: Pubkey,
    token_program: Pubkey,
    owner_bump: u8,
    nested_bump: u8,
    destination_bump: u8,
) -> Instruction {
    Instruction {
        program_id: ata_program_id,
        accounts: vec![
            AccountMeta::new(nested_ata, false),
            AccountMeta::new_readonly(nested_mint, false),
            AccountMeta::new(destination_ata, false),
            AccountMeta::new_readonly(owner_ata, false),
            AccountMeta::new_readonly(owner_mint, false),
            AccountMeta::new(wallet, true),
            AccountMeta::new_readonly(token_program, false),
        ],
        data: vec![2, owner_bump, nested_bump, destination_bump], // RecoverNested with bumps
    }
}

/// Helper to create mint account
fn create_mint_account(lamports: u64, token_program: Pubkey) -> Account {
    Account {
        lamports,
        data: AccountBuilder::mint(6, &token_program).data,
        owner: token_program,
        executable: false,
        rent_epoch: 0,
    }
}

/// Helper to create token account
fn create_token_account(
    mint: &Pubkey,
    owner: &Pubkey,
    amount: u64,
    token_program: Pubkey,
) -> Account {
    Account {
        lamports: 2_039_280,
        data: AccountBuilder::token_account(mint, owner, amount, &spl_token::id()).data,
        owner: token_program,
        executable: false,
        rent_epoch: 0,
    }
}

/// Helper to create program account
fn create_program_account() -> Account {
    Account {
        lamports: 0,
        data: Vec::new(),
        owner: LOADER_V3,
        executable: true,
        rent_epoch: 0,
    }
}

/// Create the account setup for RecoverNested tests
#[allow(clippy::too_many_arguments)]
fn create_recover_nested_accounts(
    payer: Pubkey,
    wallet: Pubkey,
    owner_mint: Pubkey,
    nested_mint: Pubkey,
    owner_ata: Pubkey,
    nested_ata: Pubkey,
    destination_ata: Pubkey,
    token_program: Pubkey,
    ata_program_id: Pubkey,
) -> Vec<(Pubkey, Account)> {
    vec![
        (payer, Account::new(1_000_000_000, 0, &system_program::id())),
        (wallet, Account::new(0, 0, &system_program::id())),
        (owner_mint, create_mint_account(1_461_600, token_program)),
        (nested_mint, create_mint_account(1_461_600, token_program)),
        (
            owner_ata,
            create_token_account(&owner_mint, &wallet, 0, token_program),
        ),
        (
            nested_ata,
            create_token_account(&nested_mint, &owner_ata, 100, token_program),
        ), // Has tokens to recover
        (
            destination_ata,
            create_token_account(&nested_mint, &wallet, 0, token_program),
        ),
        (
            system_program::id(),
            Account {
                lamports: 0,
                data: Vec::new(),
                owner: NATIVE_LOADER_ID,
                executable: true,
                rent_epoch: 0,
            },
        ),
        (token_program, create_program_account()),
        (ata_program_id, create_program_account()),
        (sysvar::rent::id(), Account::new(1009200, 17, &sysvar::id())),
    ]
}

#[test]
fn test_recover_nested_rejects_non_canonical_destination_bump() {
    let ata_program_id = spl_associated_token_account::id();
    let token_program_id = spl_token::id();
    let payer = Keypair::new();

    // Find a scenario with non-canonical bump opportunity
    let (wallet, owner_mint, nested_mint, canonical_bump, non_canonical_bump) =
        find_wallet_with_non_canonical_opportunity(&token_program_id, &ata_program_id)
            .expect("Could not find wallet with non-canonical bump opportunity for testing");

    // Derive the ATAs
    let (owner_ata, owner_bump) =
        derive_ata_address(&wallet, &token_program_id, &owner_mint, &ata_program_id);
    let (nested_ata, nested_bump) =
        derive_ata_address(&owner_ata, &token_program_id, &nested_mint, &ata_program_id);
    let (canonical_destination_ata, _) =
        derive_ata_address(&wallet, &token_program_id, &nested_mint, &ata_program_id);

    // Derive NON-CANONICAL destination ATA using lower bump
    let non_canonical_destination_ata = derive_address_with_bump(
        &[
            wallet.as_ref(),
            token_program_id.as_ref(),
            nested_mint.as_ref(),
        ],
        non_canonical_bump,
        &ata_program_id,
    );

    let mollusk = setup_mollusk_for_bump_tests(&token_program_id);

    // Test with CANONICAL destination (should succeed)
    let canonical_accounts = create_recover_nested_accounts(
        payer.pubkey(),
        wallet,
        owner_mint,
        nested_mint,
        owner_ata,
        nested_ata,
        canonical_destination_ata,
        token_program_id,
        ata_program_id,
    );

    let canonical_instruction = build_recover_nested_instruction(
        ata_program_id,
        nested_ata,
        nested_mint,
        canonical_destination_ata,
        owner_ata,
        owner_mint,
        wallet,
        token_program_id,
        owner_bump,
        nested_bump,
        canonical_bump,
    );

    // This should succeed with canonical bump
    mollusk.process_and_validate_instruction(
        &canonical_instruction,
        &canonical_accounts,
        &[Check::success()],
    );

    // Test with NON-CANONICAL destination (should fail)
    let non_canonical_accounts = create_recover_nested_accounts(
        payer.pubkey(),
        wallet,
        owner_mint,
        nested_mint,
        owner_ata,
        nested_ata,
        non_canonical_destination_ata,
        token_program_id,
        ata_program_id,
    );

    let non_canonical_instruction = build_recover_nested_instruction(
        ata_program_id,
        nested_ata,
        nested_mint,
        non_canonical_destination_ata,
        owner_ata,
        owner_mint,
        wallet,
        token_program_id,
        owner_bump,
        nested_bump,
        non_canonical_bump,
    );

    mollusk.process_and_validate_instruction(
        &non_canonical_instruction,
        &non_canonical_accounts,
        &[Check::err(ProgramError::InvalidSeeds)],
    );
}

#[test]
fn test_recover_nested_rejects_on_curve_destination_address() {
    let ata_program_id = spl_associated_token_account::id();
    let token_program_id = spl_token::id();
    let payer = Keypair::new();

    // Find a wallet with on-curve attack opportunity
    let first_off_curve_bump = 253u8;
    let (wallet, owner_mint, nested_mint, attack_bump) = find_wallet_with_on_curve_opportunity(
        first_off_curve_bump,
        &token_program_id,
        &ata_program_id,
    )
    .expect("Could not find wallet with on-curve attack opportunity for testing");

    // Derive the ATAs
    let (owner_ata, owner_bump) =
        derive_ata_address(&wallet, &token_program_id, &owner_mint, &ata_program_id);
    let (nested_ata, nested_bump) =
        derive_ata_address(&owner_ata, &token_program_id, &nested_mint, &ata_program_id);
    let (canonical_destination_ata, canonical_dest_bump) =
        derive_ata_address(&wallet, &token_program_id, &nested_mint, &ata_program_id);

    // derive the on-curve attack address
    let on_curve_destination_ata = derive_address_with_bump(
        &[
            wallet.as_ref(),
            token_program_id.as_ref(),
            nested_mint.as_ref(),
        ],
        attack_bump,
        &ata_program_id,
    );

    let mollusk = setup_mollusk_for_bump_tests(&token_program_id);

    let accounts = create_recover_nested_accounts(
        payer.pubkey(),
        wallet,
        owner_mint,
        nested_mint,
        owner_ata,
        nested_ata,
        canonical_destination_ata,
        token_program_id,
        ata_program_id,
    );

    // First verify normal operation works with canonical address
    let good_instruction = build_recover_nested_instruction(
        ata_program_id,
        nested_ata,
        nested_mint,
        canonical_destination_ata,
        owner_ata,
        owner_mint,
        wallet,
        token_program_id,
        owner_bump,
        nested_bump,
        canonical_dest_bump,
    );

    mollusk.process_and_validate_instruction(&good_instruction, &accounts, &[Check::success()]);

    // Now try with on-curve destination address - should fail
    let mut bad_accounts = accounts.clone();
    bad_accounts
        .iter_mut()
        .find(|(pubkey, _)| *pubkey == canonical_destination_ata)
        .unwrap()
        .0 = on_curve_destination_ata;

    let bad_instruction = build_recover_nested_instruction(
        ata_program_id,
        nested_ata,
        nested_mint,
        on_curve_destination_ata,
        owner_ata,
        owner_mint,
        wallet,
        token_program_id,
        owner_bump,
        nested_bump,
        attack_bump,
    );

    mollusk.process_and_validate_instruction(
        &bad_instruction,
        &bad_accounts,
        &[Check::err(ProgramError::InvalidSeeds)],
    );
}
