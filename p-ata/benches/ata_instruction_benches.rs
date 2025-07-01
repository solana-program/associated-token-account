#![cfg(feature = "test-bpf")]

use {
    mollusk_svm::{program::loader_keys::LOADER_V3, Mollusk},
    mollusk_svm_bencher::MolluskComputeUnitBencher,
    solana_account::Account,
    solana_instruction::{AccountMeta, Instruction},
    solana_keypair::Keypair,
    solana_logger,
    solana_pubkey::Pubkey,
    solana_signer::Signer,
    solana_sysvar::rent,
    spl_token_2022::extension::ExtensionType,
    spl_token_interface::state::{account::Account as TokenAccount, mint::Mint, Transmutable},
    std::{fs, path::Path},
};

// System program and native loader constants
const SYSTEM_PROGRAM_ID: Pubkey = Pubkey::new_from_array([0u8; 32]); // 11111111111111111111111111111111
const NATIVE_LOADER_ID: Pubkey = Pubkey::new_from_array([
    5, 135, 132, 191, 20, 139, 164, 40, 47, 176, 18, 87, 72, 136, 169, 241, 83, 160, 125, 173, 247,
    101, 192, 69, 92, 154, 151, 3, 128, 0, 0, 0,
]); // NativeLoader1111111111111111111111111111111

