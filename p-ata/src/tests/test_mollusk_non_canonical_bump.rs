use super::mollusk_adapter::mollusk_program_test;
use solana_instruction::{AccountMeta, Instruction};
use solana_program_test::BanksClientError;
use solana_pubkey::Pubkey;
use solana_sdk::instruction::InstructionError;
use solana_sdk::{signer::Signer, transaction::Transaction, transaction::TransactionError};
use solana_sdk_ids::{system_program, sysvar};
use std::vec::Vec;

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
    for _ in 0..40_000 {
        let wallet = Pubkey::new_unique();

        let (canonical_addr, bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()],
            ata_program_id,
        );

        // sanity check while debugging
        if Pubkey::is_on_curve(&canonical_addr) {
            panic!("*** Picked canonical address is on curve! ***");
        }

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
    accounts.push(AccountMeta::new(payer, true)); // payer signer
    accounts.push(AccountMeta::new(ata_address, false)); // ATA account (writable)
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

#[tokio::test]
async fn test_rejects_suboptimal_bump() {
    let ata_program_id = spl_associated_token_account::id();
    let token_program_id = spl_token::id();

    let mint_pubkey = Pubkey::new_unique();

    // Define (canonical, sub) bump pairs to verify.
    let pairs = [
        (255u8, 254u8),
        (254u8, 253u8),
        (255u8, 252u8),
        (254u8, 252u8),
    ];

    // Set up Mollusk test environment once.
    let mut pt = mollusk_program_test(mint_pubkey);

    // Discover wallets & add funding.
    let mut wallet_infos = Vec::new();
    for &(canonical, sub) in &pairs {
        let (wallet, canonical_addr, sub_addr) = find_wallet_pair(
            canonical,
            sub,
            &token_program_id,
            &mint_pubkey,
            &ata_program_id,
        );

        pt.add_account(
            wallet,
            solana_sdk::account::Account::new(1_000_000_000, 0, &system_program::id()),
        );

        wallet_infos.push((wallet, canonical, canonical_addr, sub, sub_addr));
    }

    let (mut banks_client, payer, mut recent_blockhash) = pt.start().await;

    #[cfg(feature = "test-debug")]
    {
        eprintln!("=== Starting non-canonical bump test ===");
        eprintln!("Testing {} pairs: {:?}", pairs.len(), pairs);
    }

    for (wallet, canonical_bump, canonical_addr, sub_bump, sub_addr) in wallet_infos {
        #[cfg(feature = "test-debug")]
        {
            eprintln!(
                "\n--- Testing pair: canonical={}, sub={} ---",
                canonical_bump, sub_bump
            );
            eprintln!("Wallet: {}", wallet);
            eprintln!("Canonical address: {}", canonical_addr);
            eprintln!("Sub-optimal address: {}", sub_addr);
        }

        // 1) Sub-optimal should fail
        #[cfg(feature = "test-debug")]
        eprintln!("Testing sub-optimal bump {} (should FAIL)", sub_bump);
        let ix_fail = build_create_ix(
            ata_program_id,
            sub_addr,
            sub_bump,
            payer.pubkey(),
            wallet,
            mint_pubkey,
            token_program_id,
        );

        let mut tx_fail = Transaction::new_with_payer(&[ix_fail], Some(&payer.pubkey()));
        tx_fail.sign(&[&payer], recent_blockhash);
        let res_fail = banks_client.process_transaction(tx_fail).await;
        match res_fail {
            Err(BanksClientError::TransactionError(TransactionError::InstructionError(
                _,
                InstructionError::InvalidSeeds,
            ))) => {}
            other => panic!("Sub-optimal bump {sub_bump}: unexpected {other:?}"),
        }
        #[cfg(feature = "test-debug")]
        eprintln!("✓ Sub-optimal bump {} correctly failed", sub_bump);

        // Refresh blockhash
        recent_blockhash = banks_client
            .get_new_latest_blockhash(&recent_blockhash)
            .await
            .expect("blockhash");

        // 2) Canonical should succeed
        #[cfg(feature = "test-debug")]
        eprintln!("Testing canonical bump {} (should SUCCEED)", canonical_bump);
        let ix_ok = build_create_ix(
            ata_program_id,
            canonical_addr,
            canonical_bump,
            payer.pubkey(),
            wallet,
            mint_pubkey,
            token_program_id,
        );

        let mut tx_ok = Transaction::new_with_payer(&[ix_ok], Some(&payer.pubkey()));
        tx_ok.sign(&[&payer], recent_blockhash);
        banks_client
            .process_transaction(tx_ok)
            .await
            .unwrap_or_else(|e| {
                #[cfg(feature = "test-debug")]
                eprintln!("✗ Canonical bump {} FAILED: {e:?}", canonical_bump);
                panic!("Canonical bump {canonical_bump} failed: {e:?}")
            });
        #[cfg(feature = "test-debug")]
        eprintln!("✓ Canonical bump {} correctly succeeded", canonical_bump);

        // Get fresh blockhash for next iteration
        recent_blockhash = banks_client
            .get_new_latest_blockhash(&recent_blockhash)
            .await
            .expect("blockhash");
    }
    #[cfg(feature = "test-debug")]
    eprintln!("\n=== All test pairs completed successfully ===");
}
