use {
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

mod consolidated_builders;
use consolidated_builders::ConsolidatedTestCaseBuilder;

struct TestCaseBuilder;

impl TestCaseBuilder {
    fn build_test_case(
        base_test: BaseTestType,
        variant: TestVariant,
        ata_implementation: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        ConsolidatedTestCaseBuilder::build_test_case(
            base_test,
            variant,
            ata_implementation,
            token_program_id,
        )
    }

    fn build_recover(
        ata_implementation: &AtaImplementation,
        token_program_id: &Pubkey,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        // Fixed mints and wallets - independent of ATA program
        let owner_mint = const_pk(20);
        let wallet = const_pk(30);
        let nested_mint = const_pk(40);

        let (owner_ata, _) = Pubkey::find_program_address(
            &[
                wallet.as_ref(),
                token_program_id.as_ref(),
                owner_mint.as_ref(),
            ],
            &ata_implementation.program_id,
        );

        let (nested_ata, _) = Pubkey::find_program_address(
            &[
                owner_ata.as_ref(),
                token_program_id.as_ref(),
                nested_mint.as_ref(),
            ],
            &ata_implementation.program_id,
        );

        let (dest_ata, _) = Pubkey::find_program_address(
            &[
                wallet.as_ref(),
                token_program_id.as_ref(),
                nested_mint.as_ref(),
            ],
            &ata_implementation.program_id,
        );

        let accounts = vec![
            (
                nested_ata,
                AccountBuilder::token_account(&nested_mint, &owner_ata, 100, token_program_id),
            ),
            (
                nested_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (
                dest_ata,
                AccountBuilder::token_account(&nested_mint, &wallet, 0, token_program_id),
            ),
            (
                owner_ata,
                AccountBuilder::token_account(&owner_mint, &wallet, 0, token_program_id),
            ),
            (
                owner_mint,
                AccountBuilder::mint_account(0, token_program_id, false),
            ),
            (wallet, AccountBuilder::system_account(1_000_000_000)),
            (
                *token_program_id,
                AccountBuilder::executable_program(LOADER_V3),
            ),
            (
                Pubkey::from(spl_token_interface::program::ID),
                AccountBuilder::executable_program(LOADER_V3),
            ),
        ];

        let raw_data = vec![2u8]; // RecoverNested discriminator
        let ix = Instruction {
            program_id: ata_implementation.program_id,
            accounts: vec![
                AccountMeta::new(nested_ata, false),
                AccountMeta::new_readonly(nested_mint, false),
                AccountMeta::new(dest_ata, false),
                AccountMeta::new(owner_ata, false),
                AccountMeta::new_readonly(owner_mint, false),
                AccountMeta::new(wallet, true),
                AccountMeta::new_readonly(*token_program_id, false),
                AccountMeta::new_readonly(Pubkey::from(spl_token_interface::program::ID), false),
            ],
            data: ata_implementation.adapt_instruction_data(raw_data),
        };

        (ix, accounts)
    }

    #[allow(clippy::too_many_arguments)]
    fn build_create_with_bump(
        ata_implementation: &AtaImplementation,
        token_program_id: &Pubkey,
        extended_mint: bool,
        with_rent: bool,
    ) -> (Instruction, Vec<(Pubkey, Account)>) {
        let base_offset = calculate_bump_base_offset(extended_mint, with_rent);
        let (payer, mint, wallet) = build_base_test_accounts(
            base_offset,
            token_program_id,
            &ata_implementation.program_id,
        );

        let (ata, bump) = Pubkey::find_program_address(
            &[wallet.as_ref(), token_program_id.as_ref(), mint.as_ref()],
            &ata_implementation.program_id,
        );

        let mut accounts = vec![
            (payer, AccountBuilder::system_account(1_000_000_000)),
            (ata, AccountBuilder::system_account(0)),
            (wallet, AccountBuilder::system_account(0)),
            (
                mint,
                AccountBuilder::mint_account(0, token_program_id, extended_mint),
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

        if with_rent {
            accounts.push((rent::id(), AccountBuilder::rent_sysvar()));
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

        let raw_data = build_instruction_data(0, &[bump]); // Create instruction (discriminator 0) with bump
        let ix = Instruction {
            program_id: ata_implementation.program_id,
            accounts: metas,
            data: ata_implementation.adapt_instruction_data(raw_data),
        };

        (ix, accounts)
    }

    fn build_worst_case_bump_scenario(
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) -> (
        (Instruction, Vec<(Pubkey, Account)>),
        (Instruction, Vec<(Pubkey, Account)>),
    ) {
        // Use fixed wallet and mint - independent of ATA program
        // These values were chosen to produce a low bump for worst-case testing
        let worst_wallet = const_pk(200);
        let mint = const_pk(199); // Fixed mint for consistency

        let (ata, bump) = Pubkey::find_program_address(
            &[
                worst_wallet.as_ref(),
                token_program_id.as_ref(),
                mint.as_ref(),
            ],
            program_id,
        );

        println!(
            "Worst case bump scenario: wallet={}, bump={} (lower = more expensive)",
            worst_wallet, bump
        );

        let accounts = vec![
            (const_pk(198), AccountBuilder::system_account(1_000_000_000)), // payer
            (ata, AccountBuilder::system_account(0)),
            (worst_wallet, AccountBuilder::system_account(0)),
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

        let metas = vec![
            AccountMeta::new(const_pk(198), true), // payer
            AccountMeta::new(ata, false),
            AccountMeta::new_readonly(worst_wallet, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            AccountMeta::new_readonly(*token_program_id, false),
        ];

        // Create instruction (expensive find_program_address)
        let create_ix = Instruction {
            program_id: *program_id,
            accounts: metas.clone(),
            data: vec![0u8], // Create discriminator
        };

        // CreateWithBump instruction (skips find_program_address)
        let create_with_bump_ix = Instruction {
            program_id: *program_id,
            accounts: metas,
            data: vec![0u8, bump], // Create discriminator + bump
        };

        (
            (create_ix, accounts.clone()),
            (create_with_bump_ix, accounts),
        )
    }
}

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
        let (test_ix, test_accounts) = TestCaseBuilder::build_test_case(
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
            TestVariant::BASE, // p-ata
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
        let (p_ata_ix, p_ata_accounts) =
            TestCaseBuilder::build_test_case(base_test, variant, p_ata_impl, token_program_id);

        // For original ATA, use base variant (no optimizations) for comparison
        let original_variant = TestVariant::BASE;
        let (original_ix, original_accounts) = TestCaseBuilder::build_test_case(
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

    fn create_empty_result(test_name: &str, variant_name: &str) -> ComparisonResult {
        let empty_benchmark = common::BenchmarkResult {
            implementation: "empty".to_string(),
            test_name: format!("{}_{}", test_name, variant_name),
            success: false,
            compute_units: 0,
            error_message: Some("Unsupported combination".to_string()),
        };

        common::ComparisonRunner::create_comparison_result(
            &format!("{}_{}", test_name, variant_name),
            empty_benchmark.clone(),
            empty_benchmark,
        )
    }

    fn print_matrix_results(
        matrix_results: &std::collections::HashMap<
            BaseTestType,
            std::collections::HashMap<TestVariant, ComparisonResult>,
        >,
        display_variants: &[TestVariant],
    ) {
        println!("\n=== PERFORMANCE MATRIX RESULTS ===");

        // Create the full column set: display variants + "all optimizations"
        let all_opt_variant = TestVariant {
            rent_arg: true,
            bump_arg: true,
            len_arg: true,
        };
        let mut columns = display_variants.to_vec();
        columns.push(all_opt_variant);

        // Print header
        print!("{:<20}", "Test");
        for variant in &columns {
            print!(" | {:>15}", variant.column_name());
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
            for variant in &columns {
                if let Some(result) = test_row.get(variant) {
                    if result.p_ata.success && result.p_ata.compute_units > 0 {
                        print!(" | {:>15}", result.p_ata.compute_units);
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

    fn output_structured_data(results: &[ComparisonResult]) {
        let mut json_entries = Vec::new();

        for result in results {
            // Only include successful comparisons or known optimization cases
            let (p_ata_cu, original_cu, compatibility) =
                match (&result.p_ata.success, &result.original.success) {
                    (true, true) => {
                        let compat = match result.compatibility_status {
                            common::CompatibilityStatus::Identical => "identical",
                            common::CompatibilityStatus::OptimizedBehavior => "optimized",
                            common::CompatibilityStatus::ExpectedDifferences => {
                                "expected_difference"
                            }
                            _ => "unknown",
                        };

                        (
                            result.p_ata.compute_units,
                            result.original.compute_units,
                            compat,
                        )
                    }
                    (true, false) => {
                        // P-ATA works, Original fails - optimization case
                        (result.p_ata.compute_units, 0, "new p-ata case")
                    }
                    _ => continue, // Skip cases where P-ATA fails
                };

            let entry = format!(
                r#"    "{}": {{
      "p_ata_cu": {},
      "original_cu": {},
      "compatibility": "{}",
      "type": "performance_test"
    }}"#,
                result.test_name, p_ata_cu, original_cu, compatibility
            );
            json_entries.push(entry);
        }

        let output = format!(
            r#"{{
  "timestamp": "{}",
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
            println!(
                "\n📊 Performance results written to benchmark_results/performance_results.json"
            );
        }
    }

    fn print_summary(results: &[ComparisonResult]) {
        println!("\n=== BYTE-FOR-BYTE TEST SUMMARY ===");

        // Print each test with color-coded status
        for result in results {
            let status_indicator = match result.compatibility_status {
                CompatibilityStatus::Identical => {
                    // Special handling for create_with_bump - it's a P-ATA optimization
                    if result.test_name == "create_with_bump" {
                        "P-ATA OPTIMIZATION"
                    } else {
                        "\x1b[32m🟢 IDENTICAL\x1b[0m"
                    }
                }
                CompatibilityStatus::OptimizedBehavior => "P-ATA OPTIMIZATION",
                CompatibilityStatus::ExpectedDifferences => {
                    "\x1b[33m🟡 EXPECTED DIFFERENCES\x1b[0m"
                }
                CompatibilityStatus::BothRejected => "\x1b[31m🔴 BOTH REJECTED\x1b[0m",
                CompatibilityStatus::AccountMismatch => "\x1b[31m🔴 ACCOUNT MISMATCH\x1b[0m",
                CompatibilityStatus::IncompatibleFailure => {
                    "\x1b[31m🔴 INCOMPATIBLE FAILURE\x1b[0m"
                }
                CompatibilityStatus::IncompatibleSuccess => {
                    "\x1b[31m🔴 INCOMPATIBLE SUCCESS\x1b[0m"
                }
            };

            let differences = Self::get_test_differences(result);
            let differences_str = if differences.is_empty() {
                String::new()
            } else {
                format!(" ({})", differences.join(", "))
            };

            println!(
                "  {} {:<18}{}",
                status_indicator, result.test_name, differences_str
            );
        }
    }

    fn get_test_differences(result: &ComparisonResult) -> Vec<String> {
        let mut differences = Vec::new();

        match result.test_name.as_str() {
            "create_with_bump" => {
                differences.push("P-ATA uses CreateWithBump".to_string());
            }
            "recover_with_bump" => {
                if !result.original.success {
                    differences.push("Original fails".to_string());
                }
            }
            _ => {}
        }

        differences
    }

    fn format_compute_savings(result: &ComparisonResult) -> String {
        if result.p_ata.success && result.original.success {
            let savings = result.original.compute_units as i64 - result.p_ata.compute_units as i64;
            let percentage = if result.original.compute_units > 0 {
                (savings as f64 / result.original.compute_units as f64) * 100.0
            } else {
                0.0
            };
            format!("[-{:.1}% CUs]", percentage)
        } else if result.p_ata.success && !result.original.success {
            "[P-ATA works]".to_string()
        } else if !result.p_ata.success && result.original.success {
            "[P-ATA fails]".to_string()
        } else {
            "[Both fail]".to_string()
        }
    }
}

// =============================== BENCHMARK RUNNER ===============================

struct BenchmarkRunner;

impl BenchmarkRunner {
    fn run_isolated_benchmark(
        name: &str,
        ix: &Instruction,
        accounts: &[(Pubkey, Account)],
        program_id: &Pubkey,
        token_program_id: &Pubkey,
    ) {
        println!("\n=== Running benchmark: {} ===", name);

        let must_pass = name != "create_token2022_sim";
        run_benchmark_with_validation(name, ix, accounts, program_id, token_program_id, must_pass);
    }

    fn run_all_benchmarks(ata_implementation: &AtaImplementation, token_program_id: &Pubkey) {
        println!(
            "\n=== Running all benchmarks for {} ===",
            ata_implementation.name
        );

        let test_cases = vec![
            (
                "create_base",
                TestCaseBuilder::build_test_case(
                    BaseTestType::Create,
                    TestVariant {
                        rent_arg: false,
                        bump_arg: false,
                        len_arg: false,
                    },
                    ata_implementation,
                    token_program_id,
                ),
            ),
            (
                "create_rent",
                TestCaseBuilder::build_test_case(
                    BaseTestType::Create,
                    TestVariant {
                        rent_arg: true,
                        bump_arg: false,
                        len_arg: false,
                    },
                    ata_implementation,
                    token_program_id,
                ),
            ),
            (
                "create_topup",
                TestCaseBuilder::build_test_case(
                    BaseTestType::CreateTopup,
                    TestVariant {
                        rent_arg: false,
                        bump_arg: false,
                        len_arg: false,
                    },
                    ata_implementation,
                    token_program_id,
                ),
            ),
            (
                "create_idemp",
                TestCaseBuilder::build_test_case(
                    BaseTestType::CreateIdempotent,
                    TestVariant {
                        rent_arg: false,
                        bump_arg: false,
                        len_arg: false,
                    },
                    ata_implementation,
                    token_program_id,
                ),
            ),
            (
                "create_with_bump_base",
                TestCaseBuilder::build_create_with_bump(
                    ata_implementation,
                    token_program_id,
                    false,
                    false,
                ),
            ),
            (
                "create_with_bump_rent",
                TestCaseBuilder::build_create_with_bump(
                    ata_implementation,
                    token_program_id,
                    false,
                    true,
                ),
            ),
            (
                "recover",
                TestCaseBuilder::build_recover(ata_implementation, token_program_id),
            ),
            // Note: Specialized helper functions removed to reduce code duplication
            // These tests should be implemented using the consolidated builder approach
        ];

        for (name, (ix, accounts)) in test_cases {
            Self::run_isolated_benchmark(
                name,
                &ix,
                &accounts,
                &ata_implementation.program_id,
                token_program_id,
            );
        }

        // Run worst-case bump scenario comparison
        Self::run_worst_case_bump_comparison(&ata_implementation.program_id, token_program_id);
    }

    fn run_worst_case_bump_comparison(program_id: &Pubkey, token_program_id: &Pubkey) {
        println!("\n=== Worst-Case Bump Scenario Comparison ===");
        let ((create_ix, create_accounts), (create_with_bump_ix, create_with_bump_accounts)) =
            TestCaseBuilder::build_worst_case_bump_scenario(program_id, token_program_id);

        // Benchmark regular Create (expensive)
        Self::run_isolated_benchmark(
            "worst_case_create",
            &create_ix,
            &create_accounts,
            program_id,
            token_program_id,
        );

        // Benchmark CreateWithBump (optimized)
        Self::run_isolated_benchmark(
            "worst_case_create_with_bump",
            &create_with_bump_ix,
            &create_with_bump_accounts,
            program_id,
            token_program_id,
        );
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

        // Validate standard P-ATA setup
        let standard_mollusk = common::ComparisonRunner::create_mollusk_for_implementation(
            &standard_impl,
            &token_program_id,
        );
        if let Err(e) =
            BenchmarkSetup::validate_ata_setup(&standard_mollusk, &standard_impl, &token_program_id)
        {
            panic!("P-ATA standard benchmark setup validation failed: {}", e);
        }

        // Validate prefunded P-ATA setup if available
        if let Some(ref prefunded_impl) = prefunded_impl {
            let prefunded_mollusk = common::ComparisonRunner::create_mollusk_for_implementation(
                prefunded_impl,
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
        let original_mollusk = common::ComparisonRunner::create_mollusk_for_implementation(
            &original_impl,
            &token_program_id,
        );
        if let Err(e) =
            BenchmarkSetup::validate_ata_setup(&original_mollusk, &original_impl, &token_program_id)
        {
            panic!("Original ATA benchmark setup validation failed: {}", e);
        }

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
        println!("\n🔧 Running P-ATA only benchmarks (original ATA not built)");
        println!("   💡 To enable comparison, run: cargo bench --features build-programs");

        // Setup Mollusk with standard P-ATA
        let mollusk = common::fresh_mollusk(&standard_program_id, &token_program_id);

        // Validate the setup works
        if let Err(e) =
            BenchmarkSetup::validate_ata_setup(&mollusk, &standard_impl, &token_program_id)
        {
            panic!("P-ATA standard benchmark setup validation failed: {}", e);
        }

        // Run P-ATA benchmarks
        BenchmarkRunner::run_all_benchmarks(&standard_impl, &token_program_id);

        // Also test prefunded variant if available
        if let Some(ref prefunded_impl) = prefunded_impl {
            println!("\n🔧 Running P-ATA prefunded benchmarks");
            BenchmarkRunner::run_all_benchmarks(prefunded_impl, &token_program_id);
        }

        println!("\n✅ P-ATA benchmarks completed successfully");
    }
}

// ================================= HELPERS =====================================

fn build_account_meta(pubkey: &Pubkey, writable: bool, signer: bool) -> AccountMeta {
    AccountMeta {
        pubkey: *pubkey,
        is_writable: writable,
        is_signer: signer,
    }
}

fn build_ata_instruction_metas(
    payer: &Pubkey,
    ata: &Pubkey,
    wallet: &Pubkey,
    mint: &Pubkey,
    system_prog: &Pubkey,
    token_prog: &Pubkey,
) -> Vec<AccountMeta> {
    vec![
        build_account_meta(payer, true, true), // payer (writable, signer)
        build_account_meta(ata, true, false),  // ata (writable, not signer)
        build_account_meta(wallet, false, false), // wallet (readonly, not signer)
        build_account_meta(mint, false, false), // mint (readonly, not signer)
        build_account_meta(system_prog, false, false), // system program (readonly, not signer)
        build_account_meta(token_prog, false, false), // token program (readonly, not signer)
    ]
}

fn build_base_test_accounts(
    base_offset: u8,
    _token_program_id: &Pubkey,
    _program_id: &Pubkey,
) -> (Pubkey, Pubkey, Pubkey) {
    let payer = const_pk(base_offset);
    let mint = const_pk(base_offset + 1);
    // Wallets are independent of ATA program - use fixed wallet address
    let wallet = const_pk(base_offset + 2);
    (payer, mint, wallet)
}

fn calculate_bump_base_offset(extended_mint: bool, with_rent: bool) -> u8 {
    match (extended_mint, with_rent) {
        (false, false) => 90, // create_with_bump_base
        (false, true) => 95,  // create_with_bump_rent
        (true, false) => 100, // create_with_bump_ext
        (true, true) => 105,  // create_with_bump_ext_rent
    }
}

fn configure_bencher<'a>(
    mollusk: Mollusk,
    _name: &'a str,
    must_pass: bool,
    out_dir: &'a str,
) -> MolluskComputeUnitBencher<'a> {
    let mut bencher = MolluskComputeUnitBencher::new(mollusk).out_dir(out_dir);

    if must_pass {
        bencher = bencher.must_pass(true);
    }

    bencher
}

fn execute_benchmark_case<'a>(
    bencher: MolluskComputeUnitBencher<'a>,
    name: &'a str,
    ix: &'a Instruction,
    accounts: &'a [(Pubkey, Account)],
) -> MolluskComputeUnitBencher<'a> {
    bencher.bench((name, ix, accounts))
}

fn run_benchmark_with_validation(
    name: &str,
    ix: &Instruction,
    accounts: &[(Pubkey, Account)],
    program_id: &Pubkey,
    token_program_id: &Pubkey,
    must_pass: bool,
) {
    let cloned_accounts = common::clone_accounts(accounts);
    let mollusk = common::fresh_mollusk(program_id, token_program_id);
    let bencher = configure_bencher(mollusk, name, must_pass, "../target/benches");
    let mut bencher = execute_benchmark_case(bencher, name, ix, &cloned_accounts);
    bencher.execute();
}
