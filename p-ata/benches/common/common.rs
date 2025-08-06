#![cfg_attr(feature = "std", allow(dead_code, unused_imports))]

use {
    crate::account_templates::StandardAccountSet,
    mollusk_svm::Mollusk,
    pinocchio_ata_program::{
        test_helpers::address_gen::{random_seeded_pk, structured_pk, AccountTypeId, TestBankId},
        test_utils::{
            setup_mollusk_unified, AtaImplementation, AtaVariant, MolluskAtaSetup,
            MolluskTokenSetup,
        },
    },
    solana_account::Account,
    solana_instruction,
    solana_pubkey::Pubkey,
    std::{
        boxed::Box,
        format, println,
        string::{String, ToString},
        vec::{self, Vec},
    },
    strum::{Display, EnumIter},
};

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

    /// Validate that the benchmark setup works with a simple test
    pub fn validate_setup(
        mollusk: &Mollusk,
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> Result<(), String> {
        use solana_instruction::{AccountMeta, Instruction};

        // Simple validation test - create a basic instruction and ensure it doesn't crash
        let payer = structured_pk(
            &AtaVariant::SplAta,
            TestBankId::Benchmarks,
            1,
            AccountTypeId::Payer,
        );
        let mint = structured_pk(
            &AtaVariant::SplAta,
            TestBankId::Benchmarks,
            1,
            AccountTypeId::Mint,
        );
        let wallet = structured_pk(
            &AtaVariant::SplAta,
            TestBankId::Benchmarks,
            1,
            AccountTypeId::Wallet,
        );
        let (ata, _bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            &program_id,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, Display)]
#[strum(serialize_all = "snake_case")]
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
    pub fn required_pata_variant(&self) -> AtaVariant {
        match self {
            Self::CreateTopup => AtaVariant::PAtaPrefunded, // Uses create-prefunded-account feature
            Self::CreateTopupNoCap => AtaVariant::PAtaLegacy, // Uses standard P-ATA without the feature
            _ => AtaVariant::PAtaLegacy,                      // All other tests use standard P-ATA
        }
    }
}

/// Generate random signers and find optimal multisig wallet for nested ATA operations
pub fn find_optimal_multisig_for_nested_ata(
    token_program: &Pubkey,
    owner_mint: &Pubkey,
    nested_mint: &Pubkey,
    ata_program_ids: &[Pubkey],
    search_entropy: u64,
) -> (Vec<Pubkey>, Pubkey) {
    use pinocchio_ata_program::test_utils::create_multisig_data;

    // Try up to 1000 combinations to find optimal bumps
    for attempt in 0..1000 {
        let attempt_entropy = search_entropy.wrapping_add(attempt);

        // Generate 3 random signers
        let signers = vec![
            random_seeded_pk(
                &AtaVariant::SplAta,
                TestBankId::Benchmarks,
                70, // RecoverMultisig base
                AccountTypeId::Signer1,
                attempt_entropy,
                attempt_entropy.wrapping_mul(31),
            ),
            random_seeded_pk(
                &AtaVariant::SplAta,
                TestBankId::Benchmarks,
                70,
                AccountTypeId::Signer2,
                attempt_entropy,
                attempt_entropy.wrapping_mul(37),
            ),
            random_seeded_pk(
                &AtaVariant::SplAta,
                TestBankId::Benchmarks,
                70,
                AccountTypeId::Signer3,
                attempt_entropy,
                attempt_entropy.wrapping_mul(41),
            ),
        ];

        // Create multisig data (threshold = 2 of 3)
        let multisig_data = create_multisig_data(
            2,
            3,
            &signers.iter().map(|s| s.to_bytes()).collect::<Vec<_>>(),
        );

        // Derive multisig wallet from the hash of multisig data
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        use std::hash::{Hash, Hasher};
        multisig_data.hash(&mut hasher);
        let multisig_hash = hasher.finish();

        let multisig_wallet = random_seeded_pk(
            &AtaVariant::SplAta,
            TestBankId::Benchmarks,
            70,
            AccountTypeId::Wallet,
            multisig_hash,
            attempt_entropy,
        );

        // Check if all ATA derivations produce optimal bumps (255)
        let mut all_optimal = true;
        for &ata_program_id in ata_program_ids {
            // Owner ATA
            let (_, owner_bump) = Pubkey::find_program_address(
                &[
                    multisig_wallet.as_ref(),
                    token_program.as_ref(),
                    owner_mint.as_ref(),
                ],
                &ata_program_id,
            );

            // Nested ATA (owner_ata is the "wallet" for nested)
            let (owner_ata, _) = Pubkey::find_program_address(
                &[
                    multisig_wallet.as_ref(),
                    token_program.as_ref(),
                    owner_mint.as_ref(),
                ],
                &ata_program_id,
            );
            let (_, nested_bump) = Pubkey::find_program_address(
                &[
                    owner_ata.as_ref(),
                    token_program.as_ref(),
                    nested_mint.as_ref(),
                ],
                &ata_program_id,
            );

            // Destination ATA
            let (_, dest_bump) = Pubkey::find_program_address(
                &[
                    multisig_wallet.as_ref(),
                    token_program.as_ref(),
                    nested_mint.as_ref(),
                ],
                &ata_program_id,
            );

            if owner_bump != 255 || nested_bump != 255 || dest_bump != 255 {
                all_optimal = false;
                break;
            }
        }

        if all_optimal {
            return (signers, multisig_wallet);
        }
    }

    // Fallback: return last attempt even if not optimal
    let signers = vec![
        random_seeded_pk(
            &AtaVariant::SplAta,
            TestBankId::Benchmarks,
            70,
            AccountTypeId::Signer1,
            search_entropy,
            search_entropy.wrapping_mul(31),
        ),
        random_seeded_pk(
            &AtaVariant::SplAta,
            TestBankId::Benchmarks,
            70,
            AccountTypeId::Signer2,
            search_entropy,
            search_entropy.wrapping_mul(37),
        ),
        random_seeded_pk(
            &AtaVariant::SplAta,
            TestBankId::Benchmarks,
            70,
            AccountTypeId::Signer3,
            search_entropy,
            search_entropy.wrapping_mul(41),
        ),
    ];

    let multisig_data = create_multisig_data(
        2,
        3,
        &signers.iter().map(|s| s.to_bytes()).collect::<Vec<_>>(),
    );
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    multisig_data.hash(&mut hasher);
    let multisig_hash = hasher.finish();

    let multisig_wallet = random_seeded_pk(
        &AtaVariant::SplAta,
        TestBankId::Benchmarks,
        70,
        AccountTypeId::Wallet,
        multisig_hash,
        search_entropy,
    );

    (signers, multisig_wallet)
}

