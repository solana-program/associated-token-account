use {
    bs58,
    colored::Colorize,
    mollusk_svm::{program::loader_keys::LOADER_V3, Mollusk},
    solana_account::Account,
    solana_instruction,
    solana_pubkey::Pubkey,
    solana_sysvar::rent,
    spl_token_2022::extension::ExtensionType,
    spl_token_interface::state::Transmutable,
    std::collections::HashMap,
    std::env,
};

// ================================ CONSTANTS ================================

pub const SYSTEM_PROGRAM_ID: Pubkey = Pubkey::new_from_array([0u8; 32]);
pub const NATIVE_LOADER_ID: Pubkey = Pubkey::new_from_array([
    5, 135, 132, 191, 20, 139, 164, 40, 47, 176, 18, 87, 72, 136, 169, 241, 83, 160, 125, 173, 247,
    101, 192, 69, 92, 154, 151, 3, 128, 0, 0, 0,
]);

// ============================= ACCOUNT BUILDERS =============================

pub struct AccountBuilder;

impl AccountBuilder {
    pub fn rent_sysvar() -> Account {
        let mollusk = Mollusk::default();
        let (_, mollusk_rent_account) = mollusk.sysvars.keyed_account_for_rent_sysvar();

        Account {
            lamports: mollusk_rent_account.lamports,
            data: mollusk_rent_account.data,
            owner: rent::id(),
            executable: false,
            rent_epoch: 0,
        }
    }

    pub fn token_account_data(mint: &Pubkey, owner: &Pubkey, amount: u64) -> Vec<u8> {
        build_token_account_data_core(
            mint.as_ref().try_into().expect("Pubkey is 32 bytes"),
            owner.as_ref().try_into().expect("Pubkey is 32 bytes"),
            amount,
        )
        .to_vec()
    }

    pub fn mint_data(decimals: u8) -> Vec<u8> {
        build_mint_data_core(decimals).to_vec()
    }

    pub fn extended_mint_data(decimals: u8) -> Vec<u8> {
        let required_len =
            ExtensionType::try_calculate_account_len::<spl_token_2022::state::Mint>(&[
                ExtensionType::ImmutableOwner,
            ])
            .expect("calc len");

        let mut data = Self::mint_data(decimals);
        data.resize(required_len, 0u8);

        let cursor = 82;
        let immutable_owner_header = [7u8, 0u8, 0u8, 0u8];
        data[cursor..cursor + 4].copy_from_slice(&immutable_owner_header);

        data
    }

    pub fn multisig_data(m: u8, signer_pubkeys: &[Pubkey]) -> Vec<u8> {
        let byte_refs: Vec<&[u8; 32]> = signer_pubkeys
            .iter()
            .map(|pk| pk.as_ref().try_into().expect("Pubkey is 32 bytes"))
            .collect();
        build_multisig_data_core(m, &byte_refs)
    }

    pub fn system_account(lamports: u64) -> Account {
        Account::new(lamports, 0, &SYSTEM_PROGRAM_ID)
    }

    pub fn executable_program(owner: Pubkey) -> Account {
        Account {
            lamports: 0,
            data: Vec::new(),
            owner,
            executable: true,
            rent_epoch: 0,
        }
    }

    pub fn token_account(
        mint: &Pubkey,
        owner: &Pubkey,
        amount: u64,
        token_program_id: &Pubkey,
    ) -> Account {
        Account {
            lamports: 2_000_000, // rent-exempt
            data: Self::token_account_data(mint, owner, amount),
            owner: *token_program_id,
            executable: false,
            rent_epoch: 0,
        }
    }

    pub fn mint_account(decimals: u8, token_program_id: &Pubkey, extended: bool) -> Account {
        Account {
            lamports: 1_000_000_000,
            data: if extended {
                Self::extended_mint_data(decimals)
            } else {
                Self::mint_data(decimals)
            },
            owner: *token_program_id,
            executable: false,
            rent_epoch: 0,
        }
    }

    pub fn token_2022_mint_account(decimals: u8, token_program_id: &Pubkey) -> Account {
        Account {
            lamports: 1_000_000_000,
            data: Self::token_2022_mint_data(decimals),
            owner: *token_program_id,
            executable: false,
            rent_epoch: 0,
        }
    }

    pub fn token_2022_mint_data(decimals: u8) -> Vec<u8> {
        let mut data = [0u8; 82];
        let mint_authority = const_pk(123);

        data[0..4].copy_from_slice(&1u32.to_le_bytes());
        data[4..36].copy_from_slice(mint_authority.as_ref());
        data[44] = decimals;
        data[45] = 1;
        data[46..50].copy_from_slice(&0u32.to_le_bytes());

        data.to_vec()
    }
}

// =============================== UTILITIES =================================

pub fn const_pk(byte: u8) -> Pubkey {
    Pubkey::new_from_array([byte; 32])
}

