#![cfg_attr(feature = "std", allow(dead_code, unused_imports))]

use {
    crate::tests::{
        benches::account_templates::StandardAccountSet, setup_mollusk_unified, MolluskAtaSetup,
        MolluskTokenSetup,
    },
    mollusk_svm::Mollusk,
    solana_account::Account,
    solana_instruction,
    solana_pubkey::Pubkey,
    std::{
        boxed::Box,
        format, println,
        string::{String, ToString},
        vec,
        vec::Vec,
    },
    strum::{Display, EnumIter},
};

/// Test bank identifier  
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestBankId {
    Benchmarks = 0,
    Failures = 1,
}

/// Account type identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum AccountTypeId {
    Payer = 0,
    Mint = 1,
    Wallet = 2,
    Ata = 3,
    SystemProgram = 4,
    TokenProgram = 5,
    RentSysvar = 6,
    OwnerMint = 7,
    NestedMint = 8,
    OwnerAta = 9,
    NestedAta = 10,
    Signer1 = 11,
    Signer2 = 12,
    Signer3 = 13,
}

// ========================== SHARED BENCHMARK SETUP ============================

/// Helper function to handle logging setup with conditional debug features
fn setup_logging(enable_debug: bool) {
    if enable_debug {
        std::env::set_var("RUST_LOG", "debug");
        solana_logger::setup_with(
            "debug,solana_runtime=debug,solana_program_runtime=debug,mollusk=debug",
        );
    } else {
        std::env::set_var("RUST_LOG", "error");
        let _ = solana_logger::setup_with(
            "error,solana_runtime=error,solana_program_runtime=error,mollusk=error",
        );
    }
}

pub struct BenchmarkSetup;

