#![cfg(feature = "test-bpf")]

use {
    mollusk_svm::{program::loader_keys::LOADER_V3, Mollusk},
    mollusk_svm_bencher::MolluskComputeUnitBencher,
    solana_logger,
    solana_sdk::{
        account::Account,
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
        signature::{Keypair, Signer},
        system_program, sysvar,
    },
    spl_token_interface::state::{account::Account as TokenAccount, mint::Mint, Transmutable},
    std::{fs, path::Path},
};

/// Build a zero-rent `Rent` sysvar account with correctly sized data buffer.
fn rent_sysvar_account() -> Account {
    Account {
        lamports: 1,
        data: vec![1u8; 17], // Minimal rent sysvar data
        owner: sysvar::rent::id(),
        executable: false,
        rent_epoch: 0,
    }
}

/// Build raw token Account data with the supplied mint / owner / amount.
fn build_token_account_data(mint: &Pubkey, owner: &Pubkey, amount: u64) -> Vec<u8> {
    let mut data = vec![0u8; TokenAccount::LEN];

    // Offsets based on token Account layout (see interface/src/state/account.rs)
    // mint:   0..32
    data[0..32].copy_from_slice(mint.as_ref());
    // owner:  32..64
    data[32..64].copy_from_slice(owner.as_ref());
    // amount: 64..72 (u64 LE)
    data[64..72].copy_from_slice(&amount.to_le_bytes());
    // state enum byte after delegate COption (32+32+8+36 = 108)
    data[108] = 1; // Initialized

    data
}

/// Build mint data with given decimals and marked initialized.
fn build_mint_data(decimals: u8) -> Vec<u8> {
    let mut data = vec![0u8; Mint::LEN];
    // decimals offset: COption<Pubkey>(36) + supply(8) = 44
    data[44] = decimals;
    data[45] = 1; // is_initialized = true
    data
}

