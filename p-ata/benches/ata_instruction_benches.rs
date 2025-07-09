use {
    mollusk_svm::Mollusk, solana_account::Account, solana_instruction::Instruction, solana_logger,
    solana_pubkey::Pubkey,
};

#[path = "common.rs"]
mod common;
use common::*;

mod common_builders;
use common_builders::CommonTestCaseBuilder;

mod account_comparison;
use account_comparison::{AccountComparisonService, ComparisonFormatter};

#[macro_use]
mod formatter;

struct TestConfiguration {
    base_test: BaseTestType,
    variants: &'static [TestVariant],
}

/// Master list of base tests and the P-ATA variants we actually run/display.
static TEST_CONFIGS: &[TestConfiguration] = &[
    TestConfiguration {
        base_test: BaseTestType::CreateIdempotent,
        variants: &[
            TestVariant::BASE,
            TestVariant::RENT,
            TestVariant::BUMP,
            TestVariant::RENT_BUMP,
        ],
    },
    TestConfiguration {
        base_test: BaseTestType::Create,
        variants: &[
            TestVariant::BASE,
            TestVariant::RENT,
            TestVariant::BUMP,
            TestVariant::RENT_BUMP,
        ],
    },
    TestConfiguration {
        base_test: BaseTestType::CreateTopup,
        variants: &[
            TestVariant::BASE,
            TestVariant::RENT,
            TestVariant::BUMP,
            TestVariant::RENT_BUMP,
        ],
    },
    TestConfiguration {
        base_test: BaseTestType::CreateTopupNoCap,
        variants: &[
            TestVariant::BASE,
            TestVariant::RENT,
            TestVariant::BUMP,
            TestVariant::RENT_BUMP,
        ],
    },
    TestConfiguration {
        base_test: BaseTestType::CreateToken2022,
        variants: &[
            TestVariant::BASE,
            TestVariant::RENT,
            TestVariant::BUMP,
            TestVariant::RENT_BUMP,
            TestVariant::BUMP_LEN,
            TestVariant::RENT_BUMP_LEN,
        ],
    },
    TestConfiguration {
        base_test: BaseTestType::RecoverNested,
        variants: &[TestVariant::BASE, TestVariant::RENT, TestVariant::BUMP],
    },
    TestConfiguration {
        base_test: BaseTestType::RecoverMultisig,
        variants: &[TestVariant::BASE, TestVariant::RENT, TestVariant::BUMP],
    },
];

// ============================ SETUP AND CONFIGURATION =============================