/// Build a zero-rent `Rent` sysvar account with correctly sized data buffer.
fn rent_sysvar_account() -> Account {
    Account {
        lamports: 1,
        data: vec![1u8; 17], // Minimal rent sysvar data
        owner: rent::id(),
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

    /// print base58 representation of NATIVE_LOADER_ID
    println!("NATIVE_LOADER_ID: {}", NATIVE_LOADER_ID.to_string());

    /// print base58 representation of SYSTEM_PROGRAM_ID
    println!("SYSTEM_PROGRAM_ID: {}", SYSTEM_PROGRAM_ID.to_string());

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
        // Deterministic keys so CU cost is reproducible across runs
        fn const_pk(b: u8) -> Pubkey {
            Pubkey::new_from_array([b; 32])
        }

        // Use different base values for each test variant to avoid address collisions
        let base_offset = match (extended_mint, with_rent, topup) {
            (false, false, false) => 10, // create_base
            (false, true, false) => 20,  // create_rent
            (false, false, true) => 30,  // create_topup
            (true, false, false) => 40,  // create_ext
            (true, true, false) => 50,   // create_ext_rent
            (true, false, true) => 60,   // create_ext_topup
            _ => 70,                     // fallback for any other combination
        };

        let payer = const_pk(base_offset);
        let mint = const_pk(base_offset + 1);

        // Choose a wallet that gives bump 255 for its ATA
        let mut wallet = const_pk(base_offset + 2);
        let mut best_bump = 0u8;
        for b in (base_offset + 2)..=255 {
            let cand = const_pk(b);
            let (_, bump) = Pubkey::find_program_address(
                &[cand.as_ref(), token_program_id.as_ref(), mint.as_ref()],
                program_id,
            );
            if bump > best_bump {
                wallet = cand;
                best_bump = bump;
                if bump == 255 {
                    break;
                }
            }
        }

        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let variant_name = match (extended_mint, with_rent, topup) {
            (false, false, false) => "create_base",
            (false, true, false) => "create_rent",
            (false, false, true) => "create_topup",
            (true, false, false) => "create_ext",
            (true, true, false) => "create_ext_rent",
            (true, false, true) => "create_ext_topup",
            _ => "unknown_variant",
        };

        println!(
            "DEBUG build_create: Deterministic keys for {}:",
            variant_name
        );
        println!("  base_offset: {}", base_offset);
        println!("  payer: {}", payer);
        println!("  mint: {}", mint);
        println!("  wallet: {}", wallet);
        println!("  ATA address: {}", ata);
        println!("  topup flag: {}", topup);

        let mint_data = if extended_mint {
            build_extended_mint_data(0)
        } else {
            build_mint_data(0)
        };

        let mut accounts = vec![
            (payer, Account::new(1_000_000_000, 0, &SYSTEM_PROGRAM_ID)),
            (ata, Account::new(0, 0, &SYSTEM_PROGRAM_ID)),
            (wallet, Account::new(0, 0, &SYSTEM_PROGRAM_ID)),
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
                SYSTEM_PROGRAM_ID,
                Account {
                    lamports: 1,
                    data: vec![],
                    owner: NATIVE_LOADER_ID,
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
            accounts.push((rent::id(), rent_sysvar_account()));
        }

        // top-up path: account has received lamports but hasn't been allocated yet
        if topup {
            println!("DEBUG: Setting up topup scenario for ATA: {}", ata);
            if let Some((_, ata_acc)) = accounts.iter_mut().find(|(k, _)| *k == ata) {
                println!(
                    "DEBUG: BEFORE topup setup - ATA lamports: {}, data_len: {}, owner: {}",
                    ata_acc.lamports,
                    ata_acc.data.len(),
                    ata_acc.owner
                );
                // Account has received lamports but is not allocated - this is the topup scenario
                ata_acc.lamports = 1_000_000; // Some lamports but below rent-exempt
                                              // No data allocated, still system-owned (account "exists" only because of lamports)
                ata_acc.data = vec![];
                ata_acc.owner = SYSTEM_PROGRAM_ID;
                println!(
                    "DEBUG: AFTER topup setup - ATA lamports: {}, data_len: {}, owner: {}",
                    ata_acc.lamports,
                    ata_acc.data.len(),
                    ata_acc.owner
                );
            } else {
                println!("ERROR: Could not find ATA account in accounts list for topup setup!");
            }
        }

        let mut metas = vec![
            AccountMeta::new(payer, true),
            AccountMeta::new(ata, false),
            AccountMeta::new_readonly(wallet, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            AccountMeta::new_readonly(*token_program_id, false),
        ];
        if with_rent {
            metas.push(AccountMeta::new_readonly(rent::id(), false));
        }

        let ix = Instruction {
            program_id: *program_id,
            accounts: metas,
            data: vec![],
        };
        (ix, accounts)
    }

    // helper to build a pre-initialized ATA so the instruction hits the CreateIdempotent early-exit
    fn build_create_idempotent(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
        with_rent: bool,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        // Helper for deterministic pubkeys (array filled with the given byte)
        fn const_pk(fill: u8) -> Pubkey {
            Pubkey::new_from_array([fill; 32])
        }

        // Fixed payer & mint for reproducibility
        let payer = const_pk(1);
        let mint = const_pk(2);

        // Choose a wallet pubkey that yields the *maximum* bump (255)
        // so that the on-chain PDA search exits after the very first
        // keccak, giving the "best" and most predictable CU number.
        let mut wallet = const_pk(3);
        let mut best_bump = 0u8;
        for byte in 3u8..=255 {
            let candidate = const_pk(byte);
            let (_, bump) = Pubkey::find_program_address(
                &[candidate.as_ref(), token_program_id.as_ref(), mint.as_ref()],
                program_id,
            );
            if bump > best_bump {
                wallet = candidate;
                best_bump = bump;
                if bump == 255 {
                    break;
                }
            }
        }

        // Now derive the ATA PDA using the chosen wallet
        let (ata, _final_bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        // Build fully initialized token account data owned by `wallet`
        let ata_data = build_token_account_data(&mint, &wallet, 0);

        // Build accounts vector with the ATA already initialized and owned by the token program
        let mut accounts = vec![
            // payer
            (payer, Account::new(1_000_000_000, 0, &SYSTEM_PROGRAM_ID)),
            // the existing ATA (rent-exempt lamports, correct owner & data)
            (
                ata,
                Account {
                    lamports: 2_000_000, // >= rent-exempt
                    data: ata_data,
                    owner: *token_program_id,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            // wallet
            (wallet, Account::new(0, 0, &SYSTEM_PROGRAM_ID)),
            // mint
            (
                mint,
                Account {
                    lamports: 1_000_000_000,
                    data: build_mint_data(0),
                    owner: *token_program_id,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            // system program
            (
                SYSTEM_PROGRAM_ID,
                Account {
                    lamports: 1,
                    data: vec![],
                    owner: NATIVE_LOADER_ID,
                    executable: true,
                    rent_epoch: 0,
                },
            ),
            // token program (executable)
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
            accounts.push((rent::id(), rent_sysvar_account()));
        }

        // Same metas ordering as the Create instruction (payer, ata, wallet, mint, system, token [, rent])
        let mut metas = vec![
            AccountMeta::new(payer, true),
            AccountMeta::new(ata, false),
            AccountMeta::new_readonly(wallet, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            AccountMeta::new_readonly(*token_program_id, false),
        ];
        if with_rent {
            metas.push(AccountMeta::new_readonly(rent::id(), false));
        }

        // Discriminator 1 triggers CreateIdempotent
        let ix = Instruction {
            program_id: *program_id,
            accounts: metas,
            data: vec![1u8],
        };

        (ix, accounts)
    }

    let (create_ix, accounts_create) =
        build_create(&program_id, &token_program_id, false, false, false);
    let (create_rent_ix, accounts_create_rent) =
        build_create(&program_id, &token_program_id, false, true, false);
    let (create_topup_ix, accounts_create_topup) =
        build_create(&program_id, &token_program_id, false, false, true);

    // NEW: CreateIdempotent benchmark setup
    let (create_idemp_ix, accounts_create_idemp) =
        build_create_idempotent(&program_id, &token_program_id, false);

    // Same set but with an extended mint (longer data len)
    let (create_ext_ix, accounts_create_ext) =
        build_create(&program_id, &token_program_id, true, false, false);
    let (create_ext_rent_ix, accounts_create_ext_rent) =
        build_create(&program_id, &token_program_id, true, true, false);
    let (create_ext_topup_ix, accounts_create_ext_topup) =
        build_create(&program_id, &token_program_id, true, false, true);

    /* ------------------------------ RECOVER ------------------------------- */
    // Helper to build deterministic Pubkeys (32 identical bytes)
    fn const_pk(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    // --- Choose owner_mint first (fixed) ---
    let owner_mint = const_pk(20);

    // --- Pick a wallet whose bump for owner_ata is 255 (1-hash PDA search) ---
    let mut wallet = const_pk(30);
    let mut best_bump = 0u8;
    for b in 30u8..=255 {
        let cand = const_pk(b);
        let (_, bump) = Pubkey::find_program_address(
            &[
                cand.as_ref(),
                token_program_id.as_ref(),
                owner_mint.as_ref(),
            ],
            &program_id,
        );
        if bump > best_bump {
            wallet = cand;
            best_bump = bump;
            if bump == 255 {
                break;
            }
        }
    }

    // --- Now pick nested_mint so that nested_ata also yields bump 255 ---
    let mut nested_mint = const_pk(40);
    let mut best_nested_bump = 0u8;
    // owner_ata is not yet defined – we'll compute candidate bumps on the fly
    let (owner_ata_tmp, _) = Pubkey::find_program_address(
        &[
            wallet.as_ref(),
            token_program_id.as_ref(),
            owner_mint.as_ref(),
        ],
        &program_id,
    );
    for b in 40u8..=255 {
        let cand = const_pk(b);
        let (_, bump) = Pubkey::find_program_address(
            &[
                owner_ata_tmp.as_ref(),
                token_program_id.as_ref(),
                cand.as_ref(),
            ],
            &program_id,
        );
        if bump > best_nested_bump {
            nested_mint = cand;
            best_nested_bump = bump;
            if bump == 255 {
                break;
            }
        }
    }

    // owner_ata PDA (bump guaranteed high)
    let (owner_ata, owner_bump) = Pubkey::find_program_address(
        &[
            wallet.as_ref(),
            token_program_id.as_ref(),
            owner_mint.as_ref(),
        ],
        &program_id,
    );
    // nested_ata PDA (bump high)
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
        (wallet, Account::new(1_000_000_000, 0, &SYSTEM_PROGRAM_ID)),
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
    // Choose a multisig wallet that also yields bump 255 for its owner_ata_ms
    let mut wallet_ms = const_pk(60);
    let mut best_bump_ms = 0u8;
    for b in 60u8..=255 {
        let cand = const_pk(b);
        let (_, bump) = Pubkey::find_program_address(
            &[
                cand.as_ref(),
                token_program_id.as_ref(),
                owner_mint.as_ref(),
            ],
            &program_id,
        );
        if bump > best_bump_ms {
            wallet_ms = cand;
            best_bump_ms = bump;
            if bump == 255 {
                break;
            }
        }
    }

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
        (signer1, Account::new(1_000_000_000, 0, &SYSTEM_PROGRAM_ID)),
        // signer2 account (system, signer)
        (signer2, Account::new(1_000_000_000, 0, &SYSTEM_PROGRAM_ID)),
        // signer3 account (system, non-signer)
        (signer3, Account::new(1_000_000_000, 0, &SYSTEM_PROGRAM_ID)),
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

    fn clone_accounts(src: &[(Pubkey, Account)]) -> Vec<(Pubkey, Account)> {
        src.iter().map(|(k, v)| (*k, v.clone())).collect()
    }

    let mut isolated_bencher = |name: &str, ix: &Instruction, accts: &[(Pubkey, Account)]| {
        println!("\n=== DEBUG: Running benchmark: {} ===", name);
        println!("DEBUG: Instruction program_id: {}", ix.program_id);
        println!("DEBUG: Account list for {}:", name);
        for (i, (pubkey, account)) in accts.iter().enumerate() {
            println!(
                "  [{}] {} - lamports: {}, data_len: {}, owner: {}",
                i,
                pubkey,
                account.lamports,
                account.data.len(),
                account.owner
            );
        }
        MolluskComputeUnitBencher::new(fresh_mollusk(&program_id, &token_program_id))
            .bench((name, ix, &clone_accounts(accts)[..]))
            .must_pass(true)
            .out_dir("../target/benches")
            .execute();
    };

    println!("\n=== Running isolated benchmarks ===");
    isolated_bencher("create_base", &create_ix, &accounts_create);
    isolated_bencher("create_rent", &create_rent_ix, &accounts_create_rent);
    isolated_bencher("create_idemp", &create_idemp_ix, &accounts_create_idemp);
    isolated_bencher("recover", &recover_ix, &accounts_recover);
    isolated_bencher("recover_multisig", &recover_msix, &accounts_recover_ms);
    // Run create_topup last to avoid potential account state conflicts
    isolated_bencher("create_topup", &create_topup_ix, &accounts_create_topup);

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
}