pub struct AllProgramIds {
    pub spl_ata_program_id: Pubkey,
    pub pata_prefunded_program_id: Pubkey,
    pub pata_legacy_program_id: Pubkey,
    pub token_program_id: Pubkey,
    pub token_2022_program_id: Pubkey,
}

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
                std::os::unix::fs::symlink(&full_target_path, &link_path)
                    .unwrap_or_else(|e| panic!("Failed to create symlink for {}: {}", filename, e));
                #[cfg(windows)]
                std::os::windows::fs::symlink_file(&full_target_path, &link_path)
                    .unwrap_or_else(|e| panic!("Failed to create symlink for {}: {}", filename, e));
            }
        }

        deploy_dir
    }

    /// Load program keypairs and return program IDs
    pub fn load_program_ids(manifest_dir: &str) -> AllProgramIds {
        use solana_keypair::Keypair;
        use solana_signer::Signer;
        use std::fs;

        let programs_to_load = [
            (
                "/target/deploy/pinocchio_ata_program-keypair.json",
                "pinocchio_ata_program",
            ),
            (
                "/target/deploy/pinocchio_ata_program_prefunded-keypair.json",
                "pinocchio_ata_program_prefunded",
            ),
            (
                "../target/deploy/spl_associated_token_account-keypair.json",
                "spl_associated_token_account",
            ),
            (
                "/programs/token-2022/target/deploy/spl_token_2022-keypair.json",
                "spl_token_2022",
            ),
            (
                "/programs/token/target/deploy/pinocchio_token_program-keypair.json",
                "pinocchio_token_program",
            ),
        ];

        let mut program_ids = AllProgramIds {
            spl_ata_program_id: Pubkey::default(),
            pata_prefunded_program_id: Pubkey::default(),
            pata_legacy_program_id: Pubkey::default(),
            token_program_id: Pubkey::default(),
            token_2022_program_id: Pubkey::default(),
        };

        for (keypair_path, program_name) in programs_to_load {
            let full_path = format!("{}/{}", manifest_dir, keypair_path);
            let keypair_data = fs::read_to_string(&full_path)
                .unwrap_or_else(|_| panic!("Failed to read {}", full_path));
            let keypair_bytes: Vec<u8> = serde_json::from_str(&keypair_data)
                .unwrap_or_else(|_| panic!("Failed to parse keypair JSON for {}", full_path));
            let keypair = Keypair::try_from(&keypair_bytes[..])
                .unwrap_or_else(|_| panic!("Invalid keypair for {}", full_path));
            let program_id = keypair.pubkey();

            match program_name {
                "pinocchio_ata_program" => program_ids.pata_legacy_program_id = program_id,
                "pinocchio_ata_program_prefunded" => {
                    program_ids.pata_prefunded_program_id = program_id
                }
                "spl_associated_token_account" => program_ids.spl_ata_program_id = program_id,
                "spl_token_2022" => program_ids.token_2022_program_id = program_id,
                "pinocchio_token_program" => program_ids.token_program_id = program_id,
                _ => panic!("Unknown program name: {}", program_name),
            }
        }

        if program_ids.token_program_id == Pubkey::default() {
            panic!("Token program ID not found");
        }
        // Use SPL Token interface ID for p-token program
        program_ids.token_program_id = Pubkey::from(spl_token_interface::program::ID);

        if program_ids.pata_prefunded_program_id == Pubkey::default() {
            panic!("P-ATA prefunded program ID not found");
        }
        if program_ids.pata_legacy_program_id == Pubkey::default() {
            panic!("P-ATA standard program ID not found");
        }
        if program_ids.spl_ata_program_id == Pubkey::default() {
            panic!("SPL ATA program ID not found");
        }
        if program_ids.token_2022_program_id == Pubkey::default() {
            panic!("Token 2022 program ID not found");
        }

        program_ids
    }

    #[allow(dead_code)]
    /// Validate that the benchmark setup works with a simple test
    pub fn validate_setup(
        mollusk: &Mollusk,
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> Result<(), String> {
        use solana_instruction::{AccountMeta, Instruction};

        // Simple validation test - create a basic instruction and ensure it doesn't crash
        let [payer, mint, wallet] = crate::pk_array![
            AtaVariant::SplAta,
            TestBankId::Benchmarks,
            1,
            [
                AccountTypeId::Payer,
                AccountTypeId::Mint,
                AccountTypeId::Wallet
            ]
        ];
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            program_id,
        );

        let accounts = StandardAccountSet::new(payer, ata, wallet, mint, token_program_id).to_vec();

        let ix = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(wallet, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(solana_system_interface::program::id(), false),
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
    #[allow(dead_code)]
    pub variant: AtaVariant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtaVariant {
    PAtaLegacy,    // P-ATA without create-prefunded-account
    PAtaPrefunded, // P-ATA with create-prefunded-account
    SplAta,        // Original SPL ATA
}

pub struct AllAtaImplementations {
    pub spl_impl: AtaImplementation,
    pub pata_prefunded_impl: AtaImplementation,
    pub pata_legacy_impl: AtaImplementation,
}

impl AllAtaImplementations {
    pub fn iter(&self) -> impl Iterator<Item = &AtaImplementation> {
        [
            &self.spl_impl,
            &self.pata_prefunded_impl,
            &self.pata_legacy_impl,
        ]
        .into_iter()
    }
}

impl AtaImplementation {
    pub fn all() -> AllAtaImplementations {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let program_ids = BenchmarkSetup::load_program_ids(manifest_dir);

        AllAtaImplementations {
            spl_impl: Self::spl_ata(program_ids.spl_ata_program_id),
            pata_prefunded_impl: Self::p_ata_prefunded(program_ids.pata_prefunded_program_id),
            pata_legacy_impl: Self::p_ata_legacy(program_ids.pata_legacy_program_id),
        }
    }

    pub fn p_ata_legacy(program_id: Pubkey) -> Self {
        Self {
            name: "p-ata-legacy",
            program_id,
            binary_name: "pinocchio_ata_program",
            variant: AtaVariant::PAtaLegacy,
        }
    }

    pub fn p_ata_prefunded(program_id: Pubkey) -> Self {
        Self {
            name: "p-ata-prefunded",
            program_id,
            binary_name: "pinocchio_ata_program_prefunded",
            variant: AtaVariant::PAtaPrefunded,
        }
    }

    pub fn spl_ata(program_id: Pubkey) -> Self {
        Self {
            name: "spl-ata",
            program_id,
            binary_name: "spl_associated_token_account",
            variant: AtaVariant::SplAta,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, PartialEq, Clone)]
pub enum CompatibilityStatus {
    /// Both implementations succeeded and produced byte-for-byte identical results.
    ///
    /// **GUARANTEES:**
    /// - Both instructions succeeded
    /// - All **writable accounts** (including ATA accounts) are byte-for-byte identical:
    ///   - `data`: Complete binary equality
    ///   - `lamports`: Exact same balance  
    ///   - `owner`: Same program owner
    /// - Read-only accounts are not compared (they shouldn't change)
    /// - Mint and owner addresses are intentionally kept consistent between P-ATA and SPL ATA
    ///   tests to enable true byte-for-byte comparison of ATA accounts
    ///
    /// **DOES NOT GUARANTEE:**
    /// - Identical compute unit consumption (tracked separately)
    /// - Identical instruction data in the case of new p-ATA optimizations (bump and/or len)
    /// - Read-only account equality (not relevant for result validation)
    Identical,
    BothRejected,        // Both failed with same error types
    OptimizedBehavior,   // P-ATA succeeded where original failed (bump optimization)
    AccountMismatch,     // Both succeeded but account states differ (concerning)
    IncompatibleFailure, // Both failed but with different error codes
    IncompatibleSuccess, // One succeeded, one failed unexpectedly
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub implementation: String,
    pub test_name: String,
    pub compute_units: u64,
    pub success: bool,
    pub error_message: Option<String>,
    pub captured_output: String, // Capture mollusk debug output
}

#[derive(Debug, Clone)]
pub struct ComparisonResult {
    pub test_name: String,
    pub p_ata: BenchmarkResult,
    pub spl_ata: BenchmarkResult,
    pub compute_savings: Option<i64>,
    pub compatibility_status: CompatibilityStatus,
}

// ========================== SHARED COMPARISON RUNNER ============================

/// Post-execution verification function type
/// Takes pre-execution accounts, post-execution accounts, and instruction
/// Returns a verification message to be added to the benchmark result
pub type PostExecutionVerificationFn = Box<
    dyn Fn(&[(Pubkey, Account)], &[(Pubkey, Account)], &solana_instruction::Instruction) -> String,
>;

pub struct BenchmarkRunner;

impl BenchmarkRunner {
    /// Run a single benchmark for one implementation, averaging over multiple iterations
    pub fn run_single_benchmark(
        test_name: &str,
        ix: &solana_instruction::Instruction,
        accounts: &[(Pubkey, Account)],
        implementation: &AtaImplementation,
        token_program_id: &Pubkey,
        iterations: usize,
    ) -> BenchmarkResult {
        let mollusk = Self::create_mollusk_for_all_ata_implementations(token_program_id);

        let mut total_compute_units = 0u64;
        let mut success_count = 0usize;
        let mut last_error_message = None;

        // Run the benchmark multiple times to get average compute units
        for _i in 0..iterations {
            // Run with quiet logging unless full-debug-logs feature is enabled
            #[cfg(not(feature = "full-debug-logs"))]
            let result = mollusk.process_instruction(ix, accounts);

            #[cfg(feature = "full-debug-logs")]
            let result = {
                let _original_rust_log =
                    std::env::var("RUST_LOG").unwrap_or_else(|_| "error".to_string());
                setup_logging(true);
                let result = mollusk.process_instruction(ix, accounts);
                std::env::set_var("RUST_LOG", &_original_rust_log);
                setup_logging(false);
                result
            };

            let iteration_success = matches!(
                result.program_result,
                mollusk_svm::result::ProgramResult::Success
            );

            if iteration_success {
                total_compute_units += result.compute_units_consumed;
                success_count += 1;
            } else {
                last_error_message = Some(format!("{:?}", result.program_result));
            }

            // Per-iteration debug output
            // println!("iter {i}: {}", result.compute_units_consumed);
        }

        // Calculate average compute units (only from successful runs)
        let avg_compute_units = if success_count > 0 {
            total_compute_units / success_count as u64
        } else {
            0
        };
        let overall_success = success_count > 0;
        let error_message = if overall_success {
            None
        } else {
            last_error_message
        };

        BenchmarkResult {
            implementation: implementation.name.to_string(),
            test_name: test_name.to_string(),
            compute_units: avg_compute_units,
            success: overall_success,
            error_message,
            captured_output: String::new(), // Will be populated if we need to re-run with debug
        }
    }

    /// Run a benchmark with a closure that builds test cases for each iteration
    /// This allows for different random wallets in each iteration
    pub fn run_single_benchmark_with_builder<F>(
        test_name: &str,
        test_case_builder: F,
        implementation: &AtaImplementation,
        token_program_id: &Pubkey,
        iterations: usize,
    ) -> BenchmarkResult
    where
        F: Fn(usize) -> (solana_instruction::Instruction, Vec<(Pubkey, Account)>),
    {
        let mollusk = Self::create_mollusk_for_all_ata_implementations(token_program_id);

        let mut total_compute_units = 0u64;
        let mut success_count = 0usize;
        let mut last_error_message = None;

        // Run the benchmark multiple times with different test cases for each iteration
        for i in 0..iterations {
            let (ix, accounts) = test_case_builder(i);
            let accounts_slice: Vec<(Pubkey, Account)> = accounts;

            // Run with quiet logging unless full-debug-logs feature is enabled
            #[cfg(not(feature = "full-debug-logs"))]
            let result = mollusk.process_instruction(&ix, &accounts_slice);

            #[cfg(feature = "full-debug-logs")]
            let result = {
                let _original_rust_log =
                    std::env::var("RUST_LOG").unwrap_or_else(|_| "error".to_string());
                setup_logging(true);
                let result = mollusk.process_instruction(&ix, &accounts_slice);
                std::env::set_var("RUST_LOG", &_original_rust_log);
                setup_logging(false);
                result
            };

            let iteration_success = matches!(
                result.program_result,
                mollusk_svm::result::ProgramResult::Success
            );

            if iteration_success {
                total_compute_units += result.compute_units_consumed;
                success_count += 1;
            } else {
                last_error_message = Some(format!("{:?}", result.program_result));
            }

            // Per-iteration debug output
            // println!("iter {i}: {}", result.compute_units_consumed);
        }

        // Calculate average compute units (only from successful runs)
        let avg_compute_units = if success_count > 0 {
            total_compute_units / success_count as u64
        } else {
            0
        };
        let overall_success = success_count > 0;
        let error_message = if overall_success {
            None
        } else {
            last_error_message
        };

        BenchmarkResult {
            implementation: implementation.name.to_string(),
            test_name: test_name.to_string(),
            compute_units: avg_compute_units,
            success: overall_success,
            error_message,
            captured_output: String::new(), // Will be populated if we need to re-run with debug
        }
    }

    /// Run a single benchmark with optional post-execution verification
    /// If verification_fn is provided and the instruction succeeds, it will capture
    /// post-execution state and call the verification function
    pub fn run_single_benchmark_with_post_account_inspection(
        test_name: &str,
        ix: &solana_instruction::Instruction,
        accounts: &[(Pubkey, Account)],
        implementation: &AtaImplementation,
        token_program_id: &Pubkey,
        verification_fn: Option<PostExecutionVerificationFn>,
    ) -> BenchmarkResult {
        // First run the benchmark normally (using 1 iteration for post-inspection)
        let mut result = Self::run_single_benchmark(
            test_name,
            ix,
            accounts,
            implementation,
            token_program_id,
            1,
        );

        // If verification function is provided and instruction succeeded, add verification
        if let Some(verify_fn) = verification_fn {
            if result.success {
                let mollusk = Self::create_mollusk_for_all_ata_implementations(token_program_id);
                let execution_result = mollusk.process_instruction(ix, accounts);

                if matches!(
                    execution_result.program_result,
                    mollusk_svm::result::ProgramResult::Success
                ) {
                    // Convert InstructionResult to post-execution accounts vector
                    let mut post_execution_accounts = Vec::new();
                    for (pubkey, _) in accounts {
                        if let Some(account) = execution_result.get_account(pubkey) {
                            post_execution_accounts.push((*pubkey, account.clone()));
                        }
                    }

                    let verification_message = verify_fn(accounts, &post_execution_accounts, ix);
                    result.captured_output.push_str(&verification_message);
                }
            }
        }

        result
    }

    /// Run a benchmark with verbose debug logging enabled - used for problematic results (single iteration)
    pub fn run_single_benchmark_with_debug(
        test_name: &str,
        ix: &solana_instruction::Instruction,
        accounts: &[(Pubkey, Account)],
        implementation: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> BenchmarkResult {
        // Temporarily enable debug logging
        let original_rust_log = std::env::var("RUST_LOG").unwrap_or_else(|_| "error".to_string());
        setup_logging(true);

        let mollusk = Self::create_mollusk_for_all_ata_implementations(token_program_id);

        // Capture output during execution
        let captured_output =
            Self::capture_output_during_execution(|| mollusk.process_instruction(ix, accounts));

        let (result, output) = captured_output;

        // Restore quiet logging unless full-debug-logs feature is enabled
        #[cfg(not(feature = "full-debug-logs"))]
        {
            std::env::set_var("RUST_LOG", &original_rust_log);
            setup_logging(false);
        }

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
            captured_output: output,
        }
    }

    /// Capture stdout/stderr output during function execution
    fn capture_output_during_execution<F, R>(f: F) -> (R, String)
    where
        F: FnOnce() -> R,
    {
        use std::sync::{Arc, Mutex};

        let captured = Arc::new(Mutex::new(Vec::new()));
        let captured_clone = captured.clone();

        let result = f();

        let captured_text = if let Ok(buffer) = captured_clone.lock() {
            String::from_utf8_lossy(&buffer).to_string()
        } else {
            String::new()
        };

        (result, captured_text)
    }

    /// Create mollusk instance with all ATA implementations loaded
    /// Uses the unified setup function for all ATA implementations
    pub fn create_mollusk_for_all_ata_implementations(token_program_id: &Pubkey) -> Mollusk {
        // Convert from pinocchio Pubkey to solana Pubkey for unified function
        let solana_token_program_id =
            solana_pubkey::Pubkey::new_from_array(token_program_id.to_bytes());

        // Use the unified setup to load all ATA implementations + token programs
        setup_mollusk_unified(
            MolluskAtaSetup::AllImplementations,
            MolluskTokenSetup::WithToken2022(solana_token_program_id),
        )
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

        let compatibility_status =
            Self::determine_compatibility_status(&p_ata_result, &original_result);

        ComparisonResult {
            test_name: test_name.to_string(),
            p_ata: p_ata_result,
            spl_ata: original_result,
            compute_savings,
            compatibility_status,
        }
    }

    /// Determine compatibility status based on results
    pub fn determine_compatibility_status(
        p_ata_result: &BenchmarkResult,
        original_result: &BenchmarkResult,
    ) -> CompatibilityStatus {
        // Check if this is a P-ATA-only test (N/A for original)
        if let Some(ref error_msg) = original_result.error_message {
            if error_msg.contains("N/A - Test not applicable to original ATA") {
                return CompatibilityStatus::OptimizedBehavior; // P-ATA-only feature
            }
        }

        match (p_ata_result.success, original_result.success) {
            (true, true) => CompatibilityStatus::Identical,
            (false, false) => {
                // Both failed - check if they failed with same error type
                match (&p_ata_result.error_message, &original_result.error_message) {
                    (Some(p_ata_err), Some(orig_err)) => {
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
                if p_ata_result.test_name.starts_with("fail_") {
                    CompatibilityStatus::IncompatibleSuccess
                } else {
                    CompatibilityStatus::OptimizedBehavior
                }
            }
            (false, true) => CompatibilityStatus::IncompatibleSuccess,
        }
    }

    /// Check if two error messages are compatible (same type of error)
    fn errors_are_compatible(p_ata_err: &str, orig_err: &str) -> bool {
        p_ata_err == orig_err
    }

    /// Print individual comparison result
    #[allow(dead_code)]
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
            result.spl_ata.compute_units,
            if result.spl_ata.success {
                "Success"
            } else {
                "Failed"
            }
        );

        // Savings analysis (mainly relevant for successful tests)
        if let Some(savings) = result.compute_savings {
            match savings {
                savings if savings > 0 => println!("  Savings: {:>8} CUs ", savings,),
                savings if savings < 0 => println!("  Overhead: {:>7} CUs ", -savings,),
                _ => println!("  Equal compute usage"),
            }
        }

        // Compatibility status
        match result.compatibility_status {
            CompatibilityStatus::Identical => {
                if result.test_name.starts_with("fail_")
                    && result.p_ata.success
                    && result.spl_ata.success
                {
                    println!("  Status: Both succeeded (TEST ISSUE - should fail!)")
                } else {
                    println!("  Status: Identical (both succeeded)")
                }
            }
            CompatibilityStatus::BothRejected => {
                println!("  Status: Both failed (same error type)")
            }
            CompatibilityStatus::OptimizedBehavior => {
                println!("  Status: P-ATA optimization working")
            }
            CompatibilityStatus::AccountMismatch => {
                println!("  Status: Account mismatch (concerning)")
            }
            CompatibilityStatus::IncompatibleFailure => {
                println!("  Status: Different failure modes (concerning)")
            }
            CompatibilityStatus::IncompatibleSuccess => {
                if result.test_name.starts_with("fail_") {
                    // Check which implementation actually succeeded
                    if result.p_ata.success && !result.spl_ata.success {
                        println!(
                            "  Status: 🚨 CRITICAL SECURITY ISSUE - P-ATA bypassed validation!"
                        )
                    } else if !result.p_ata.success && result.spl_ata.success {
                        println!("  Status: 🚨 CRITICAL SECURITY ISSUE - Original ATA bypassed validation!")
                    } else {
                        println!("  Status: 🚨 CRITICAL SECURITY ISSUE - Validation mismatch!")
                    }
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
        if !result.spl_ata.success {
            if let Some(ref error) = result.spl_ata.error_message {
                println!("  Original Error: {}", error);
            }
        }
    }
}

// ========================== BASE TEST TYPES ============================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, Display)]
#[strum(serialize_all = "snake_case")]
#[allow(dead_code)]
pub enum BaseTestType {
    Create,
    CreateIdempotent,
    CreateTopup,
    CreateTopupNoCap,
    CreateToken2022,
    CreateExtended,
    RecoverNested,
    RecoverMultisig,
    WorstCase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TestVariant {
    pub rent_arg: bool,
    pub bump_arg: bool,
    pub token_account_len_arg: bool,
}

#[allow(dead_code)]
impl TestVariant {
    pub const BASE: Self = Self {
        rent_arg: false,
        bump_arg: false,
        token_account_len_arg: false,
    };
    pub const RENT: Self = Self {
        rent_arg: true,
        bump_arg: false,
        token_account_len_arg: false,
    };
    pub const BUMP: Self = Self {
        rent_arg: false,
        bump_arg: true,
        token_account_len_arg: false,
    };
    pub const RENT_BUMP: Self = Self {
        rent_arg: true,
        bump_arg: true,
        token_account_len_arg: false,
    };
    pub const BUMP_LEN: Self = Self {
        rent_arg: false,
        bump_arg: true,
        token_account_len_arg: true,
    };
    pub const RENT_BUMP_LEN: Self = Self {
        rent_arg: true,
        bump_arg: true,
        token_account_len_arg: true,
    };

    pub fn column_name(&self) -> &'static str {
        match (self.rent_arg, self.bump_arg, self.token_account_len_arg) {
            (false, false, false) => "p-ata",
            (true, false, false) => "rent arg",
            (false, true, false) => "bump arg",
            (false, false, true) => panic!("token_account_len arg without bump arg"),
            (false, true, true) => "bump+token_account_len arg",
            (true, true, false) => "rent+bump arg",
            (true, false, true) => panic!("token_account_len arg without bump arg"),
            (true, true, true) => "all optimizations",
        }
    }

    pub fn test_suffix(&self) -> String {
        let mut parts = Vec::new();
        if self.rent_arg {
            parts.push("rent");
        }
        if self.bump_arg || self.token_account_len_arg {
            parts.push("bump");
        }
        if self.token_account_len_arg {
            parts.push("token_account_len");
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!("_{}", parts.join("_"))
        }
    }
}

impl BaseTestType {
    /// Returns which P-ATA variant this test should use
    #[allow(dead_code)]
    pub fn required_pata_variant(&self) -> AtaVariant {
        match self {
            Self::CreateTopup => AtaVariant::PAtaPrefunded, // Uses create-prefunded-account feature
            Self::CreateTopupNoCap => AtaVariant::PAtaLegacy, // Uses standard P-ATA without the feature
            _ => AtaVariant::PAtaLegacy,                      // All other tests use standard P-ATA
        }
    }
}