impl BenchmarkSetup {
    fn validate_ata_setup(
        mollusk: &Mollusk,
        ata_implementation: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> Result<(), String> {
        let test_variant = TestVariant {
            rent_arg: false,
            bump_arg: false,
            token_account_len_arg: false,
        };
        let (test_ix, test_accounts) = CommonTestCaseBuilder::build_test_case(
            BaseTestType::Create,
            test_variant,
            ata_implementation,
            token_program_id,
        );
        // println!("Running test case: {:?}", test_ix);
        let result = mollusk.process_instruction(&test_ix, &test_accounts);

        match result.program_result {
            mollusk_svm::result::ProgramResult::Success => {
                println!(
                    "✓ ATA setup validation passed for {}",
                    ata_implementation.name
                );
                Ok(())
            }
            _ => Err(format!(
                "Setup validation failed for {}: {:?}",
                ata_implementation.name, result.program_result
            )),
        }
    }
}

// =============================== COMPARISON FRAMEWORK ===============================

struct PerformanceTestOrchestrator;

impl PerformanceTestOrchestrator {
    /// Select the appropriate P-ATA implementation for a given test
    fn select_pata_implementation<'a>(
        base_test: BaseTestType,
        legacy_impl: &'a AtaImplementation,
        prefunded_impl: &'a AtaImplementation,
    ) -> &'a AtaImplementation {
        match base_test.required_pata_variant() {
            AtaVariant::PAtaPrefunded => {
                println!("Using P-ATA prefunded binary for {}", base_test.name());
                prefunded_impl
            }
            _ => legacy_impl,
        }
    }

    fn run_full_comparison(
        pata_legacy_impl: &AtaImplementation,
        pata_prefunded_impl: &AtaImplementation,
        spl_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> Vec<ComparisonResult> {
        println!("\n=== P-ATA VS SPL ATA MATRIX COMPARISON ===");
        println!("P-ATA Legacy Program ID: {}", pata_legacy_impl.program_id);
        println!(
            "P-ATA Prefunded Program ID: {}",
            pata_prefunded_impl.program_id
        );
        println!("SPL ATA Program ID: {}", spl_impl.program_id);
        println!("Token Program ID: {}", token_program_id);

        Self::run_matrix_comparison_with_variants(
            pata_legacy_impl,
            pata_prefunded_impl,
            spl_impl,
            token_program_id,
        )
    }

    fn run_matrix_comparison_with_variants(
        pata_legacy_impl: &AtaImplementation,
        pata_prefunded_impl: &AtaImplementation,
        spl_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> Vec<ComparisonResult> {
        let display_variants = [TestVariant::BASE, TestVariant::RENT, TestVariant::BUMP];

        let mut matrix_results = std::collections::HashMap::new();
        let mut all_results = Vec::new();

        // Run all test configurations
        for config in TEST_CONFIGS {
            let base_test = config.base_test;
            println!("\n--- Testing variant {} ---", base_test.name());

            // Select appropriate P-ATA implementation for this test
            let pata_impl =
                Self::select_pata_implementation(base_test, pata_legacy_impl, pata_prefunded_impl);

            let mut test_row = std::collections::HashMap::new();

            // Run all configured variants for this test row
            for &variant in config.variants {
                let test_name = format!("{}_{}", base_test.name(), variant.test_suffix());
                let comparison = Self::run_single_test_comparison(
                    &test_name,
                    base_test,
                    variant,
                    pata_impl,
                    spl_impl,
                    token_program_id,
                    &pata_legacy_impl.program_id,
                );

                // Print immediate detailed results for debugging
                formatter::print_test_results(&comparison, false);

                all_results.push(comparison.clone());
                test_row.insert(variant, comparison);
            }

            matrix_results.insert(base_test, test_row);
        }

        formatter::print_matrix_results(&matrix_results, &display_variants);
        formatter::print_compatibility_summary(&all_results);
        formatter::output_matrix_data(&matrix_results, &display_variants);
        all_results
    }

    fn run_single_test_comparison(
        test_name: &str,
        base_test: BaseTestType,
        variant: TestVariant,
        p_ata_impl: &AtaImplementation,
        spl_impl: &AtaImplementation,
        token_program_id: &Pubkey,
        _standard_program_id: &Pubkey,
    ) -> ComparisonResult {
        let (p_ata_ix, p_ata_accounts) = CommonTestCaseBuilder::build_test_case(
            base_test,
            variant,
            p_ata_impl,
            token_program_id,
        );

        // For address generation consistency, use the same variant as P-ATA
        let (original_ix, original_accounts) =
            CommonTestCaseBuilder::build_test_case(base_test, variant, spl_impl, token_program_id);

        // Handle special cases where original ATA doesn't support the feature
        let mut original_result = if Self::original_supports_test(base_test) {
            common::BenchmarkRunner::run_single_benchmark(
                test_name,
                &original_ix,
                &original_accounts,
                spl_impl,
                token_program_id,
            )
        } else {
            common::BenchmarkResult {
                implementation: "spl-ata".to_string(),
                test_name: test_name.to_string(),
                success: false,
                compute_units: 0,
                error_message: Some(format!("Original ATA doesn't support {}", base_test.name())),
                captured_output: String::new(),
            }
        };

        let mut p_ata_result = common::BenchmarkRunner::run_single_benchmark(
            test_name,
            &p_ata_ix,
            &p_ata_accounts,
            p_ata_impl,
            token_program_id,
        );

        // Enhanced comparison with account state verification
        let mut comparison = Self::create_enhanced_comparison_result(
            test_name,
            p_ata_result.clone(),
            original_result.clone(),
            &p_ata_ix,
            &p_ata_accounts,
            &original_ix,
            &original_accounts,
            token_program_id,
        );

        // Check if we need debug logging for problematic results
        let needs_debug_logging = Self::is_problematic_result(&comparison);

        if needs_debug_logging {
            // Re-run with debug logging to capture verbose output
            p_ata_result = common::BenchmarkRunner::run_single_benchmark_with_debug(
                test_name,
                &p_ata_ix,
                &p_ata_accounts,
                p_ata_impl,
                token_program_id,
            );

            if Self::original_supports_test(base_test) {
                // Also re-run original ATA with debug logging
                original_result = common::BenchmarkRunner::run_single_benchmark_with_debug(
                    test_name,
                    &original_ix,
                    &original_accounts,
                    spl_impl,
                    token_program_id,
                );
            }

            // Update comparison result with debug output
            comparison = Self::create_enhanced_comparison_result(
                test_name,
                p_ata_result,
                original_result,
                &p_ata_ix,
                &p_ata_accounts,
                &original_ix,
                &original_accounts,
                token_program_id,
            );
        }

        comparison
    }

    /// Check if a comparison result is problematic and needs debug logging
    fn is_problematic_result(result: &ComparisonResult) -> bool {
        match result.compatibility_status {
            // Security issues - definitely need debug logs
            common::CompatibilityStatus::IncompatibleSuccess => true,
            // Account state mismatches - need debug logs
            common::CompatibilityStatus::AccountMismatch => true,
            // Incompatible failure modes - might need debug logs
            common::CompatibilityStatus::IncompatibleFailure => true,
            // All other cases are expected or acceptable
            _ => false,
        }
    }

    fn original_supports_test(base_test: BaseTestType) -> bool {
        match base_test {
            BaseTestType::RecoverMultisig => false, // SPL ATA doesn't support multisig recovery
            _ => true,
        }
    }

    fn create_enhanced_comparison_result(
        test_name: &str,
        p_ata_result: common::BenchmarkResult,
        spl_ata_result: common::BenchmarkResult,
        p_ata_ix: &Instruction,
        p_ata_accounts: &[(Pubkey, Account)],
        spl_ata_ix: &Instruction,
        spl_ata_accounts: &[(Pubkey, Account)],
        token_program_id: &Pubkey,
    ) -> ComparisonResult {
        // Start with basic comparison
        let mut comparison = common::BenchmarkRunner::create_comparison_result(
            test_name,
            p_ata_result.clone(),
            spl_ata_result.clone(),
        );

        // If both succeeded, perform account state comparison using new service
        if p_ata_result.success && spl_ata_result.success {
            let mollusk = common::BenchmarkRunner::create_mollusk_for_all_ata_implementations(
                token_program_id,
            );

            // Execute both instructions and capture final account states
            let p_ata_execution = mollusk.process_instruction(p_ata_ix, p_ata_accounts);
            let spl_ata_execution = mollusk.process_instruction(spl_ata_ix, spl_ata_accounts);

            if let (
                mollusk_svm::result::ProgramResult::Success,
                mollusk_svm::result::ProgramResult::Success,
            ) = (
                &p_ata_execution.program_result,
                &spl_ata_execution.program_result,
            ) {
                // Use the new comparison service
                let comparison_service = AccountComparisonService::new();
                let comparison_results = comparison_service.compare_account_states(
                    &p_ata_execution.resulting_accounts,
                    &spl_ata_execution.resulting_accounts,
                    &p_ata_ix.accounts,
                    &spl_ata_ix.accounts,
                );

                // Determine compatibility based on comparison results
                let all_equivalent =
                    comparison_service.all_accounts_equivalent(&comparison_results);
                let has_expected_differences =
                    comparison_service.has_expected_differences(&comparison_results);

                if !all_equivalent {
                    comparison.compatibility_status = common::CompatibilityStatus::AccountMismatch;
                }

                // Format and display comparison results if there are any issues
                if !all_equivalent || has_expected_differences {
                    let formatter = ComparisonFormatter::new();
                    let debug_output = formatter.format_comparison_results(&comparison_results);

                    if !debug_output.is_empty() {
                        println!("\nACCOUNT STATE COMPARISON:");
                        for line in debug_output {
                            println!("{}", line);
                        }
                    }

                    if !all_equivalent {
                        println!("\n❌ Account state differences detected!");
                    } else if has_expected_differences {
                        println!("\n📊 Expected differences detected (P-ATA optimizations)");
                    }
                }
            }
        }

        comparison
    }
}

// ================================= MAIN =====================================

fn main() {
    // Completely suppress debug output from Mollusk and Solana runtime unless full-debug-logs is enabled
    #[cfg(not(feature = "full-debug-logs"))]
    {
        std::env::set_var("RUST_LOG", "error");
        // Setup quiet logging by default - only show warnings and errors
        let _ = solana_logger::setup_with(
            "error,solana_runtime=error,solana_program_runtime=error,mollusk=error",
        );
    }

    #[cfg(feature = "full-debug-logs")]
    {
        std::env::set_var("RUST_LOG", "debug");
        // Setup debug logging for full-debug-logs feature
        let _ = solana_logger::setup_with(
            "debug,solana_runtime=debug,solana_program_runtime=debug,mollusk=debug",
        );
    }

    // Get manifest directory and setup environment
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    println!("CARGO_MANIFEST_DIR: {}", manifest_dir);
    println!("🔨 P-ATA vs Original ATA Benchmark Suite");

    BenchmarkSetup::setup_sbf_environment(manifest_dir);

    let impls = AtaImplementation::all();
    let program_ids = BenchmarkSetup::load_program_ids(manifest_dir);

    println!(
        "P-ATA Legacy Program ID: {}",
        impls.pata_legacy_impl.program_id
    );
    println!(
        "P-ATA Prefunded Program ID: {}",
        impls.pata_prefunded_impl.program_id
    );
    println!("Token Program ID: {}", program_ids.token_program_id);

    println!("SPL ATA Program ID: {}", impls.spl_impl.program_id);

    println!("\n🔍 Running comparison between implementations");

    let mollusk = common::BenchmarkRunner::create_mollusk_for_all_ata_implementations(
        &program_ids.token_program_id,
    );

    // Validate prefunded P-ATA setup
    if let Err(e) = BenchmarkSetup::validate_ata_setup(
        &mollusk,
        &impls.pata_prefunded_impl,
        &program_ids.token_program_id,
    ) {
        panic!("P-ATA prefunded benchmark setup validation failed: {}", e);
    }

    // Validate SPL ATA setup
    if let Err(e) =
        BenchmarkSetup::validate_ata_setup(&mollusk, &impls.spl_impl, &program_ids.token_program_id)
    {
        panic!("SPL ATA benchmark setup validation failed: {}", e);
    }

    // Validate legacy P-ATA (without prefunded) setup
    println!(
        "Validating legacy P-ATA setup with token program {}",
        program_ids.token_program_id
    );
    if let Err(e) = BenchmarkSetup::validate_ata_setup(
        &mollusk,
        &impls.pata_legacy_impl,
        &program_ids.token_program_id,
    ) {
        panic!("Legacy P-ATA benchmark setup validation failed: {}", e);
    }

    // Run comparison using the appropriate P-ATA implementation for each test
    let _comparison_results = PerformanceTestOrchestrator::run_full_comparison(
        &impls.pata_legacy_impl,
        &impls.pata_prefunded_impl,
        &impls.spl_impl,
        &program_ids.token_program_id,
    );

    println!("\n✅ Comprehensive comparison completed successfully");
    println!("Total test results: {}", _comparison_results.len());
}
