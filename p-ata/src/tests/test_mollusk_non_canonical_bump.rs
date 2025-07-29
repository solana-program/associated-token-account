use {
    mollusk_svm::{program::loader_keys::LOADER_V3, result::Check, Mollusk},
    solana_instruction::{AccountMeta, Instruction},
    solana_pubkey::Pubkey,
    solana_sdk::{program_error::ProgramError, signature::Keypair, signer::Signer},
    solana_sdk_ids::{system_program, sysvar},
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
    // as long as each number is >=250,
    for _ in 0..40_000 {
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
    panic!("Failed to find wallet for canonical {canonical_bump} / sub {sub_bump}");
}

/// Construct a create instruction for a given ATA address & bump.
fn build_create_ix(
    ata_program_id: Pubkey,
    ata_address: Pubkey,
    bump: u8,
    payer: Pubkey,
    wallet: Pubkey,
    mint: Pubkey,
    token_program: Pubkey,
) -> Instruction {
    let mut accounts = Vec::with_capacity(7);
    accounts.push(AccountMeta::new(payer, true));
    accounts.push(AccountMeta::new(ata_address, false));
    accounts.push(AccountMeta::new_readonly(wallet, false));
    accounts.push(AccountMeta::new_readonly(mint, false));
    accounts.push(AccountMeta::new_readonly(system_program::id(), false));
    accounts.push(AccountMeta::new_readonly(token_program, false));
    accounts.push(AccountMeta::new_readonly(sysvar::rent::id(), false));

    Instruction {
        program_id: ata_program_id,
        accounts,
        data: Vec::from([0u8, bump]), // discriminator 0 (Create) + bump
    }
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

            let ix_fail = build_create_ix(
                ata_program_id,
                sub_addr,
                sub_bump,
                payer.pubkey(),
                wallet,
                mint_pubkey,
                token_program_id,
            );

            let accounts = crate::tests::test_utils::create_ata_test_accounts(
                &payer,
                sub_addr,
                wallet,
                mint_pubkey,
                token_program_id,
            );

            mollusk.process_and_validate_instruction(
                &ix_fail,
                &accounts,
                &[Check::err(ProgramError::InvalidInstructionData)],
            );
        }

        {
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

            let ix_ok = build_create_ix(
                ata_program_id,
                canonical_addr,
                canonical_bump,
                payer.pubkey(),
                wallet,
                mint_pubkey,
                token_program_id,
            );

            let accounts = crate::tests::test_utils::create_ata_test_accounts(
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
