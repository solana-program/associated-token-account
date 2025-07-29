use {
    super::test_bump_utils::{
        find_wallet_with_non_canonical_opportunity, setup_mollusk_for_bump_tests,
    },
    crate::tests::test_utils::{
        create_mollusk_mint_data, create_mollusk_token_account_data, NATIVE_LOADER_ID,
    },
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
    std::vec,
    std::vec::Vec,
};

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

        let (_, found_bump) = Pubkey::find_program_address(
            &[
                wallet.as_ref(),
                token_program.as_ref(),
                nested_mint.as_ref(),
            ],
            ata_program_id,
        );

        // We need find_program_address to return exactly first_off_curve_bump
        if found_bump != first_off_curve_bump {
            continue;
        }

        // Manually derive the attack address
        let seeds: &[&[u8]; 3] = &[
            wallet.as_ref(),
            token_program.as_ref(),
            nested_mint.as_ref(),
        ];
        let mut hasher = solana_program::hash::Hasher::default();
        for seed in seeds {
            hasher.hash(seed);
        }
        hasher.hash(&[attack_bump]);
        hasher.hash(ata_program_id.as_ref());
        hasher.hash(b"ProgramDerivedAddress");

        return Some((wallet, owner_mint, nested_mint, attack_bump));
    }
    None
}

/// Simple off-curve check for testing (mirrors the logic in processor.rs)
fn is_off_curve_test(address: &Pubkey) -> bool {
    use curve25519_dalek::edwards::CompressedEdwardsY;

    let compressed = CompressedEdwardsY(address.to_bytes());
    match compressed.decompress() {
        None => true,                    // invalid encoding → off-curve
        Some(pt) => pt.is_small_order(), // small-order = off-curve, otherwise on-curve
    }
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
    let accounts = vec![
        AccountMeta::new(nested_ata, false),
        AccountMeta::new_readonly(nested_mint, false),
        AccountMeta::new(destination_ata, false),
        AccountMeta::new_readonly(owner_ata, false),
        AccountMeta::new_readonly(owner_mint, false),
        AccountMeta::new(wallet, true),
        AccountMeta::new_readonly(token_program, false),
    ];

    Instruction {
        program_id: ata_program_id,
        accounts,
        data: vec![2, owner_bump, nested_bump, destination_bump], // RecoverNested with bumps
    }
}

/// Create the account setup for RecoverNested tests
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
        (
            owner_mint,
            Account {
                lamports: 1_461_600,
                data: create_mollusk_mint_data(6),
                owner: token_program,
                executable: false,
                rent_epoch: 0,
            },
        ),
        (
            nested_mint,
            Account {
                lamports: 1_461_600,
                data: create_mollusk_mint_data(6),
                owner: token_program,
                executable: false,
                rent_epoch: 0,
            },
        ),
        (
            owner_ata,
            Account {
                lamports: 2_039_280,
                data: create_mollusk_token_account_data(&owner_mint, &wallet, 0),
                owner: token_program,
                executable: false,
                rent_epoch: 0,
            },
        ),
        (
            nested_ata,
            Account {
                lamports: 2_039_280,
                data: create_mollusk_token_account_data(&nested_mint, &owner_ata, 100), // Has tokens to recover
                owner: token_program,
                executable: false,
                rent_epoch: 0,
            },
        ),
        (
            destination_ata,
            Account {
                lamports: 2_039_280,
                data: create_mollusk_token_account_data(&nested_mint, &wallet, 0),
                owner: token_program,
                executable: false,
                rent_epoch: 0,
            },
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
        (
            token_program,
            Account {
                lamports: 0,
                data: Vec::new(),
                owner: LOADER_V3,
                executable: true,
                rent_epoch: 0,
            },
        ),
        (
            ata_program_id,
            Account {
                lamports: 0,
                data: Vec::new(),
                owner: LOADER_V3,
                executable: true,
                rent_epoch: 0,
            },
        ),
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
    let (owner_ata, owner_bump) = Pubkey::find_program_address(
        &[
            wallet.as_ref(),
            token_program_id.as_ref(),
            owner_mint.as_ref(),
        ],
        &ata_program_id,
    );

    let (nested_ata, nested_bump) = Pubkey::find_program_address(
        &[
            owner_ata.as_ref(),
            token_program_id.as_ref(),
            nested_mint.as_ref(),
        ],
        &ata_program_id,
    );

    // Derive the CANONICAL destination ATA
    let (canonical_destination_ata, _) = Pubkey::find_program_address(
        &[
            wallet.as_ref(),
            token_program_id.as_ref(),
            nested_mint.as_ref(),
        ],
        &ata_program_id,
    );

    // Derive NON-CANONICAL destination ATA using lower bump
    let seeds: &[&[u8]; 3] = &[
        wallet.as_ref(),
        token_program_id.as_ref(),
        nested_mint.as_ref(),
    ];
    let mut hasher = solana_program::hash::Hasher::default();
    for seed in seeds {
        hasher.hash(seed);
    }
    hasher.hash(&[non_canonical_bump]);
    hasher.hash(ata_program_id.as_ref());
    hasher.hash(b"ProgramDerivedAddress");
    let hash = hasher.result();
    let non_canonical_destination_ata = Pubkey::from(hash.to_bytes());

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
    let (owner_ata, owner_bump) = Pubkey::find_program_address(
        &[
            wallet.as_ref(),
            token_program_id.as_ref(),
            owner_mint.as_ref(),
        ],
        &ata_program_id,
    );

    let (nested_ata, nested_bump) = Pubkey::find_program_address(
        &[
            owner_ata.as_ref(),
            token_program_id.as_ref(),
            nested_mint.as_ref(),
        ],
        &ata_program_id,
    );

    let (canonical_destination_ata, canonical_dest_bump) = Pubkey::find_program_address(
        &[
            wallet.as_ref(),
            token_program_id.as_ref(),
            nested_mint.as_ref(),
        ],
        &ata_program_id,
    );

    // derive the on-curve attack address
    let seeds: &[&[u8]; 3] = &[
        wallet.as_ref(),
        token_program_id.as_ref(),
        nested_mint.as_ref(),
    ];
    let mut hasher = solana_program::hash::Hasher::default();
    for seed in seeds {
        hasher.hash(seed);
    }
    hasher.hash(&[attack_bump]);
    hasher.hash(ata_program_id.as_ref());
    hasher.hash(b"ProgramDerivedAddress");

    let hash = hasher.result();
    let on_curve_destination_ata = Pubkey::from(hash.to_bytes());

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
    for (pubkey, _) in &mut bad_accounts {
        if *pubkey == canonical_destination_ata {
            *pubkey = on_curve_destination_ata;
            break;
        }
    }

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
