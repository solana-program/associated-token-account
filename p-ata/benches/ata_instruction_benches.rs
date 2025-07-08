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
            len_arg: false,
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
        let base_tests = [
            BaseTestType::CreateIdempotent,
            BaseTestType::Create,
            BaseTestType::CreateTopup,
            BaseTestType::CreateTopupNoCap,
            BaseTestType::CreateToken2022,
            BaseTestType::RecoverNested,
            BaseTestType::RecoverMultisig,
        ];

        let display_variants = [
            TestVariant::BASE, // p-ata base
            TestVariant::RENT, // rent arg
            TestVariant::BUMP, // bump arg
            TestVariant::LEN,  // len arg
        ];

        let mut matrix_results = std::collections::HashMap::new();
        let mut all_results = Vec::new();

        // Run all test combinations
        for base_test in base_tests {
            println!("\n--- Testing variant {} ---", base_test.name());

            // Select appropriate P-ATA implementation for this test
            let pata_impl =
                Self::select_pata_implementation(base_test, pata_legacy_impl, pata_prefunded_impl);

            let supported_variants = base_test.supported_variants();
            let mut test_row = std::collections::HashMap::new();

            // Run all supported variants for display
            for variant in &supported_variants {
                if display_variants.contains(variant) {
                    let test_name = format!("{}_{}", base_test.name(), variant.test_suffix());
                    let comparison = Self::run_single_test_comparison(
                        &test_name,
                        base_test,
                        *variant,
                        pata_impl,
                        spl_impl,
                        token_program_id,
                        &pata_legacy_impl.program_id,
                    );

                    // Print immediate detailed results for debugging
                    Self::print_test_results(&comparison, false);

                    all_results.push(comparison.clone());
                    test_row.insert(*variant, comparison);
                }
            }

            // Run actual "all optimizations" test - combine all applicable optimizations
            let all_optimizations_variant = Self::get_all_optimizations_variant(base_test);
            if let Some(all_opt_variant) = all_optimizations_variant {
                let test_name = format!("{}_all_optimizations", base_test.name());
                println!("  Running {} (all applicable optimizations)", test_name);
                let comparison = Self::run_single_test_comparison(
                    &test_name,
                    base_test,
                    all_opt_variant,
                    pata_impl,
                    spl_impl,
                    token_program_id,
                    &pata_legacy_impl.program_id,
                );

                // Print immediate detailed results for debugging
                Self::print_test_results(&comparison, false);

                all_results.push(comparison.clone());

                // Add to matrix with special marker
                let all_opt_marker = TestVariant {
                    rent_arg: true,
                    bump_arg: true,
                    len_arg: true,
                }; // Special marker for display
                test_row.insert(all_opt_marker, comparison);
            }

            matrix_results.insert(base_test, test_row);
        }

        Self::print_matrix_results(&matrix_results, &display_variants);
        Self::print_compatibility_summary(&all_results);
        Self::output_matrix_data(&matrix_results, &display_variants);
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
        // SPL ATA will strip variant-specific instruction data in adapt_instruction_data()
        let (original_ix, original_accounts) = CommonTestCaseBuilder::build_test_case(
            base_test,
            variant, // Use same variant for consistent address generation
            spl_impl,
            token_program_id,
        );

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

    /// Determine the actual "all optimizations" variant for each test type
    /// This combines all meaningful optimizations for the specific test, not just everything
    fn get_all_optimizations_variant(base_test: BaseTestType) -> Option<TestVariant> {
        match base_test {
            BaseTestType::Create => Some(TestVariant::RENT_BUMP), // rent + bump
            BaseTestType::CreateIdempotent => Some(TestVariant::RENT), // only rent makes sense
            BaseTestType::CreateTopup => Some(TestVariant::RENT_BUMP), // rent + bump
            BaseTestType::CreateTopupNoCap => Some(TestVariant::RENT_BUMP), // rent + bump
            BaseTestType::CreateToken2022 => Some(TestVariant::RENT_BUMP_LEN), // rent + bump + len
            BaseTestType::RecoverNested => Some(TestVariant::BUMP), // only bump makes sense
            BaseTestType::RecoverMultisig => Some(TestVariant::BUMP), // only bump makes sense
            _ => None,
        }
    }

    fn print_matrix_results(
        matrix_results: &std::collections::HashMap<
            BaseTestType,
            std::collections::HashMap<TestVariant, ComparisonResult>,
        >,
        display_variants: &[TestVariant],
    ) {
        println!("\n=== PERFORMANCE MATRIX RESULTS ===");

        // Create the full column set: SPL ATA + P-ATA variants + "all optimizations"
        let all_opt_variant = TestVariant {
            rent_arg: true,
            bump_arg: true,
            len_arg: true,
        };
        let mut columns = vec![TestVariant::BASE]; // This will be used for SPL ATA data
        columns.extend_from_slice(display_variants); // This includes BASE for p-ata, plus rent, bump, len
        columns.push(all_opt_variant);

        // Print header with proper column names
        print!("{:<20}", "Test");
        for (i, variant) in columns.iter().enumerate() {
            let column_name = if i == 0 {
                "SPL ATA" // First column shows SPL ATA numbers
            } else {
                variant.column_name()
            };
            print!(" | {:>15}", column_name);
        }
        println!();

        // Print separator
        print!("{:-<20}", "");
        for _ in &columns {
            print!("-+-{:-<15}", "");
        }
        println!();

        // Print test rows
        for (base_test, test_row) in matrix_results {
            print!("{:<20}", base_test.name());
            for (i, variant) in columns.iter().enumerate() {
                if let Some(result) = test_row.get(variant) {
                    let compute_units = if i == 0 {
                        // First column: show SPL ATA numbers (SPL ATA)
                        if result.spl_ata.success && result.spl_ata.compute_units > 0 {
                            result.spl_ata.compute_units
                        } else {
                            0
                        }
                    } else {
                        // All other columns: show P-ATA numbers for the specific variant
                        if result.p_ata.success && result.p_ata.compute_units > 0 {
                            result.p_ata.compute_units
                        } else {
                            0
                        }
                    };

                    if compute_units > 0 {
                        print!(" | {:>15}", compute_units);
                    } else {
                        print!(" | {:>15}", "");
                    }
                } else {
                    print!(" | {:>15}", "");
                }
            }
            println!();
        }
    }

    fn print_test_results(result: &ComparisonResult, show_debug: bool) {
        print!("--- Testing {} --- ", result.test_name);

        // Check if we need detailed output (problems detected)
        let needs_detailed_output = matches!(
            result.compatibility_status,
            common::CompatibilityStatus::AccountMismatch
                | common::CompatibilityStatus::IncompatibleSuccess
                | common::CompatibilityStatus::IncompatibleFailure
        );

        match result.compatibility_status {
            common::CompatibilityStatus::Identical => {
                println!("✅ Byte-for-Byte Identical",);
            }
            common::CompatibilityStatus::BothRejected => {
                println!("❌ Both failed (compatible)");
            }
            common::CompatibilityStatus::AccountMismatch => {
                println!("🔴 ACCOUNT STATE MISMATCH!");
                println!("      Both succeeded but produced different account states");
            }
            common::CompatibilityStatus::IncompatibleFailure => {
                println!("⚠️ Different error types");
                println!("      Both failed but with incompatible error messages");
            }
            common::CompatibilityStatus::IncompatibleSuccess => {
                println!("🚨 INCOMPATIBLE SUCCESS/FAILURE!");
                if result.p_ata.success && !result.spl_ata.success {
                    println!("      P-ATA succeeded where SPL ATA failed");
                } else if !result.p_ata.success && result.spl_ata.success {
                    println!("      SPL ATA succeeded where P-ATA failed");
                }
            }
            common::CompatibilityStatus::OptimizedBehavior => {
                println!("🚀 P-ATA optimization working");
            }
        }

        // Show detailed debugging information only when there are problems
        if needs_detailed_output || show_debug {
            println!(
                "      P-ATA:    {} CUs | {}",
                result.p_ata.compute_units,
                if result.p_ata.success {
                    "Success"
                } else {
                    "Failed"
                }
            );
            println!(
                "      SPL ATA: {} CUs | {}",
                result.spl_ata.compute_units,
                if result.spl_ata.success {
                    "Success"
                } else {
                    "Failed"
                }
            );

            // Show error messages
            if !result.p_ata.success {
                if let Some(ref error) = result.p_ata.error_message {
                    println!("      P-ATA Error: {}", error);
                }
            }
            if !result.spl_ata.success {
                if let Some(ref error) = result.spl_ata.error_message {
                    println!("      SPL ATA Error: {}", error);
                }
            }

            // Show captured debug output if available and non-empty
            if !result.p_ata.captured_output.is_empty() {
                println!("      P-ATA Debug Output:");
                for line in result.p_ata.captured_output.lines() {
                    println!("        {}", line);
                }
            }
            if !result.spl_ata.captured_output.is_empty() {
                println!("      SPL ATA Debug Output:");
                for line in result.spl_ata.captured_output.lines() {
                    println!("        {}", line);
                }
            }
        }
    }

    fn print_compatibility_summary(all_results: &[ComparisonResult]) {
        println!("\n=== COMPATIBILITY ANALYSIS SUMMARY ===");

        let mut identical_count = 0;
        let mut optimized_count = 0;
        let mut account_mismatch_count = 0;
        let mut incompatible_failure_count = 0;
        let mut incompatible_success_count = 0;
        let mut both_rejected_count = 0;

        let mut concerning_results = Vec::new();

        for result in all_results {
            match result.compatibility_status {
                common::CompatibilityStatus::Identical => identical_count += 1,
                common::CompatibilityStatus::OptimizedBehavior => optimized_count += 1,
                common::CompatibilityStatus::BothRejected => both_rejected_count += 1,
                common::CompatibilityStatus::AccountMismatch => {
                    account_mismatch_count += 1;
                    concerning_results.push(result);
                }
                common::CompatibilityStatus::IncompatibleFailure => {
                    incompatible_failure_count += 1;
                    concerning_results.push(result);
                }
                common::CompatibilityStatus::IncompatibleSuccess => {
                    incompatible_success_count += 1;
                    concerning_results.push(result);
                }
            }
        }

        println!("Total Tests: {}", all_results.len());
        println!(
            "  ✅ P-ATA Passed Byte-for-Byte Identical with SPL ATA: {}",
            identical_count
        );
        println!(
            "  🚀 P-ATA Optimizations Passed (not relevant for SPL ATA): {}",
            optimized_count
        );
        println!("  ❌ Both Rejected Unexpectedly: {}", both_rejected_count);

        if !concerning_results.is_empty() {
            println!("\n⚠️  CONCERNING COMPATIBILITY ISSUES:");
            if account_mismatch_count > 0 {
                println!("  🔴 Account State Mismatches: {}", account_mismatch_count);
            }
            if incompatible_failure_count > 0 {
                println!(
                    "  🔴 Incompatible Failure Modes: {}",
                    incompatible_failure_count
                );
            }
            if incompatible_success_count > 0 {
                println!(
                    "  🔴 Incompatible Success/Failure: {}",
                    incompatible_success_count
                );
            }

            println!("\n  Detailed Issues:");
            for result in &concerning_results {
                println!(
                    "    {} - {:?}",
                    result.test_name, result.compatibility_status
                );
                if !result.p_ata.success {
                    if let Some(ref error) = result.p_ata.error_message {
                        println!("      P-ATA Error: {}", error);
                    }
                }
                if !result.spl_ata.success {
                    if let Some(ref error) = result.spl_ata.error_message {
                        println!("      SPL ATA Error: {}", error);
                    }
                }
            }
        } else {
            println!("\n✅ All tests show compatible behavior!");
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

    fn output_matrix_data(
        matrix_results: &std::collections::HashMap<
            BaseTestType,
            std::collections::HashMap<TestVariant, ComparisonResult>,
        >,
        display_variants: &[TestVariant],
    ) {
        let mut json_tests = std::collections::HashMap::new();

        // Create the full column set: display variants + "all optimizations"
        let all_opt_variant = TestVariant {
            rent_arg: true,
            bump_arg: true,
            len_arg: true,
        };
        let mut columns = display_variants.to_vec();
        columns.push(all_opt_variant);

        for (base_test, test_row) in matrix_results {
            let mut test_variants = std::collections::HashMap::new();

            for variant in &columns {
                if let Some(result) = test_row.get(variant) {
                    if result.p_ata.success && result.p_ata.compute_units > 0 {
                        let spl_ata_cu = if result.spl_ata.success {
                            result.spl_ata.compute_units
                        } else {
                            0
                        };

                        let compatibility = match result.compatibility_status {
                            common::CompatibilityStatus::Identical => "identical",
                            common::CompatibilityStatus::OptimizedBehavior => "optimized",
                            _ => "other",
                        };

                        let spl_ata_cu_str = if spl_ata_cu > 0 {
                            spl_ata_cu.to_string()
                        } else {
                            "null".to_string()
                        };

                        test_variants.insert(variant.column_name().replace(" ", "_"), format!(
                            r#"{{"p_ata_cu": {}, "spl_ata_cu": {}, "compatibility": "{}", "type": "performance_test"}}"#,
                            result.p_ata.compute_units,
                            spl_ata_cu_str,
                            compatibility
                        ));
                    }
                }
            }

            if !test_variants.is_empty() {
                json_tests.insert(base_test.name(), test_variants);
            }
        }

        // Build JSON manually
        let mut json_entries = Vec::new();
        for (test_name, test_variants) in json_tests {
            let mut variant_entries = Vec::new();
            for (variant_name, variant_data) in test_variants {
                variant_entries.push(format!(r#"    "{}": {}"#, variant_name, variant_data));
            }

            let test_entry = format!(
                r#"  "{}": {{
{}
  }}"#,
                test_name,
                variant_entries.join(",\n")
            );
            json_entries.push(test_entry);
        }

        let output = format!(
            r#"{{
  "timestamp": {},
  "performance_tests": {{
{}
  }}
}}"#,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            json_entries.join(",\n")
        );

        // Create benchmark_results directory if it doesn't exist
        std::fs::create_dir_all("benchmark_results").ok();

        // Write performance results
        if let Err(e) = std::fs::write("benchmark_results/performance_results.json", output) {
            eprintln!("Failed to write performance results: {}", e);
        } else {
            println!("\n📊 Matrix results written to benchmark_results/performance_results.json");
        }
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
    // TODO: fix
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