fn main() {
    // Enable useful logs from Mollusk and Solana runtime so we can diagnose failures.
    // Adjust the log filter as desired (e.g. "info", "debug", "trace").
    let _ = solana_logger::setup_with(
        "info,solana_runtime=info,solana_program_runtime=info,mollusk=debug",
    );

    // Tell Mollusk where to locate the compiled SBF program ELF so it can be loaded.
    // Resolve relative to the project root (CARGO_MANIFEST_DIR).
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    println!("CARGO_MANIFEST_DIR: {}", manifest_dir);

    let sbf_out_dir = format!("{}/target/sbpf-solana-solana/release", manifest_dir);
    println!("Setting SBF_OUT_DIR to: {}", sbf_out_dir);

    std::env::set_var("SBF_OUT_DIR", sbf_out_dir.clone());

    // Check if the directory exists and list its contents
    if let Ok(entries) = std::fs::read_dir(&sbf_out_dir) {
        println!("Contents of SBF_OUT_DIR:");
        for entry in entries {
            if let Ok(entry) = entry {
                println!("  - {}", entry.file_name().to_string_lossy());
            }
        }
    } else {
        println!("ERROR: SBF_OUT_DIR does not exist or cannot be read!");
    }

    // Copy pinocchio_token.so from programs/ to SBF_OUT_DIR if it doesn't exist
    let programs_dir = format!("{}/programs", manifest_dir);
    let token_so_src = Path::new(&programs_dir).join("pinocchio_token_program.so");
    let token_so_dst = Path::new(&sbf_out_dir).join("pinocchio_token_program.so");

    if token_so_src.exists() {
        if !token_so_dst.exists() {
            println!("Copying pinocchio_token_program.so to SBF_OUT_DIR");
            fs::copy(&token_so_src, &token_so_dst)
                .expect("Failed to copy pinocchio_token_program.so to SBF_OUT_DIR");
        }
    } else {
        panic!("pinocchio_token_program.so not found in programs/ directory");
    }

    // List SBF_OUT_DIR contents again after copying
    println!("\nContents of SBF_OUT_DIR after copying:");
    if let Ok(entries) = std::fs::read_dir(&sbf_out_dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                println!("  - {}", entry.file_name().to_string_lossy());
            }
        }
    }

    // Load the program IDs from their keypair files
    let ata_keypair_path = format!(
        "{}/target/deploy/pinocchio_ata_program-keypair.json",
        manifest_dir
    );
    let ata_keypair_data = fs::read_to_string(&ata_keypair_path)
        .expect("Failed to read pinocchio_ata_program-keypair.json");
    let ata_keypair_bytes: Vec<u8> = serde_json::from_str(&ata_keypair_data)
        .expect("Failed to parse pinocchio_ata_program keypair JSON");
    let ata_keypair =
        Keypair::from_bytes(&ata_keypair_bytes).expect("Invalid pinocchio_ata_program keypair");
    let program_id = ata_keypair.pubkey();

    // Read pinocchio_token keypair from programs/ directory
    let token_keypair_path = format!(
        "{}/programs/pinocchio_token_program-keypair.json",
        manifest_dir
    );
    let token_keypair_data = fs::read_to_string(&token_keypair_path)
        .expect("Failed to read pinocchio_token_program-keypair.json");
    let token_keypair_bytes: Vec<u8> = serde_json::from_str(&token_keypair_data)
        .expect("Failed to parse pinocchio_token_program keypair JSON");
    let token_keypair =
        Keypair::from_bytes(&token_keypair_bytes).expect("Invalid pinocchio_token_program keypair");
    let token_program_id = Pubkey::from(spl_token_interface::program::ID);

    /* ------------------------------- CREATE -------------------------------- */
    let payer = Pubkey::new_unique();
    let wallet = Pubkey::new_unique();
    let mint = Pubkey::new_unique();

    // Derived Associated Token Account (wallet + mint)
    let (ata, _bump) = Pubkey::find_program_address(
        &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
        &program_id,
    );

    // Account list (see processor::process_create docs)
    let accounts_create = vec![
        // payer
        (payer, Account::new(1_000_000_000, 0, &system_program::id())),
        // ata (PDA, uninitialized)
        (ata, Account::new(0, 0, &system_program::id())),
        // wallet
        (wallet, Account::new(0, 0, &system_program::id())),
        // mint (owned by SPL Token ID since pinocchio-token expects it)
        (
            mint,
            Account {
                lamports: 1_000_000_000,
                data: build_mint_data(0),
                owner: token_program_id,
                executable: false,
                rent_epoch: 0,
            },
        ),
        // system program (dummy)
        (
            system_program::id(),
            Account {
                lamports: 1,
                data: vec![],
                owner: solana_sdk::native_loader::id(),
                executable: true,
                rent_epoch: 0,
            },
        ),
        // token program (marked executable true so invoke succeeds)
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
        // SPL Token program (points to same implementation as pinocchio-token)
        (
            Pubkey::from(spl_token_interface::program::ID),
            Account {
                lamports: 0,
                data: Vec::new(),
                owner: LOADER_V3,
                executable: true,
                rent_epoch: 0,
            },
        ),
        // rent sysvar
        (sysvar::rent::id(), rent_sysvar_account()),
    ];

    let create_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer, true),
            AccountMeta::new(ata, false),
            AccountMeta::new_readonly(wallet, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(token_program_id, false),
            AccountMeta::new_readonly(sysvar::rent::id(), false),
            AccountMeta::new_readonly(Pubkey::from(spl_token_interface::program::ID), false),
        ],
        data: vec![], // 0 => Create
    };

    /* ------------------------------ RECOVER ------------------------------- */
    let owner_mint = Pubkey::new_unique();
    let nested_mint = Pubkey::new_unique();

    let (owner_ata, owner_bump) = Pubkey::find_program_address(
        &[
            wallet.as_ref(),
            token_program_id.as_ref(),
            owner_mint.as_ref(),
        ],
        &program_id,
    );
    let (nested_ata, _nested_bump) = Pubkey::find_program_address(
        &[
            owner_ata.as_ref(),
            token_program_id.as_ref(),
            nested_mint.as_ref(),
        ],
        &program_id,
    );
    let (dest_ata, _dest_bump) = Pubkey::find_program_address(
        &[
            wallet.as_ref(),
            token_program_id.as_ref(),
            nested_mint.as_ref(),
        ],
        &program_id,
    );

    let accounts_recover = vec![
        // nested_ata – holds tokens owned by owner_ata
        (
            nested_ata,
            Account {
                lamports: 1_000_000_000,
                data: build_token_account_data(&nested_mint, &owner_ata, 100),
                owner: token_program_id,
                executable: false,
                rent_epoch: 0,
            },
        ),
        // nested_mint
        (
            nested_mint,
            Account {
                lamports: 1_000_000_000,
                data: build_mint_data(0),
                owner: token_program_id,
                executable: false,
                rent_epoch: 0,
            },
        ),
        // dest_ata – wallet's ATA for nested_mint (starts empty)
        (
            dest_ata,
            Account {
                lamports: 1_000_000_000,
                data: build_token_account_data(&nested_mint, &wallet, 0),
                owner: token_program_id,
                executable: false,
                rent_epoch: 0,
            },
        ),
        // owner_ata – wallet's ATA for owner_mint (owner of nested_ata)
        (
            owner_ata,
            Account {
                lamports: 1_000_000_000,
                data: build_token_account_data(&owner_mint, &wallet, 0),
                owner: token_program_id,
                executable: false,
                rent_epoch: 0,
            },
        ),
        // owner_mint
        (
            owner_mint,
            Account {
                lamports: 1_000_000_000,
                data: build_mint_data(0),
                owner: token_program_id,
                executable: false,
                rent_epoch: 0,
            },
        ),
        // wallet (signer)
        (
            wallet,
            Account::new(1_000_000_000, 0, &system_program::id()),
        ),
        // token program (executable)
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
        // SPL Token program (points to same implementation as pinocchio-token)
        (
            Pubkey::from(spl_token_interface::program::ID),
            Account {
                lamports: 0,
                data: Vec::new(),
                owner: LOADER_V3,
                executable: true,
                rent_epoch: 0,
            },
        ),
    ];

    let recover_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(nested_ata, false),
            AccountMeta::new_readonly(nested_mint, false),
            AccountMeta::new(dest_ata, false),
            AccountMeta::new(owner_ata, false),
            AccountMeta::new_readonly(owner_mint, false),
            AccountMeta::new(wallet, true),
            AccountMeta::new_readonly(token_program_id, false),
            AccountMeta::new_readonly(Pubkey::from(spl_token_interface::program::ID), false),
        ],
        data: vec![2u8], // 2 => RecoverNested
    };

    /* ------------------------------ BENCH -------------------------------- */
    // Start with a Mollusk instance that already contains the common builtin programs
    let mut mollusk = Mollusk::default();

    // === DEBUG: show program ids and loader id being registered ===
    println!(
        "Registering p-ata program id: {} loader: {}",
        program_id, LOADER_V3
    );
    println!(
        "Registering pinocchio-token under SPL Token ID: {} loader: {}",
        Pubkey::from(spl_token_interface::program::ID),
        LOADER_V3
    );
    println!(
        "Registering pinocchio-token under custom token program ID: {} loader: {}",
        token_program_id, LOADER_V3
    );

    // Add our program under test (p-ata)
    mollusk.add_program(&program_id, "pinocchio_ata_program", &LOADER_V3);
    // Add pinocchio-token under the SPL Token ID since that's what some tests use
    mollusk.add_program(
        &Pubkey::from(spl_token_interface::program::ID),
        "pinocchio_token_program",
        &LOADER_V3,
    );
    // Add pinocchio-token under the custom token program ID that the benchmark uses
    mollusk.add_program(&token_program_id, "pinocchio_token_program", &LOADER_V3);

    // Verify the instruction is using the correct program ID
    println!("\n=== Verifying instruction setup ===");
    println!("create_ix.program_id: {}", create_ix.program_id);
    println!("Expected program_id: {}", program_id);
    assert_eq!(
        create_ix.program_id, program_id,
        "Instruction program ID doesn't match!"
    );

    // Test a simple instruction first
    println!("\n=== Testing simple instruction first ===");
    println!("Accounts being passed:");
    for (pubkey, account) in &accounts_create {
        println!(
            "  - {} (owner: {}, executable: {}, lamports: {})",
            pubkey, account.owner, account.executable, account.lamports
        );
    }
    let test_result = mollusk.process_instruction(&create_ix, &accounts_create);
    println!("Test result: {:?}", test_result);

    if !matches!(
        test_result.program_result,
        mollusk_svm::result::ProgramResult::Success
    ) {
        println!("ERROR: Test instruction failed!");
        println!("Program result: {:?}", test_result.program_result);
        println!("Compute units: {}", test_result.compute_units_consumed);
        panic!("Unable to run test instruction");
    }

    println!("\n=== Running benchmarks ===");
    MolluskComputeUnitBencher::new(mollusk)
        .bench(("create", &create_ix, &accounts_create[..]))
        .bench(("recover", &recover_ix, &accounts_recover[..]))
        .must_pass(true)
        .out_dir("../target/benches")
        .execute();

    // Prevent "function never used" warnings for the bumps (they're needed for seed calc correctness)
    let _ = owner_bump;
}
