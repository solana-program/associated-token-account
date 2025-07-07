use {
    crate::common_builders::calculate_test_number,
    mollusk_svm::{program::loader_keys::LOADER_V3, Mollusk},
    mollusk_svm_bencher::MolluskComputeUnitBencher,
    solana_account::Account,
    solana_instruction::{AccountMeta, Instruction},
    solana_logger,
    solana_pubkey::Pubkey,
    solana_sysvar::rent,
};

#[path = "common.rs"]
mod common;
use common::*;

mod common_builders;
use common_builders::CommonTestCaseBuilder;

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

        let result = mollusk.process_instruction(&test_ix, &test_accounts);

        match result.program_result {
            mollusk_svm::result::ProgramResult::Success => {
                println!(
                    "✓ Benchmark setup validation passed for {}",
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

struct ComparisonRunner;

impl ComparisonRunner {
    /// Select the appropriate P-ATA implementation for a given test
    fn select_pata_implementation<'a>(
        base_test: BaseTestType,
        standard_impl: &'a AtaImplementation,
        prefunded_impl: Option<&'a AtaImplementation>,
    ) -> &'a AtaImplementation {
        match base_test.required_pata_variant() {
            AtaVariant::PAtaPrefunded => {
                if let Some(prefunded) = prefunded_impl {
                    println!("Using P-ATA prefunded binary for {}", base_test.name());
                    prefunded
                } else {
                    panic!(
                        "FATAL: {} requires prefunded variant but it's not available!",
                        base_test.name()
                    );
                }
            }
            _ => standard_impl,
        }
    }

    fn run_full_comparison(
        standard_impl: &AtaImplementation,
        prefunded_impl: Option<&AtaImplementation>,
        original_impl: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> Vec<ComparisonResult> {
        println!("\n=== P-ATA VS ORIGINAL ATA MATRIX COMPARISON ===");
        println!("P-ATA Standard Program ID: {}", standard_impl.program_id);
        if let Some(prefunded) = prefunded_impl {
            println!("P-ATA Prefunded Program ID: {}", prefunded.program_id);
        }
        println!("Original Program ID: {}", original_impl.program_id);
        println!("Token Program ID: {}", token_program_id);

        Self::run_matrix_comparison_with_variants(
            standard_impl,
            prefunded_impl,
            original_impl,
            token_program_id,
        )
    }

    fn run_matrix_comparison_with_variants(
        standard_impl: &AtaImplementation,
        prefunded_impl: Option<&AtaImplementation>,
        original_impl: &AtaImplementation,
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
            println!("\n--- Testing {} ---", base_test.name());

            // Select appropriate P-ATA implementation for this test
            let pata_impl =
                Self::select_pata_implementation(base_test, standard_impl, prefunded_impl);

            let supported_variants = base_test.supported_variants();
            let mut test_row = std::collections::HashMap::new();

            // Run all supported variants for display
            for variant in &supported_variants {
                if display_variants.contains(variant) {
                    let test_name = format!("{}_{}", base_test.name(), variant.test_suffix());
                    println!("  Running {}", test_name);
                    let comparison = Self::run_single_test_comparison(
                        &test_name,
                        base_test,
                        *variant,
                        pata_impl,
                        original_impl,
                        token_program_id,
                        &standard_impl.program_id,
                    );
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
                    original_impl,
                    token_program_id,
                    &standard_impl.program_id,
                );
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
        Self::output_matrix_data(&matrix_results, &display_variants);
        all_results
    }

    fn run_single_test_comparison(
        test_name: &str,
        base_test: BaseTestType,
        variant: TestVariant,
        p_ata_impl: &AtaImplementation,
        original_impl: &AtaImplementation,
        token_program_id: &Pubkey,
        _standard_program_id: &Pubkey,
    ) -> ComparisonResult {
        let (p_ata_ix, p_ata_accounts) = CommonTestCaseBuilder::build_test_case(
            base_test,
            variant,
            p_ata_impl,
            token_program_id,
        );

        // For original ATA, use base variant (no optimizations) for comparison
        let original_variant = TestVariant::BASE;
        let (original_ix, original_accounts) = CommonTestCaseBuilder::build_test_case(
            base_test,
            original_variant,
            original_impl,
            token_program_id,
        );

        // Handle special cases where original ATA doesn't support the feature
        let original_result = if Self::original_supports_test(base_test) {
            common::ComparisonRunner::run_single_benchmark(
                test_name,
                &original_ix,
                &original_accounts,
                original_impl,
                token_program_id,
            )
        } else {
            common::BenchmarkResult {
                implementation: "original".to_string(),
                test_name: test_name.to_string(),
                success: false,
                compute_units: 0,
                error_message: Some(format!("Original ATA doesn't support {}", base_test.name())),
            }
        };

        let p_ata_result = common::ComparisonRunner::run_single_benchmark(
            test_name,
            &p_ata_ix,
            &p_ata_accounts,
            p_ata_impl,
            token_program_id,
        );

        common::ComparisonRunner::create_comparison_result(test_name, p_ata_result, original_result)
    }

    fn original_supports_test(base_test: BaseTestType) -> bool {
        match base_test {
            BaseTestType::RecoverMultisig => false, // Original ATA doesn't support multisig recovery
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
                "SPL ATA" // First column shows original ATA numbers
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
                        // First column: show original ATA numbers (SPL ATA)
                        if result.original.success && result.original.compute_units > 0 {
                            result.original.compute_units
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
                        let original_cu = if result.original.success {
                            result.original.compute_units
                        } else {
                            0
                        };

                        let compatibility = match result.compatibility_status {
                            common::CompatibilityStatus::Identical => "identical",
                            common::CompatibilityStatus::OptimizedBehavior => "optimized",
                            common::CompatibilityStatus::ExpectedDifferences => {
                                "expected_difference"
                            }
                            _ => "other",
                        };

                        let original_cu_str = if original_cu > 0 {
                            original_cu.to_string()
                        } else {
                            "null".to_string()
                        };

                        test_variants.insert(variant.column_name().replace(" ", "_"), format!(
                            r#"{{"p_ata_cu": {}, "original_cu": {}, "compatibility": "{}", "type": "performance_test"}}"#,
                            result.p_ata.compute_units,
                            original_cu_str,
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
    // Setup logging
    let _ = solana_logger::setup_with(
        "info,solana_runtime=info,solana_program_runtime=info,mollusk=debug",
    );

    // Get manifest directory and setup environment
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    println!("CARGO_MANIFEST_DIR: {}", manifest_dir);
    println!("🔨 P-ATA vs Original ATA Benchmark Suite");

    BenchmarkSetup::setup_sbf_environment(manifest_dir);

    // Load all available program IDs (P-ATA variants + original)
    let (standard_program_id, prefunded_program_id, original_ata_program_id, token_program_id) =
        BenchmarkSetup::load_all_program_ids(manifest_dir);

    // Create implementation structures for available programs
    let standard_impl = AtaImplementation::p_ata_standard(standard_program_id);
    let prefunded_impl = prefunded_program_id.map(AtaImplementation::p_ata_prefunded);

    println!("P-ATA Standard Program ID: {}", standard_program_id);
    if let Some(prefunded_id) = prefunded_program_id {
        println!("P-ATA Prefunded Program ID: {}", prefunded_id);
    }
    println!("Token Program ID: {}", token_program_id);

    if let Some(original_program_id) = original_ata_program_id {
        // COMPARISON MODE: Original ATA available
        let original_impl = AtaImplementation::original(original_program_id);
        println!("Original ATA Program ID: {}", original_program_id);

        println!("\n🔍 Running comprehensive comparison between implementations");

        // Validate prefunded P-ATA setup if available
        if let Some(ref prefunded_impl) = prefunded_impl {
            let prefunded_mollusk =
                common::ComparisonRunner::create_mollusk_for_all_ata_implementations(
                    &token_program_id,
                );
            if let Err(e) = BenchmarkSetup::validate_ata_setup(
                &prefunded_mollusk,
                prefunded_impl,
                &token_program_id,
            ) {
                panic!("P-ATA prefunded benchmark setup validation failed: {}", e);
            }
        }

        // Validate original ATA setup
        let original_mollusk =
            common::ComparisonRunner::create_mollusk_for_all_ata_implementations(&token_program_id);
        if let Err(e) =
            BenchmarkSetup::validate_ata_setup(&original_mollusk, &original_impl, &token_program_id)
        {
            panic!("Original ATA benchmark setup validation failed: {}", e);
        }

        // Validate standard P-ATA setup
        println!(
            "Validating standard P-ATA setup with token program {}",
            token_program_id
        );
        println!("Standard P-ATA program ID: {}", standard_impl.program_id);

        // Run comparison using the appropriate P-ATA implementation for each test
        let _comparison_results = ComparisonRunner::run_full_comparison(
            &standard_impl,
            prefunded_impl.as_ref(),
            &original_impl,
            &token_program_id,
        );

        println!("\n✅ Comprehensive comparison completed successfully");
        println!("Total test results: {}", _comparison_results.len());
    } else {
        // P-ATA ONLY MODE: Original ATA not available
        println!("\n🔧 Original ATA program not built!");
        println!("   💡 run: cargo bench --features build-programs");
    }
}