/// Generate random wallet and find optimal bumps for nested ATA operations
pub fn find_optimal_wallet_for_nested_ata(
    token_program: &Pubkey,
    owner_mint: &Pubkey,
    nested_mint: &Pubkey,
    ata_program_ids: &[Pubkey],
    search_entropy: u64,
) -> Pubkey {
    // Try up to 1000 wallets to find optimal bumps
    for attempt in 0..1000 {
        let attempt_entropy = search_entropy.wrapping_add(attempt);

        let wallet = random_seeded_pk(
            &AtaVariant::SplAta,
            TestBankId::Benchmarks,
            60, // RecoverNested base
            AccountTypeId::Wallet,
            attempt_entropy,
            attempt_entropy.wrapping_mul(43),
        );

        // Check if all ATA derivations produce optimal bumps (255)
        let mut all_optimal = true;
        for &ata_program_id in ata_program_ids {
            // Owner ATA
            let (_, owner_bump) = Pubkey::find_program_address(
                &[wallet.as_ref(), token_program.as_ref(), owner_mint.as_ref()],
                &ata_program_id,
            );

            // Nested ATA (owner_ata is the "wallet" for nested)
            let (owner_ata, _) = Pubkey::find_program_address(
                &[wallet.as_ref(), token_program.as_ref(), owner_mint.as_ref()],
                &ata_program_id,
            );
            let (_, nested_bump) = Pubkey::find_program_address(
                &[
                    owner_ata.as_ref(),
                    token_program.as_ref(),
                    nested_mint.as_ref(),
                ],
                &ata_program_id,
            );

            // Destination ATA
            let (_, dest_bump) = Pubkey::find_program_address(
                &[
                    wallet.as_ref(),
                    token_program.as_ref(),
                    nested_mint.as_ref(),
                ],
                &ata_program_id,
            );

            if owner_bump != 255 || nested_bump != 255 || dest_bump != 255 {
                all_optimal = false;
                break;
            }
        }

        if all_optimal {
            return wallet;
        }
    }

    // Fallback: return last attempt even if not optimal
    random_seeded_pk(
        &AtaVariant::SplAta,
        TestBankId::Benchmarks,
        60,
        AccountTypeId::Wallet,
        search_entropy,
        search_entropy.wrapping_mul(43),
    )
}

pub fn find_optimal_wallet_for_mints(
    token_program: &Pubkey,
    mints: &[Pubkey],
    ata_program_ids: &[Pubkey],
    search_entropy: u64,
) -> Pubkey {
    // Try up to 1000 wallets to find optimal bumps across ALL ATA programs and mints
    for attempt in 0..1000 {
        let attempt_entropy = search_entropy.wrapping_add(attempt);

        let wallet = random_seeded_pk(
            &AtaVariant::SplAta,
            TestBankId::Benchmarks,
            0, // Base test number for standard creates
            AccountTypeId::Wallet,
            attempt_entropy,
            attempt_entropy.wrapping_mul(47),
        );

        // Check if ALL ATA derivations produce optimal bumps (255) for ALL programs and mints
        let mut all_optimal = true;
        for &ata_program_id in ata_program_ids {
            for &mint in mints {
                let (_, bump) = Pubkey::find_program_address(
                    &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()],
                    &ata_program_id,
                );

                if bump != 255 {
                    all_optimal = false;
                    break;
                }
            }
            if !all_optimal {
                break;
            }
        }

        if all_optimal {
            return wallet;
        }
    }

    // Fallback: return last attempt even if not optimal
    random_seeded_pk(
        &AtaVariant::SplAta,
        TestBankId::Benchmarks,
        0,
        AccountTypeId::Wallet,
        search_entropy,
        search_entropy.wrapping_mul(47),
    )
}
