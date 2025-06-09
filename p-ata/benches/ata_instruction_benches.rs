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
    spl_token_2022::extension::ExtensionType,
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
    println!(
        "Base mint data length: {}, data[44]: {}, data[45]: {}",
        data.len(),
        data[44],
        data[45]
    );
    data
}

/// Build an "extended" mint whose data length is larger than the base Mint::LEN so that
/// the ATA create path activates the `get_account_len` CPI.  We don't need to populate a real
/// extension layout; the runtime only checks the length to decide that extensions exist.
fn build_extended_mint_data(decimals: u8) -> Vec<u8> {
    // Calculate the exact size token-2022 expects for a Mint with ImmutableOwner extension
    let required_len = ExtensionType::try_calculate_account_len::<spl_token_2022::state::Mint>(&[
        ExtensionType::ImmutableOwner,
    ])
    .expect("calc len");
    println!("Extended mint required_len: {}", required_len);

    // Start with base mint
    let mut data = build_mint_data(decimals);
    // Ensure vector has required length, zero-padded
    data.resize(required_len, 0u8);

    // Compose TLV entries at correct offset (base len = 82)
    let mut cursor = 82; // Standard SPL Token mint length
                         // ImmutableOwner header
    data[cursor..cursor + 2].copy_from_slice(&(ExtensionType::ImmutableOwner as u16).to_le_bytes());
    data[cursor + 2..cursor + 4].copy_from_slice(&0u16.to_le_bytes()); // len = 0
    cursor += 4;
    // Sentinel header
    data[cursor..cursor + 4].copy_from_slice(&0u32.to_le_bytes());

    println!(
        "Extended mint data length: {}, data[45]: {}",
        data.len(),
        data[45]
    );
    data
}

