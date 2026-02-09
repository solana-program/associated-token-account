use {
    super::test_bump_utils::setup_mollusk_for_bump_tests,
    mollusk_svm::result::Check,
    ata_mollusk_harness::{
        build_create_ata_instruction, create_ata_test_accounts, CreateAtaInstructionType,
    },
    solana_pubkey::Pubkey,
    solana_sdk::{program_error::ProgramError, signature::Keypair, signer::Signer},
    std::vec::Vec,
};

/// Find a wallet such that its canonical off-curve bump equals `target_canonical` and also
/// has at least one lower off-curve bump. Returns:
/// (wallet, canonical_addr, sub_addr)
fn find_wallet_pair(
    canonical_bump: u8,
    sub_bump: u8,
    token_program: &Pubkey,
    mint: &Pubkey,
    ata_program_id: &Pubkey,
) -> (Pubkey, Pubkey, Pubkey) {
    assert!(canonical_bump > sub_bump);
    const MAX_FIND_ATTEMPTS: u32 = 40_000;
    for _ in 0..MAX_FIND_ATTEMPTS {
        let wallet = Pubkey::new_unique();

        let (canonical_addr, bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()],
            ata_program_id,
        );

        if bump != canonical_bump {
            continue;
        }

        if let Ok(sub_addr) = Pubkey::create_program_address(
            &[
                wallet.as_ref(),
                token_program.as_ref(),
                mint.as_ref(),
                &[sub_bump],
            ],
            ata_program_id,
        ) {
            return (wallet, canonical_addr, sub_addr);
        }
    }
    panic!(
        "Failed to find wallet for canonical {} / sub {} after {} attempts",
        canonical_bump, sub_bump, MAX_FIND_ATTEMPTS
    );
}

#[test]
fn test_rejects_suboptimal_bump() {
    let ata_program_id = spl_associated_token_account::id();
    let token_program_id = spl_token::id();
    let mint_pubkey = Pubkey::new_unique();
    let payer = Keypair::new();

    // Define (canonical, sub) bump pairs to verify.
    let pairs = [
        (255u8, 254u8),
        (254u8, 253u8),
        (255u8, 252u8),
        (254u8, 252u8),
        (255u8, 250u8),
        (254u8, 250u8),
    ];

    let mollusk = setup_mollusk_for_bump_tests(&token_program_id.to_bytes());

    let mut wallet_infos = Vec::new();
    for &(canonical, sub) in &pairs {
        let (wallet, canonical_addr, sub_addr) = find_wallet_pair(
            canonical,
            sub,
            &token_program_id,
            &mint_pubkey,
            &ata_program_id,
        );
        wallet_infos.push((wallet, canonical, canonical_addr, sub, sub_addr));
    }

    for (wallet, canonical_bump, canonical_addr, sub_bump, sub_addr) in wallet_infos {
        // Test 1: Sub-optimal should fail
        {
            let ix_fail = build_create_ata_instruction(
                ata_program_id,
                payer.pubkey(),
                sub_addr,
                wallet,
                mint_pubkey,
                token_program_id,
                CreateAtaInstructionType::Create {
                    bump: Some(sub_bump),
                    account_len: None,
                },
            );

            let accounts =
                create_ata_test_accounts(&payer, sub_addr, wallet, mint_pubkey, token_program_id);

            mollusk.process_and_validate_instruction(
                &ix_fail,
                &accounts,
                &[Check::err(ProgramError::InvalidInstructionData)],
            );
        }

        {
            let ix_ok = build_create_ata_instruction(
                ata_program_id,
                payer.pubkey(),
                canonical_addr,
                wallet,
                mint_pubkey,
                token_program_id,
                CreateAtaInstructionType::Create {
                    bump: Some(canonical_bump),
                    account_len: None,
                },
            );

            let accounts = create_ata_test_accounts(
                &payer,
                canonical_addr,
                wallet,
                mint_pubkey,
                token_program_id,
            );

            mollusk.process_and_validate_instruction(&ix_ok, &accounts, &[Check::success()]);
        }
    }
}