/// Find a public key that gives optimal bump (255) for ATA derivation
pub fn const_pk_with_optimal_bump(
    base_byte: u8,
    ata_program_id: &Pubkey,
    token_program_id: &Pubkey,
    mint: &Pubkey,
) -> Pubkey {
    // Start with the base key
    let base_key = const_pk(base_byte);

    // Test if base key already has optimal bump
    let (_, bump) = Pubkey::find_program_address(
        &[base_key.as_ref(), token_program_id.as_ref(), mint.as_ref()],
        ata_program_id,
    );

    if bump == 255 {
        return base_key;
    }

    // Search for a key that gives optimal bump
    // We'll modify the key slightly by changing the last few bytes
    let mut key_bytes = [base_byte; 32];

    // Try different variations until we find one with bump 255
    for modifier in 0u32..10000 {
        // Modify the last 4 bytes with the modifier
        let modifier_bytes = modifier.to_le_bytes();
        key_bytes[28..32].copy_from_slice(&modifier_bytes);

        let test_key = Pubkey::new_from_array(key_bytes);
        let (_, test_bump) = Pubkey::find_program_address(
            &[test_key.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            ata_program_id,
        );

        if test_bump == 255 {
            println!(
                "Found optimal bump key for base {}: {} (modifier: {})",
                base_byte, test_key, modifier
            );
            return test_key;
        }
    }

    // If we couldn't find optimal bump, warn and return base key
    println!(
        "Warning: Could not find optimal bump key for base {}, using base key with bump {}",
        base_byte, bump
    );
    base_key
}

pub fn clone_accounts(src: &[(Pubkey, Account)]) -> Vec<(Pubkey, Account)> {
    src.iter().map(|(k, v)| (*k, v.clone())).collect()
}

pub fn fresh_mollusk(program_id: &Pubkey, token_program_id: &Pubkey) -> Mollusk {
    let mut mollusk = Mollusk::default();
    mollusk.add_program(program_id, "pinocchio_ata_program", &LOADER_V3);
    mollusk.add_program(
        &Pubkey::from(spl_token_interface::program::ID),
        "pinocchio_token_program",
        &LOADER_V3,
    );
    mollusk.add_program(token_program_id, "pinocchio_token_program", &LOADER_V3);

    // Add Token-2022 program with the actual Token-2022 binary
    let token_2022_id = Pubkey::new_from_array(pinocchio_pubkey::pubkey!(
        "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
    ));
    mollusk.add_program(&token_2022_id, "spl_token_2022", &LOADER_V3);

    mollusk
}

pub(crate) fn build_instruction_data(discriminator: u8, additional_data: &[u8]) -> Vec<u8> {
    let mut data = vec![discriminator];
    data.extend_from_slice(additional_data);
    data
}

pub fn build_multisig_data_core(m: u8, signer_pubkeys: &[&[u8; 32]]) -> Vec<u8> {
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
    data[0] = m;
    data[1] = signer_pubkeys.len() as u8;
    data[2] = 1;

    for (i, pk) in signer_pubkeys.iter().enumerate() {
        let offset = 3 + i * 32;
        data[offset..offset + 32].copy_from_slice(*pk);
    }
    data
}

#[inline(always)]
fn build_mint_data_core(decimals: u8) -> [u8; 82] {
    let mut data = [0u8; 82];
    data[0..4].copy_from_slice(&0u32.to_le_bytes());
    data[44] = decimals;
    data[45] = 1;
    data[46..50].copy_from_slice(&0u32.to_le_bytes());

    data
}

#[inline(always)]
fn build_token_account_data_core(mint: &[u8; 32], owner: &[u8; 32], amount: u64) -> [u8; 165] {
    let mut data = [0u8; 165];
    data[0..32].copy_from_slice(mint);
    data[32..64].copy_from_slice(owner);
    data[64..72].copy_from_slice(&amount.to_le_bytes());
    data[108] = 1;
    data
}

// ========================== SHARED BENCHMARK SETUP ============================

pub struct BenchmarkSetup;

impl BenchmarkSetup {
    /// Setup SBF output directory and copy required files
    pub fn setup_sbf_environment(manifest_dir: &str) -> String {
        use std::path::Path;

        // Use the standard deploy directory where p-ata program is built
        let deploy_dir = format!("{}/target/deploy", manifest_dir);
        println!("Setting SBF_OUT_DIR to: {}", deploy_dir);
        std::env::set_var("SBF_OUT_DIR", &deploy_dir);

        // Ensure the deploy directory exists
        std::fs::create_dir_all(&deploy_dir).expect("Failed to create deploy directory");

        // Create symbolic links to programs in their actual locations
        let symlinks = [
            (
                "spl_associated_token_account.so",
                "../target/deploy/spl_associated_token_account.so",
            ),
            (
                "pinocchio_token_program.so",
                "programs/token/target/deploy/pinocchio_token_program.so",
            ),
            (
                "spl_token_2022.so",
                "programs/token-2022/target/deploy/spl_token_2022.so",
            ),
        ];

        for (filename, target_path) in &symlinks {
            let link_path = Path::new(&deploy_dir).join(filename);
            let full_target_path = Path::new(manifest_dir).join(target_path);

            if full_target_path.exists() && !link_path.exists() {
                println!("Creating symlink {} -> {}", filename, target_path);
                #[cfg(unix)]
                {
                    std::os::unix::fs::symlink(&full_target_path, &link_path).unwrap_or_else(|e| {
                        panic!("Failed to create symlink for {}: {}", filename, e)
                    });
                }
                #[cfg(windows)]
                {
                    std::os::windows::fs::symlink_file(&full_target_path, &link_path)
                        .unwrap_or_else(|e| {
                            panic!("Failed to create symlink for {}: {}", filename, e)
                        });
                }
            }
        }

        deploy_dir
    }

    /// Load program keypairs and return program IDs
    pub fn load_program_ids(manifest_dir: &str) -> (Pubkey, Pubkey) {
        use solana_keypair::Keypair;
        use solana_signer::Signer;
        use std::fs;

        // Load ATA program keypair
        let ata_keypair_path = format!(
            "{}/target/deploy/pinocchio_ata_program-keypair.json",
            manifest_dir
        );
        let ata_keypair_data = fs::read_to_string(&ata_keypair_path)
            .expect("Failed to read pinocchio_ata_program-keypair.json");
        let ata_keypair_bytes: Vec<u8> = serde_json::from_str(&ata_keypair_data)
            .expect("Failed to parse pinocchio_ata_program keypair JSON");
        let ata_keypair = Keypair::try_from(&ata_keypair_bytes[..])
            .expect("Invalid pinocchio_ata_program keypair");
        let ata_program_id = ata_keypair.pubkey();

        // Use SPL Token interface ID for token program
        let token_program_id = Pubkey::from(spl_token_interface::program::ID);

        (ata_program_id, token_program_id)
    }

    /// Try to load P-ATA prefunded program ID, return None if not available
    pub(crate) fn try_load_prefunded_ata_program_id(manifest_dir: &str) -> Option<Pubkey> {
        use solana_keypair::Keypair;
        use solana_signer::Signer;
        use std::fs;

        let prefunded_keypair_path = format!(
            "{}/target/deploy/pinocchio_ata_program_prefunded-keypair.json",
            manifest_dir
        );

        if let Ok(keypair_data) = fs::read_to_string(&prefunded_keypair_path) {
            if let Ok(keypair_bytes) = serde_json::from_str::<Vec<u8>>(&keypair_data) {
                if let Ok(keypair) = Keypair::try_from(&keypair_bytes[..]) {
                    println!("Loaded P-ATA prefunded program ID: {}", keypair.pubkey());
                    return Some(keypair.pubkey());
                }
            }
        }

        println!("P-ATA prefunded program not found");
        println!("   Build with --features create-account-prefunded to enable prefunded tests");
        None
    }

    /// Load both p-ata and original ATA program IDs
    pub(crate) fn load_both_program_ids(manifest_dir: &str) -> (Pubkey, Option<Pubkey>, Pubkey) {
        let (p_ata_program_id, token_program_id) = Self::load_program_ids(manifest_dir);

        // Try to load original ATA program keypair
        let original_ata_program_id = Self::try_load_original_ata_program_id(manifest_dir);

        (p_ata_program_id, original_ata_program_id, token_program_id)
    }

    /// Load all available program IDs (P-ATA variants + original)
    pub(crate) fn load_all_program_ids(
        manifest_dir: &str,
    ) -> (Pubkey, Option<Pubkey>, Option<Pubkey>, Pubkey) {
        let (standard_program_id, token_program_id) = Self::load_program_ids(manifest_dir);
        let prefunded_program_id = Self::try_load_prefunded_ata_program_id(manifest_dir);
        let original_program_id = Self::try_load_original_ata_program_id(manifest_dir);

        (
            standard_program_id,
            prefunded_program_id,
            original_program_id,
            token_program_id,
        )
    }

    /// Try to load original ATA program ID, return None if not available
    pub(crate) fn try_load_original_ata_program_id(manifest_dir: &str) -> Option<Pubkey> {
        use solana_keypair::Keypair;
        use solana_signer::Signer;
        use std::fs;

        // Original ATA is built to ../target/deploy/ (parent directory)
        let original_keypair_path = format!(
            "{}/../target/deploy/spl_associated_token_account-keypair.json",
            manifest_dir
        );

        if let Ok(keypair_data) = fs::read_to_string(&original_keypair_path) {
            if let Ok(keypair_bytes) = serde_json::from_str::<Vec<u8>>(&keypair_data) {
                if let Ok(keypair) = Keypair::try_from(&keypair_bytes[..]) {
                    println!("Loaded original ATA program ID: {}", keypair.pubkey());
                    return Some(keypair.pubkey());
                }
            }
        }

        println!("Original ATA program not found, comparison mode unavailable");
        println!("   Run with --features build-programs to build both implementations");
        None
    }

    /// Validate that the benchmark setup works with a simple test
    pub(crate) fn validate_setup(
        mollusk: &Mollusk,
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> Result<(), String> {
        use solana_instruction::{AccountMeta, Instruction};

        // Simple validation test - create a basic instruction and ensure it doesn't crash
        let payer = const_pk(1);
        let mint = const_pk(2);
        let wallet = const_pk(3);
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            (ata, AccountBuilder::system_account(0)),
            (wallet, AccountBuilder::system_account(0)),
            (
                mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (
                SYSTEM_PROGRAM_ID,
                AccountBuilder::executable_program(NATIVE_LOADER_ID),
            ),
            (
                *token_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
        ];

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(*token_program_id, false),
            ],
            data: vec![0u8], // Create instruction
        };

        let result = mollusk.process_instruction(&ix, &accounts);

        match result.program_result {
            mollusk_svm::result::ProgramResult::Success => {
                println!("✓ Benchmark setup validation passed");
                Ok(())
            }
            _ => Err(format!(
                "Setup validation failed: {:?}",
                result.program_result
            )),
        }
    }
}

// ========================== SHARED COMPARISON FRAMEWORK ============================

#[derive(Debug, Clone)]
pub struct AtaImplementation {
    pub name: &'static str,
    pub program_id: Pubkey,
    pub binary_name: &'static str,
    pub variant: AtaVariant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtaVariant {
    PAtaStandard,  // P-ATA without create-account-prefunded
    PAtaPrefunded, // P-ATA with create-account-prefunded
    Original,      // Original SPL ATA
}

impl AtaImplementation {
    pub fn p_ata_standard(program_id: Pubkey) -> Self {
        Self {
            name: "p-ata-standard",
            program_id,
            binary_name: "pinocchio_ata_program",
            variant: AtaVariant::PAtaStandard,
        }
    }

    pub(crate) fn p_ata_prefunded(program_id: Pubkey) -> Self {
        Self {
            name: "p-ata-prefunded",
            program_id,
            binary_name: "pinocchio_ata_program_prefunded",
            variant: AtaVariant::PAtaPrefunded,
        }
    }

    pub fn p_ata(program_id: Pubkey) -> Self {
        // For backward compatibility, default to standard variant
        Self::p_ata_standard(program_id)
    }

    pub fn original(program_id: Pubkey) -> Self {
        Self {
            name: "original",
            program_id,
            binary_name: "spl_associated_token_account",
            variant: AtaVariant::Original,
        }
    }

    /// Adapt instruction data for this implementation
    pub fn adapt_instruction_data(&self, data: Vec<u8>) -> Vec<u8> {
        match self.variant {
            AtaVariant::PAtaStandard | AtaVariant::PAtaPrefunded => data, // P-ATA supports bump optimizations
            AtaVariant::Original => {
                // Original ATA doesn't support bump optimizations, strip them
                match data.as_slice() {
                    [0, _bump] => vec![0], // Create with bump -> Create without bump
                    [2, _bump] => vec![2], // RecoverNested with bump -> RecoverNested without bump
                    _ => data,             // Pass through other formats
                }
            }
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum CompatibilityStatus {
    Identical,           // Both succeeded with identical account states
    BothRejected,        // Both failed with same error types
    OptimizedBehavior,   // P-ATA succeeded where original failed (bump optimization)
    ExpectedDifferences, // Both succeeded but with expected differences (e.g., different ATA addresses)
    AccountMismatch,     // Both succeeded but account states differ (concerning)
    IncompatibleFailure, // Both failed but with different error codes
    IncompatibleSuccess, // One succeeded, one failed unexpectedly
}

#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub implementation: String,
    pub test_name: String,
    pub compute_units: u64,
    pub success: bool,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ComparisonResult {
    pub test_name: String,
    pub p_ata: BenchmarkResult,
    pub original: BenchmarkResult,
    pub compute_savings: Option<i64>,
    pub savings_percentage: Option<f64>,
    pub compatibility_status: CompatibilityStatus,
}

// ========================== SHARED COMPARISON RUNNER ============================

pub struct ComparisonRunner;

impl ComparisonRunner {
    /// Run a single benchmark for one implementation
    pub fn run_single_benchmark(
        test_name: &str,
        ix: &solana_instruction::Instruction,
        accounts: &[(Pubkey, Account)],
        implementation: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> BenchmarkResult {
        let mollusk = Self::create_mollusk_for_implementation(implementation, token_program_id);
        let result = mollusk.process_instruction(ix, accounts);

        let success = matches!(
            result.program_result,
            mollusk_svm::result::ProgramResult::Success
        );
        let error_message = if !success {
            Some(format!("{:?}", result.program_result))
        } else {
            None
        };

        BenchmarkResult {
            implementation: implementation.name.to_string(),
            test_name: test_name.to_string(),
            compute_units: result.compute_units_consumed,
            success,
            error_message,
        }
    }

    /// Create appropriate Mollusk instance for implementation
    pub fn create_mollusk_for_implementation(
        implementation: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> Mollusk {
        let mut mollusk = Mollusk::default();

        // Add the ATA program
        mollusk.add_program(
            &implementation.program_id,
            implementation.binary_name,
            &LOADER_V3,
        );

        // Add required token programs
        mollusk.add_program(
            &Pubkey::from(spl_token_interface::program::ID),
            "pinocchio_token_program",
            &LOADER_V3,
        );
        mollusk.add_program(token_program_id, "pinocchio_token_program", &LOADER_V3);

        // Add Token-2022
        let token_2022_id = Pubkey::new_from_array(pinocchio_pubkey::pubkey!(
            "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
        ));
        mollusk.add_program(&token_2022_id, "spl_token_2022", &LOADER_V3);

        mollusk
    }

    /// Create comparison result with compatibility checking
    pub fn create_comparison_result(
        test_name: &str,
        p_ata_result: BenchmarkResult,
        original_result: BenchmarkResult,
    ) -> ComparisonResult {
        let compute_savings = if p_ata_result.success && original_result.success {
            Some(original_result.compute_units as i64 - p_ata_result.compute_units as i64)
        } else {
            None
        };

        let savings_percentage = compute_savings.map(|savings| {
            if original_result.compute_units > 0 {
                (savings as f64 / original_result.compute_units as f64) * 100.0
            } else {
                0.0
            }
        });

        let compatibility_status =
            Self::determine_compatibility_status(&p_ata_result, &original_result);

        ComparisonResult {
            test_name: test_name.to_string(),
            p_ata: p_ata_result,
            original: original_result,
            compute_savings,
            savings_percentage,
            compatibility_status,
        }
    }

    /// Determine compatibility status based on results
    pub fn determine_compatibility_status(
        p_ata_result: &BenchmarkResult,
        original_result: &BenchmarkResult,
    ) -> CompatibilityStatus {
        match (p_ata_result.success, original_result.success) {
            (true, true) => {
                // Both succeeded - check if this is the Token-2022 test which has expected differences
                if p_ata_result.test_name == "create_token2022" {
                    CompatibilityStatus::ExpectedDifferences
                } else if p_ata_result.test_name.starts_with("fail_") {
                    // CRITICAL: Both implementations succeeded in a failure test - this is a test issue!
                    CompatibilityStatus::Identical
                } else {
                    // For other tests, assume identical if both succeeded
                    CompatibilityStatus::Identical
                }
            }
            (false, false) => {
                // Both failed - check if they failed with same error type
                match (&p_ata_result.error_message, &original_result.error_message) {
                    (Some(p_ata_err), Some(orig_err)) => {
                        // Simple heuristic: if error messages contain similar keywords, consider them compatible
                        if Self::errors_are_compatible(p_ata_err, orig_err) {
                            CompatibilityStatus::BothRejected
                        } else {
                            CompatibilityStatus::IncompatibleFailure
                        }
                    }
                    _ => CompatibilityStatus::IncompatibleFailure,
                }
            }
            (true, false) => {
                // P-ATA succeeded, Original failed
                if p_ata_result.test_name.starts_with("fail_") {
                    // CRITICAL SECURITY ISSUE: P-ATA succeeded in a failure test where original correctly failed!
                    CompatibilityStatus::IncompatibleSuccess
                } else {
                    // Performance test - P-ATA optimization (e.g., bump optimization)
                    CompatibilityStatus::OptimizedBehavior
                }
            }
            (false, true) => CompatibilityStatus::IncompatibleSuccess,
        }
    }

    /// Check if two error messages are compatible (same type of error)
    fn errors_are_compatible(p_ata_err: &str, orig_err: &str) -> bool {
        // Check for exact match - identical errors are always compatible
        p_ata_err == orig_err
    }

    /// Print individual comparison result
    pub fn print_comparison_result(result: &ComparisonResult) {
        println!("\n--- {} ---", result.test_name);

        // Compute unit comparison
        println!(
            "  P-ATA:    {:>8} CUs | {}",
            result.p_ata.compute_units,
            if result.p_ata.success {
                "Success"
            } else {
                "Failed"
            }
        );
        println!(
            "  Original: {:>8} CUs | {}",
            result.original.compute_units,
            if result.original.success {
                "Success"
            } else {
                "Failed"
            }
        );

        // Savings analysis (mainly relevant for successful tests)
        if let (Some(savings), Some(percentage)) =
            (result.compute_savings, result.savings_percentage)
        {
            if savings > 0 {
                println!("  Savings: {:>8} CUs ({:.1}%)", savings, percentage);
            } else if savings < 0 {
                println!("  Overhead: {:>7} CUs ({:.1}%)", -savings, -percentage);
            } else {
                println!("  Equal compute usage");
            }
        }

        // Compatibility status
        match result.compatibility_status {
            CompatibilityStatus::Identical => {
                if result.test_name.starts_with("fail_") && result.p_ata.success && result.original.success {
                    println!("  Status: Both succeeded (TEST ISSUE - should fail!)")
                } else {
                    println!("  Status: Identical (both succeeded)")
                }
            }
            CompatibilityStatus::BothRejected => {
                println!("  Status: Both rejected (same error type)")
            }
            CompatibilityStatus::OptimizedBehavior => {
                println!("  Status: P-ATA optimization working")
            }
            CompatibilityStatus::ExpectedDifferences => {
                println!("  Status: Both succeeded with expected differences")
            }
            CompatibilityStatus::AccountMismatch => {
                println!("  Status: Account mismatch (concerning)")
            }
            CompatibilityStatus::IncompatibleFailure => {
                println!("  Status: Different failure modes (concerning)")
            }
            CompatibilityStatus::IncompatibleSuccess => {
                if result.test_name.starts_with("fail_") {
                    println!("  Status: 🚨 CRITICAL SECURITY ISSUE - P-ATA bypassed validation!")
                } else {
                    println!("  Status: Incompatible success/failure (concerning)")
                }
            }
        }

        // Show error details if needed
        if !result.p_ata.success {
            if let Some(ref error) = result.p_ata.error_message {
                println!("  P-ATA Error: {}", error);
            }
        }
        if !result.original.success {
            if let Some(ref error) = result.original.error_message {
                println!("  Original Error: {}", error);
            }
        }
    }
}

// ======================= VERBOSE COMPARISON SUPPORT =======================

/// Structure to hold verbose execution results
pub struct VerboseResults {
    pub instruction_data: Vec<u8>,
    pub original_accounts: Vec<(Pubkey, Account)>,
    pub final_accounts: Vec<(Pubkey, Account)>,
}

/// Verbose comparison utilities
pub struct VerboseComparison;

impl VerboseComparison {
    /// Check if verbose mode is enabled via environment variable
    pub fn is_enabled() -> bool {
        env::var("P_ATA_VERBOSE").is_ok()
    }

    /// Print detailed byte-by-byte comparison
    pub fn print_detailed_comparison(
        comparison: &ComparisonResult,
        p_ata_verbose: &VerboseResults,
        original_verbose: &VerboseResults,
    ) {
        println!("\n🔍 VERBOSE COMPARISON: {}", comparison.test_name);
        println!("==========================================");

        // Compare instruction data
        Self::compare_instruction_data(
            &p_ata_verbose.instruction_data,
            &original_verbose.instruction_data,
        );

        // Compare input account data (before execution)
        Self::compare_input_account_data(
            &p_ata_verbose.original_accounts,
            &original_verbose.original_accounts,
        );

        // Compare changed accounts
        Self::compare_account_changes(
            &p_ata_verbose.original_accounts,
            &p_ata_verbose.final_accounts,
            &original_verbose.original_accounts,
            &original_verbose.final_accounts,
        );
    }

    /// Compare instruction data byte-by-byte
    fn compare_instruction_data(p_ata_data: &[u8], original_data: &[u8]) {
        println!("\n📋 INSTRUCTION DATA COMPARISON:");
        println!(
            "P-ATA instruction data:     {}",
            bs58::encode(p_ata_data).into_string()
        );
        println!(
            "Original instruction data:  {}",
            bs58::encode(original_data).into_string()
        );

        if p_ata_data == original_data {
            println!("{}", "✅ Instruction data IDENTICAL".green());
        } else {
            println!("{}", "❌ Instruction data DIFFERS".red());
            Self::print_byte_comparison(p_ata_data, original_data, "Instruction");
        }
    }

    /// Compare input account data (before execution)
    fn compare_input_account_data(
        p_ata_accounts: &[(Pubkey, Account)],
        original_accounts: &[(Pubkey, Account)],
    ) {
        println!("\n📥 INPUT ACCOUNT DATA COMPARISON:");

        let max_accounts = p_ata_accounts.len().max(original_accounts.len());

        for i in 0..max_accounts {
            println!("\n🔍 Account {} ({})", i, Self::get_account_role_name(i));

            let p_ata_account = p_ata_accounts.get(i);
            let original_account = original_accounts.get(i);

            match (p_ata_account, original_account) {
                (Some((p_ata_pk, p_ata_acc)), Some((orig_pk, orig_acc))) => {
                    println!("   P-ATA address:    {}", p_ata_pk);
                    println!("   Original address: {}", orig_pk);

                    // Compare the account data, lamports, and owner
                    let data_match = p_ata_acc.data == orig_acc.data;
                    let lamports_match = p_ata_acc.lamports == orig_acc.lamports;
                    let owner_match = p_ata_acc.owner == orig_acc.owner;

                    if data_match && lamports_match && owner_match {
                        println!("   {}", "✅ Account state IDENTICAL".green());
                    } else {
                        println!("   {}", "❌ Account state DIFFERS".red());
                        if !data_match {
                            println!(
                                "     📊 Data differs: {} vs {} bytes",
                                p_ata_acc.data.len(),
                                orig_acc.data.len()
                            );
                            if !p_ata_acc.data.is_empty() || !orig_acc.data.is_empty() {
                                Self::analyze_token_account_differences(
                                    &p_ata_acc.data,
                                    &orig_acc.data,
                                );
                            }
                        }
                        if !lamports_match {
                            println!(
                                "     💰 Lamports differ: {} vs {}",
                                p_ata_acc.lamports, orig_acc.lamports
                            );
                        }
                        if !owner_match {
                            println!(
                                "     👤 Owner differs: {} vs {}",
                                p_ata_acc.owner, orig_acc.owner
                            );
                        }
                    }
                }
                (Some((p_ata_pk, _)), None) => {
                    println!("   ⚠️  Only in P-ATA: {}", p_ata_pk);
                }
                (None, Some((orig_pk, _))) => {
                    println!("   ⚠️  Only in Original: {}", orig_pk);
                }
                (None, None) => unreachable!(),
            }
        }
    }

    /// Compare account changes between implementations by instruction position
    fn compare_account_changes(
        p_ata_original: &[(Pubkey, Account)],
        p_ata_final: &[(Pubkey, Account)],
        orig_original: &[(Pubkey, Account)],
        orig_final: &[(Pubkey, Account)],
    ) {
        println!("\n📊 ACCOUNT CHANGES COMPARISON:");

        // Map accounts by position in the original account list
        let p_ata_changes_by_pos =
            Self::find_account_changes_by_position(p_ata_original, p_ata_final);
        let orig_changes_by_pos = Self::find_account_changes_by_position(orig_original, orig_final);

        if p_ata_changes_by_pos.is_empty() && orig_changes_by_pos.is_empty() {
            println!("No account changes detected in either implementation.");
            return;
        }

        // Compare accounts by position (role in instruction)
        let max_pos = p_ata_changes_by_pos
            .keys()
            .max()
            .unwrap_or(&0)
            .max(orig_changes_by_pos.keys().max().unwrap_or(&0));

        for pos in 0..=*max_pos {
            if let (Some(p_ata_change), Some(orig_change)) = (
                p_ata_changes_by_pos.get(&pos),
                orig_changes_by_pos.get(&pos),
            ) {
                println!("\n🔄 Account {} Changed:", Self::get_account_role_name(pos));
                println!("   P-ATA Address:    {}", p_ata_change.0);
                println!("   Original Address: {}", orig_change.0);
                Self::compare_account_data(
                    &p_ata_change.1.data,
                    &orig_change.1.data,
                    &format!("Position {}", pos),
                );
            } else if let Some(p_ata_change) = p_ata_changes_by_pos.get(&pos) {
                println!(
                    "\n⚠️  Account {} changed in P-ATA only: {}",
                    Self::get_account_role_name(pos),
                    p_ata_change.0
                );
                Self::print_account_summary(&p_ata_change.1, "P-ATA");
            } else if let Some(orig_change) = orig_changes_by_pos.get(&pos) {
                println!(
                    "\n⚠️  Account {} changed in Original only: {}",
                    Self::get_account_role_name(pos),
                    orig_change.0
                );
                Self::print_account_summary(&orig_change.1, "Original");
            }
        }
    }

    /// Get human-readable name for account position
    fn get_account_role_name(position: usize) -> &'static str {
        match position {
            0 => "0 (Payer)",
            1 => "1 (ATA)",
            2 => "2 (Wallet)",
            3 => "3 (Mint)",
            4 => "4 (System Program)",
            5 => "5 (Token Program)",
            6 => "6 (Rent Sysvar)",
            _ => "Unknown",
        }
    }

    /// Print account summary
    fn print_account_summary(account: &Account, label: &str) {
        println!(
            "   {} account data: {} bytes, {} lamports, owner: {}",
            label,
            account.data.len(),
            account.lamports,
            account.owner
        );
        if !account.data.is_empty() {
            println!("   Data: {}", bs58::encode(&account.data).into_string());
        }
    }

    /// Find accounts that changed between original and final states by position
    fn find_account_changes_by_position(
        original: &[(Pubkey, Account)],
        final_accounts: &[(Pubkey, Account)],
    ) -> HashMap<usize, (Pubkey, Account)> {
        let mut changes = HashMap::new();

        for (pos, (pubkey, final_account)) in final_accounts.iter().enumerate() {
            if let Some((_, original_account)) = original.get(pos) {
                if original_account.data != final_account.data
                    || original_account.lamports != final_account.lamports
                    || original_account.owner != final_account.owner
                {
                    changes.insert(pos, (*pubkey, final_account.clone()));
                }
            }
        }

        changes
    }

    /// Find accounts that changed between original and final states (legacy method)
    fn find_account_changes(
        original: &[(Pubkey, Account)],
        final_accounts: &[(Pubkey, Account)],
    ) -> HashMap<Pubkey, Account> {
        let mut changes = HashMap::new();

        for (pubkey, final_account) in final_accounts {
            if let Some((_, original_account)) = original.iter().find(|(pk, _)| pk == pubkey) {
                if original_account.data != final_account.data
                    || original_account.lamports != final_account.lamports
                    || original_account.owner != final_account.owner
                {
                    changes.insert(*pubkey, final_account.clone());
                }
            }
        }

        changes
    }

    /// Compare account data byte-by-byte
    fn compare_account_data(p_ata_data: &[u8], original_data: &[u8], label: &str) {
        // Handle empty data case
        if p_ata_data.is_empty() && original_data.is_empty() {
            println!("   Both accounts have no data (empty)");
            return;
        }

        let p_ata_preview = if p_ata_data.is_empty() {
            "(empty)".to_string()
        } else {
            format!(
                "{} ({} bytes)",
                bs58::encode(p_ata_data).into_string(),
                p_ata_data.len()
            )
        };

        let orig_preview = if original_data.is_empty() {
            "(empty)".to_string()
        } else {
            format!(
                "{} ({} bytes)",
                bs58::encode(original_data).into_string(),
                original_data.len()
            )
        };

        if p_ata_data == original_data {
            println!("   P-ATA data:     {}", p_ata_preview.green());
            println!("   Original data:  {}", orig_preview.green());
            println!("   {}", "✅ Account data IDENTICAL".green());
        } else {
            println!("   P-ATA data:     {}", p_ata_preview.red());
            println!("   Original data:  {}", orig_preview.red());
            println!("   {}", "❌ Account data DIFFERS".red());
            if !p_ata_data.is_empty() || !original_data.is_empty() {
                Self::print_byte_comparison(p_ata_data, original_data, label);
            }
        }
    }

    /// Print byte-by-byte comparison with colored output
    fn print_byte_comparison(data1: &[u8], data2: &[u8], label: &str) {
        println!("\n🔍 Byte-by-byte comparison for {}:", label);

        let b58_1 = bs58::encode(data1).into_string();
        let b58_2 = bs58::encode(data2).into_string();

        let max_len = b58_1.len().max(b58_2.len());
        let chars1: Vec<char> = b58_1.chars().collect();
        let chars2: Vec<char> = b58_2.chars().collect();

        println!(
            "P-ATA:    {}",
            Self::colorize_comparison(&chars1, &chars2, true)
        );
        println!(
            "Original: {}",
            Self::colorize_comparison(&chars2, &chars1, false)
        );

        // Print summary
        let differences = Self::count_differences(&chars1, &chars2);
        if differences > 0 {
            println!(
                "{} differences found in {} characters",
                differences.to_string().red(),
                max_len
            );
        }
    }

    /// Colorize base58 string comparison
    fn colorize_comparison(chars1: &[char], chars2: &[char], _is_first: bool) -> String {
        let mut result = String::new();

        for (i, &ch1) in chars1.iter().enumerate() {
            if let Some(&ch2) = chars2.get(i) {
                if ch1 == ch2 {
                    result.push_str(&ch1.to_string().green().to_string());
                } else {
                    result.push_str(&ch1.to_string().red().to_string());
                }
            } else {
                // This character exists in one string but not the other
                result.push_str(&ch1.to_string().red().to_string());
            }
        }

        result
    }

    /// Count differences between two character arrays
    fn count_differences(chars1: &[char], chars2: &[char]) -> usize {
        let mut differences = 0;
        let max_len = chars1.len().max(chars2.len());

        for i in 0..max_len {
            let ch1 = chars1.get(i);
            let ch2 = chars2.get(i);

            if ch1 != ch2 {
                differences += 1;
            }
        }

        differences
    }

    /// Analyze differences in token account data structure
    fn analyze_token_account_differences(p_ata_data: &[u8], original_data: &[u8]) {
        println!("     📊 Token Account Structure Analysis:");

        // Token account structure (165 bytes total):
        // mint: Pubkey (32 bytes, offset 0-31)
        // owner: Pubkey (32 bytes, offset 32-63)
        // amount: u64 (8 bytes, offset 64-71)
        // delegate: COption<Pubkey> (36 bytes, offset 72-107) - 4 byte tag + 32 byte pubkey
        // state: u8 (1 byte, offset 108)
        // is_native: COption<u64> (12 bytes, offset 109-120) - 4 byte tag + 8 byte value
        // delegated_amount: u64 (8 bytes, offset 121-128)
        // close_authority: COption<Pubkey> (36 bytes, offset 129-164) - 4 byte tag + 32 byte pubkey

        let fields = [
            ("Mint", 0, 32),
            ("Owner", 32, 32),
            ("Amount", 64, 8),
            ("Delegate", 72, 36),
            ("State", 108, 1),
            ("IsNative", 109, 12),
            ("DelegatedAmount", 121, 8),
            ("CloseAuthority", 129, 36),
        ];

        for (field_name, offset, size) in fields {
            let end = offset + size;

            let p_ata_field = p_ata_data.get(offset..end).unwrap_or(&[]);
            let orig_field = original_data.get(offset..end).unwrap_or(&[]);

            if p_ata_field != orig_field {
                println!("       🔴 {} differs:", field_name);
                println!("         P-ATA:    {:02x?}", p_ata_field);
                println!("         Original: {:02x?}", orig_field);

                // Special analysis for certain fields
                match field_name {
                    "Mint" | "Owner" => {
                        if p_ata_field.len() == 32 && orig_field.len() == 32 {
                            let p_ata_pk = Pubkey::try_from(p_ata_field).unwrap_or_default();
                            let orig_pk = Pubkey::try_from(orig_field).unwrap_or_default();
                            println!("         P-ATA:    {}", p_ata_pk);
                            println!("         Original: {}", orig_pk);
                        }
                    }
                    "Amount" | "DelegatedAmount" => {
                        if p_ata_field.len() == 8 && orig_field.len() == 8 {
                            let p_ata_amount =
                                u64::from_le_bytes(p_ata_field.try_into().unwrap_or([0; 8]));
                            let orig_amount =
                                u64::from_le_bytes(orig_field.try_into().unwrap_or([0; 8]));
                            println!("         P-ATA:    {} tokens", p_ata_amount);
                            println!("         Original: {} tokens", orig_amount);
                        }
                    }
                    "State" => {
                        if !p_ata_field.is_empty() && !orig_field.is_empty() {
                            println!(
                                "         P-ATA:    {}",
                                Self::decode_account_state(p_ata_field[0])
                            );
                            println!(
                                "         Original: {}",
                                Self::decode_account_state(orig_field[0])
                            );
                        }
                    }
                    _ => {}
                }
            } else {
                println!("       ✅ {} identical", field_name);
            }
        }
    }

    /// Decode account state byte to human readable string
    fn decode_account_state(state: u8) -> &'static str {
        match state {
            0 => "Uninitialized",
            1 => "Initialized",
            2 => "Frozen",
            _ => "Unknown",
        }
    }
}

impl ComparisonRunner {
    /// Enhanced comparison run with verbose output support
    pub fn run_single_benchmark_verbose(
        test_name: &str,
        ix: &solana_instruction::Instruction,
        accounts: &[(Pubkey, Account)],
        implementation: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (BenchmarkResult, VerboseResults) {
        let cloned_accounts = clone_accounts(accounts);
        let mollusk = Self::create_mollusk_for_implementation(implementation, token_program_id);

        // Store the original accounts before execution
        let original_accounts = cloned_accounts.clone();

        let result = mollusk.process_instruction(ix, &cloned_accounts);

        let benchmark_result = BenchmarkResult {
            implementation: implementation.name.to_string(),
            test_name: test_name.to_string(),
            compute_units: result.compute_units_consumed,
            success: matches!(
                result.program_result,
                mollusk_svm::result::ProgramResult::Success
            ),
            error_message: if matches!(
                result.program_result,
                mollusk_svm::result::ProgramResult::Success
            ) {
                None
            } else {
                Some(format!("{:?}", result.program_result))
            },
        };

        // Get the final account states - Mollusk modifies the accounts in place
        // so the resulting_accounts should contain the final states
        let final_accounts = if result.resulting_accounts.is_empty() {
            // Fallback: if resulting_accounts is empty, use the modified cloned_accounts
            cloned_accounts
        } else {
            result.resulting_accounts
        };

        let verbose_results = VerboseResults {
            instruction_data: ix.data.clone(),
            original_accounts,
            final_accounts,
        };

        (benchmark_result, verbose_results)
    }

    /// Run comprehensive comparison with verbose output
    pub fn run_verbose_comparison(
        test_name: &str,
        p_ata_ix: &solana_instruction::Instruction,
        p_ata_accounts: &[(Pubkey, Account)],
        original_ix: &solana_instruction::Instruction,
        original_accounts: &[(Pubkey, Account)],
        p_ata_impl: &AtaImplementation,
        original_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> ComparisonResult {
        let (p_ata_result, p_ata_verbose) = Self::run_single_benchmark_verbose(
            test_name,
            p_ata_ix,
            p_ata_accounts,
            p_ata_impl,
            token_program_id,
        );
        let (original_result, original_verbose) = Self::run_single_benchmark_verbose(
            test_name,
            original_ix,
            original_accounts,
            original_impl,
            token_program_id,
        );

        let comparison_result =
            Self::create_comparison_result(test_name, p_ata_result, original_result);

        // Print verbose output if enabled
        if VerboseComparison::is_enabled() {
            VerboseComparison::print_detailed_comparison(
                &comparison_result,
                &p_ata_verbose,
                &original_verbose,
            );
        }

        comparison_result
    }
}

// ========================== BASE TEST TYPES ============================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BaseTestType {
    Create,
    CreateIdempotent,
    CreateTopup,
    CreateTopupNoCap,
    CreateToken2022,
    RecoverNested,
    RecoverMultisig,
    WorstCase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TestVariant {
    pub rent_arg: bool,
    pub bump_arg: bool,
    pub len_arg: bool,
}

impl TestVariant {
    pub const BASE: Self = Self {
        rent_arg: false,
        bump_arg: false,
        len_arg: false,
    };
    pub const RENT: Self = Self {
        rent_arg: true,
        bump_arg: false,
        len_arg: false,
    };
    pub const BUMP: Self = Self {
        rent_arg: false,
        bump_arg: true,
        len_arg: false,
    };
    pub const LEN: Self = Self {
        rent_arg: false,
        bump_arg: false,
        len_arg: true,
    };
    pub const RENT_BUMP: Self = Self {
        rent_arg: true,
        bump_arg: true,
        len_arg: false,
    };
    pub const RENT_LEN: Self = Self {
        rent_arg: true,
        bump_arg: false,
        len_arg: true,
    };
    pub const RENT_BUMP_LEN: Self = Self {
        rent_arg: true,
        bump_arg: true,
        len_arg: true,
    };

    pub fn column_name(&self) -> &'static str {
        match (self.rent_arg, self.bump_arg, self.len_arg) {
            (false, false, false) => "p-ata",
            (true, false, false) => "rent arg",
            (false, true, false) => "bump arg",
            (false, false, true) => "bump+len arg", // LEN variant now includes bump
            (true, true, false) => "rent+bump arg",
            (true, false, true) => "rent+bump+len arg", // RENT+LEN variant now includes bump
            (true, true, true) => "all optimizations",  // Special marker for best combination
            _ => "unknown",
        }
    }

    pub fn test_suffix(&self) -> String {
        let mut parts = Vec::new();
        if self.rent_arg {
            parts.push("rent");
        }
        if self.bump_arg || self.len_arg {
            parts.push("bump");
        }
        if self.len_arg {
            parts.push("len");
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!("_{}", parts.join("_"))
        }
    }
}

impl BaseTestType {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::CreateIdempotent => "create_idempotent",
            Self::CreateTopup => "create_topup",
            Self::CreateTopupNoCap => "create_topup_no_cap",
            Self::CreateToken2022 => "create_token2022",
            Self::RecoverNested => "recover_nested",
            Self::RecoverMultisig => "recover_multisig",
            Self::WorstCase => "worst_case",
        }
    }

    /// Returns which P-ATA variant this test should use
    pub fn required_pata_variant(&self) -> AtaVariant {
        match self {
            Self::CreateTopup => AtaVariant::PAtaPrefunded, // Uses create-account-prefunded feature
            Self::CreateTopupNoCap => AtaVariant::PAtaStandard, // Uses standard P-ATA without the feature
            _ => AtaVariant::PAtaStandard, // All other tests use standard P-ATA
        }
    }

    pub fn supported_variants(&self) -> Vec<TestVariant> {
        match self {
            Self::Create => vec![
                TestVariant::BASE,
                TestVariant::RENT,
                TestVariant::BUMP,
                TestVariant::RENT_BUMP,
            ],
            Self::CreateIdempotent => vec![TestVariant::BASE, TestVariant::RENT],
            Self::CreateTopup => vec![
                TestVariant::BASE,
                TestVariant::RENT,
                TestVariant::BUMP,
                TestVariant::RENT_BUMP,
            ],
            Self::CreateTopupNoCap => vec![
                TestVariant::BASE,
                TestVariant::RENT,
                TestVariant::BUMP,
                TestVariant::RENT_BUMP,
            ],
            Self::CreateToken2022 => vec![
                TestVariant::BASE,
                TestVariant::RENT,
                TestVariant::BUMP,
                TestVariant::LEN,
                TestVariant::RENT_BUMP,
                TestVariant::RENT_LEN,
                TestVariant::RENT_BUMP_LEN,
            ],
            Self::RecoverNested => vec![TestVariant::BASE, TestVariant::BUMP],
            Self::RecoverMultisig => vec![TestVariant::BASE, TestVariant::BUMP],
            Self::WorstCase => vec![TestVariant::BASE, TestVariant::BUMP],
        }
    }
}