/// Build a Multisig account data with given signer public keys and threshold `m`.
fn build_multisig_data(m: u8, signer_pubkeys: &[Pubkey]) -> Vec<u8> {
    use spl_token_interface::state::multisig::{Multisig, MAX_SIGNERS};
    assert!(
        m as usize <= signer_pubkeys.len(),
        "m cannot exceed number of provided signers"
    );
    assert!(m >= 1, "m must be at least 1");
    assert!(
        signer_pubkeys.len() <= MAX_SIGNERS as usize,
        "too many signers provided"
    );

    let mut data = vec![0u8; Multisig::LEN];
    data[0] = m; // m threshold
    data[1] = signer_pubkeys.len() as u8; // n signers
    data[2] = 1; // is_initialized

    for (i, pk) in signer_pubkeys.iter().enumerate() {
        let offset = 3 + i * 32;
        data[offset..offset + 32].copy_from_slice(pk.as_ref());
    }
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
    let _token_keypair =
        Keypair::from_bytes(&token_keypair_bytes).expect("Invalid pinocchio_token_program keypair");
    let token_program_id = Pubkey::from(spl_token_interface::program::ID);

    /* ---------- helper to build CREATE variants ---------- */
    #[allow(clippy::too_many_arguments)]
    fn build_create(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
        extended_mint: bool,
        with_rent: bool,
        topup: bool,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let payer = Pubkey::new_unique();
        let wallet = Pubkey::new_unique();
        let mint = Pubkey::new_unique();

        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let mint_data = if extended_mint {
            build_extended_mint_data(0)
        } else {
            build_mint_data(0)
        };

        let mut accounts = vec![
            (payer, Account::new(1_000_000_000, 0, &system_program::id())),
            (ata, Account::new(0, 0, &system_program::id())),
            (wallet, Account::new(0, 0, &system_program::id())),
            (
                mint,
                Account {
                    lamports: 1_000_000_000,
                    data: mint_data,
                    owner: *token_program_id,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
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
            (
                *token_program_id,
                Account {
                    lamports: 0,
                    data: Vec::new(),
                    owner: LOADER_V3,
                    executable: true,
                    rent_epoch: 0,
                },
            ),
        ];

        if with_rent {
            accounts.push((sysvar::rent::id(), rent_sysvar_account()));
        }

        // top-up path: pre-create ata with correct size but insufficient lamports
        if topup {
            if let Some((_, ata_acc)) = accounts.iter_mut().find(|(k, _)| *k == ata) {
                ata_acc.data = vec![0u8; 165];
                // Set to insufficient lamports to test actual top-up path (needs ~2M for rent exempt)
                ata_acc.lamports = 1_000_000; // Below rent-exempt, needs top-up
                                              // Keep system-owned for legitimate top-up scenario (allocated but not initialized)
            }
        }

        let mut metas = vec![
            AccountMeta::new(payer, true),
            AccountMeta::new(ata, false),
            AccountMeta::new_readonly(wallet, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(*token_program_id, false),
        ];
        if with_rent {
            metas.push(AccountMeta::new_readonly(sysvar::rent::id(), false));
        }

        let ix = Instruction {
            program_id: *program_id,
            accounts: metas,
            data: vec![],
        };
        (ix, accounts)
    }

    let (create_ix, accounts_create) =
        build_create(&program_id, &token_program_id, false, false, false);
    let (create_rent_ix, accounts_create_rent) =
        build_create(&program_id, &token_program_id, false, true, false);
    let (create_topup_ix, accounts_create_topup) =
        build_create(&program_id, &token_program_id, false, false, true);

    // Same set but with an extended mint (longer data len)
    let (create_ext_ix, accounts_create_ext) =
        build_create(&program_id, &token_program_id, true, false, false);
    let (create_ext_rent_ix, accounts_create_ext_rent) =
        build_create(&program_id, &token_program_id, true, true, false);
    let (create_ext_topup_ix, accounts_create_ext_topup) =
        build_create(&program_id, &token_program_id, true, false, true);

    /* ------------------------------ RECOVER ------------------------------- */
    let wallet = Pubkey::new_unique();
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

    /* ------------------------- RECOVER (MULTISIG WALLET) ------------------------- */
    let wallet_ms = Pubkey::new_unique();
    let signer1 = Pubkey::new_unique();
    let signer2 = Pubkey::new_unique();
    let signer3 = Pubkey::new_unique();
    let ms_threshold: u8 = 2; // 2 of 3 multisig

    let (owner_ata_ms, owner_bump_ms) = Pubkey::find_program_address(
        &[
            wallet_ms.as_ref(),
            token_program_id.as_ref(),
            owner_mint.as_ref(),
        ],
        &program_id,
    );
    let (nested_ata_ms, _nested_bump_ms) = Pubkey::find_program_address(
        &[
            owner_ata_ms.as_ref(),
            token_program_id.as_ref(),
            nested_mint.as_ref(),
        ],
        &program_id,
    );
    let (dest_ata_ms, _dest_bump_ms) = Pubkey::find_program_address(
        &[
            wallet_ms.as_ref(),
            token_program_id.as_ref(),
            nested_mint.as_ref(),
        ],
        &program_id,
    );

    let accounts_recover_ms = vec![
        // nested_ata_ms – holds tokens owned by owner_ata_ms
        (
            nested_ata_ms,
            Account {
                lamports: 1_000_000_000,
                data: build_token_account_data(&nested_mint, &owner_ata_ms, 100),
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
        // dest_ata_ms – wallet_ms's ATA for nested_mint
        (
            dest_ata_ms,
            Account {
                lamports: 1_000_000_000,
                data: build_token_account_data(&nested_mint, &wallet_ms, 0),
                owner: token_program_id,
                executable: false,
                rent_epoch: 0,
            },
        ),
        // owner_ata_ms – wallet_ms's ATA for owner_mint (owner of nested_ata_ms)
        (
            owner_ata_ms,
            Account {
                lamports: 1_000_000_000,
                data: build_token_account_data(&owner_mint, &wallet_ms, 0),
                owner: token_program_id,
                executable: false,
                rent_epoch: 0,
            },
        ),
        // owner_mint (same as before)
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
        // wallet_ms (multisig)
        (
            wallet_ms,
            Account {
                lamports: 1_000_000_000,
                data: build_multisig_data(ms_threshold, &[signer1, signer2, signer3]),
                owner: token_program_id,
                executable: false,
                rent_epoch: 0,
            },
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
        // signer1 account (system, signer)
        (
            signer1,
            Account::new(1_000_000_000, 0, &system_program::id()),
        ),
        // signer2 account (system, signer)
        (
            signer2,
            Account::new(1_000_000_000, 0, &system_program::id()),
        ),
        // signer3 account (system, non-signer)
        (
            signer3,
            Account::new(1_000_000_000, 0, &system_program::id()),
        ),
    ];

    // Build account metas for the instruction
    let mut recover_ms_metas = vec![
        AccountMeta::new(nested_ata_ms, false),
        AccountMeta::new_readonly(nested_mint, false),
        AccountMeta::new(dest_ata_ms, false),
        AccountMeta::new(owner_ata_ms, false),
        AccountMeta::new_readonly(owner_mint, false),
        AccountMeta::new(wallet_ms, false), // multisig wallet writable, not signer
        AccountMeta::new_readonly(token_program_id, false),
        AccountMeta::new_readonly(Pubkey::from(spl_token_interface::program::ID), false),
    ];
    // append signer metas
    recover_ms_metas.push(AccountMeta::new_readonly(signer1, true));
    recover_ms_metas.push(AccountMeta::new_readonly(signer2, true));
    recover_ms_metas.push(AccountMeta::new_readonly(signer3, false));

    let recover_msix = Instruction {
        program_id,
        accounts: recover_ms_metas,
        data: vec![2u8], // RecoverNested
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
        .bench(("create_base", &create_ix, &accounts_create[..]))
        .bench(("create_rent", &create_rent_ix, &accounts_create_rent[..]))
        .bench(("create_topup", &create_topup_ix, &accounts_create_topup[..]))
        .bench(("recover", &recover_ix, &accounts_recover[..]))
        .bench(("recover_multisig", &recover_msix, &accounts_recover_ms[..]))
        .must_pass(true)
        .out_dir("../target/benches")
        .execute();

    // Prevent "function never used" warnings for the bumps (they're needed for seed calc correctness)
    let _ = owner_bump;
    let _ = owner_bump_ms;

    // After initial program registry debug prints remove original mollusk variable to avoid confusion. We'll create helper.

    // Helper to produce a fresh Mollusk with the p-ata and token programs registered
    fn fresh_mollusk(program_id: &Pubkey, token_program_id: &Pubkey) -> Mollusk {
        let mut m = Mollusk::default();
        m.add_program(program_id, "pinocchio_ata_program", &LOADER_V3);
        m.add_program(
            &Pubkey::from(spl_token_interface::program::ID),
            "pinocchio_token_program",
            &LOADER_V3,
        );
        m.add_program(token_program_id, "pinocchio_token_program", &LOADER_V3);
        m
    }

    println!("\n=== Running isolated benchmarks ===");

    // create_base
    MolluskComputeUnitBencher::new(fresh_mollusk(&program_id, &token_program_id))
        .bench(("create_base", &create_ix, &accounts_create[..]))
        .must_pass(true)
        .out_dir("../target/benches")
        .execute();

    // create_rent
    MolluskComputeUnitBencher::new(fresh_mollusk(&program_id, &token_program_id))
        .bench(("create_rent", &create_rent_ix, &accounts_create_rent[..]))
        .must_pass(true)
        .out_dir("../target/benches")
        .execute();

    // create_topup
    MolluskComputeUnitBencher::new(fresh_mollusk(&program_id, &token_program_id))
        .bench(("create_topup", &create_topup_ix, &accounts_create_topup[..]))
        .must_pass(true)
        .out_dir("../target/benches")
        .execute();

    // recover (normal)
    MolluskComputeUnitBencher::new(fresh_mollusk(&program_id, &token_program_id))
        .bench(("recover", &recover_ix, &accounts_recover[..]))
        .must_pass(true)
        .out_dir("../target/benches")
        .execute();

    // recover_multisig (isolated with its own multisig accounts)
    MolluskComputeUnitBencher::new(fresh_mollusk(&program_id, &token_program_id))
        .bench(("recover_multisig", &recover_msix, &accounts_recover_ms[..]))
        .must_pass(true)
        .out_dir("../target/benches")
        .execute();

    // Extended-mint isolated benches disabled for now, since p-token doesn't support extensions yet.
}
