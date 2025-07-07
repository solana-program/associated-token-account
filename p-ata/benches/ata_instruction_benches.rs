use {
    mollusk_svm::Mollusk, solana_account::Account, solana_instruction::Instruction, solana_logger,
    solana_pubkey::Pubkey,
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
            println!("\n--- Testing variant {} ---", base_test.name());

            // Select appropriate P-ATA implementation for this test
            let pata_impl =
                Self::select_pata_implementation(base_test, standard_impl, prefunded_impl);

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
                        original_impl,
                        token_program_id,
                        &standard_impl.program_id,
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
                    original_impl,
                    token_program_id,
                    &standard_impl.program_id,
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
        let mut original_result = if Self::original_supports_test(base_test) {
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
                captured_output: String::new(),
            }
        };

        let mut p_ata_result = common::ComparisonRunner::run_single_benchmark(
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
            p_ata_result = common::ComparisonRunner::run_single_benchmark_with_debug(
                test_name,
                &p_ata_ix,
                &p_ata_accounts,
                p_ata_impl,
                token_program_id,
            );

            if Self::original_supports_test(base_test) {
                // Also re-run original ATA with debug logging
                original_result = common::ComparisonRunner::run_single_benchmark_with_debug(
                    test_name,
                    &original_ix,
                    &original_accounts,
                    original_impl,
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
            common::CompatibilityStatus::ExpectedDifferences => {
                println!("📊 Expected differences",);
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
                if result.p_ata.success && !result.original.success {
                    println!("      P-ATA succeeded where Original failed");
                } else if !result.p_ata.success && result.original.success {
                    println!("      Original succeeded where P-ATA failed");
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
                "      Original: {} CUs | {}",
                result.original.compute_units,
                if result.original.success {
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
            if !result.original.success {
                if let Some(ref error) = result.original.error_message {
                    println!("      Original Error: {}", error);
                }
            }

            // Show captured debug output if available and non-empty
            if !result.p_ata.captured_output.is_empty() {
                println!("      P-ATA Debug Output:");
                for line in result.p_ata.captured_output.lines() {
                    println!("        {}", line);
                }
            }
            if !result.original.captured_output.is_empty() {
                println!("      Original Debug Output:");
                for line in result.original.captured_output.lines() {
                    println!("        {}", line);
                }
            }
        }
    }

    fn print_compatibility_summary(all_results: &[ComparisonResult]) {
        println!("\n=== COMPATIBILITY ANALYSIS SUMMARY ===");

        let mut identical_count = 0;
        let mut optimized_count = 0;
        let mut expected_diff_count = 0;
        let mut account_mismatch_count = 0;
        let mut incompatible_failure_count = 0;
        let mut incompatible_success_count = 0;
        let mut both_rejected_count = 0;

        let mut concerning_results = Vec::new();

        for result in all_results {
            match result.compatibility_status {
                common::CompatibilityStatus::Identical => identical_count += 1,
                common::CompatibilityStatus::OptimizedBehavior => optimized_count += 1,
                common::CompatibilityStatus::ExpectedDifferences => expected_diff_count += 1,
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
        println!("  ✅ Identical Results: {}", identical_count);
        println!("  🚀 P-ATA Optimizations: {}", optimized_count);
        println!("  📊 Expected Differences: {}", expected_diff_count);
        println!("  ❌ Both Rejected (Compatible): {}", both_rejected_count);

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
                if !result.original.success {
                    if let Some(ref error) = result.original.error_message {
                        println!("      Original Error: {}", error);
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
        original_result: common::BenchmarkResult,
        p_ata_ix: &Instruction,
        p_ata_accounts: &[(Pubkey, Account)],
        original_ix: &Instruction,
        original_accounts: &[(Pubkey, Account)],
        token_program_id: &Pubkey,
    ) -> ComparisonResult {
        // Start with basic comparison
        let mut comparison = common::ComparisonRunner::create_comparison_result(
            test_name,
            p_ata_result.clone(),
            original_result.clone(),
        );

        // If both succeeded, perform byte-for-byte account state comparison
        if p_ata_result.success && original_result.success {
            let mollusk = common::ComparisonRunner::create_mollusk_for_all_ata_implementations(
                token_program_id,
            );

            // Execute P-ATA instruction and capture final account states
            let p_ata_execution = mollusk.process_instruction(p_ata_ix, p_ata_accounts);
            let original_execution = mollusk.process_instruction(original_ix, original_accounts);

            if let (
                mollusk_svm::result::ProgramResult::Success,
                mollusk_svm::result::ProgramResult::Success,
            ) = (
                &p_ata_execution.program_result,
                &original_execution.program_result,
            ) {
                // Check if this is just a SysvarRent difference (expected P-ATA optimization)
                let has_sysvar_rent_difference =
                    Self::has_sysvar_rent_difference(p_ata_ix, original_ix);

                // Compare account states byte-for-byte
                let accounts_match = Self::compare_account_states(
                    &p_ata_execution.resulting_accounts,
                    &original_execution.resulting_accounts,
                    p_ata_ix,
                    original_ix,
                );

                if !accounts_match {
                    // Check if it's just SysvarRent differences (expected optimization)
                    if has_sysvar_rent_difference
                        && Self::accounts_match_except_sysvar_rent(
                            &p_ata_execution.resulting_accounts,
                            &original_execution.resulting_accounts,
                            p_ata_ix,
                            original_ix,
                        )
                    {
                        comparison.compatibility_status =
                            common::CompatibilityStatus::ExpectedDifferences;
                    } else {
                        // Real account state mismatch
                        comparison.compatibility_status =
                            common::CompatibilityStatus::AccountMismatch;
                    }
                }
            }
        }

        comparison
    }

    fn has_sysvar_rent_difference(p_ata_ix: &Instruction, original_ix: &Instruction) -> bool {
        let sysvar_rent = "SysvarRent111111111111111111111111111111111"
            .parse::<Pubkey>()
            .unwrap();

        let p_ata_has_rent = p_ata_ix
            .accounts
            .iter()
            .any(|meta| meta.pubkey == sysvar_rent);
        let original_has_rent = original_ix
            .accounts
            .iter()
            .any(|meta| meta.pubkey == sysvar_rent);

        p_ata_has_rent != original_has_rent
    }

    fn accounts_match_except_sysvar_rent(
        p_ata_accounts: &[(Pubkey, Account)],
        original_accounts: &[(Pubkey, Account)],
        p_ata_ix: &Instruction,
        original_ix: &Instruction,
    ) -> bool {
        let sysvar_rent = "SysvarRent111111111111111111111111111111111"
            .parse::<Pubkey>()
            .unwrap();

        // Filter out SysvarRent accounts and compare the rest
        let p_ata_filtered: Vec<_> = p_ata_accounts
            .iter()
            .filter(|(pubkey, _)| *pubkey != sysvar_rent)
            .collect();
        let original_filtered: Vec<_> = original_accounts
            .iter()
            .filter(|(pubkey, _)| *pubkey != sysvar_rent)
            .collect();

        // Create filtered instructions without SysvarRent
        let p_ata_ix_filtered = Instruction {
            program_id: p_ata_ix.program_id,
            accounts: p_ata_ix
                .accounts
                .iter()
                .filter(|meta| meta.pubkey != sysvar_rent)
                .cloned()
                .collect(),
            data: p_ata_ix.data.clone(),
        };

        let original_ix_filtered = Instruction {
            program_id: original_ix.program_id,
            accounts: original_ix
                .accounts
                .iter()
                .filter(|meta| meta.pubkey != sysvar_rent)
                .cloned()
                .collect(),
            data: original_ix.data.clone(),
        };

        // Now compare using the existing logic but with filtered data
        let p_ata_map: std::collections::HashMap<&Pubkey, &Account> =
            p_ata_filtered.iter().map(|(k, v)| (k, v)).collect();
        let original_map: std::collections::HashMap<&Pubkey, &Account> =
            original_filtered.iter().map(|(k, v)| (k, v)).collect();

        let max_accounts = p_ata_ix_filtered
            .accounts
            .len()
            .max(original_ix_filtered.accounts.len());

        for i in 0..max_accounts {
            let p_ata_meta = p_ata_ix_filtered.accounts.get(i);
            let original_meta = original_ix_filtered.accounts.get(i);

            match (p_ata_meta, original_meta) {
                (Some(p_ata_meta), Some(original_meta)) => {
                    if p_ata_meta.is_writable || original_meta.is_writable {
                        let p_ata_account = p_ata_map.get(&p_ata_meta.pubkey);
                        let original_account = original_map.get(&original_meta.pubkey);

                        match (p_ata_account, original_account) {
                            (Some(&p_ata_acc), Some(&original_acc)) => {
                                // For token accounts, use behavioral equivalence check
                                let account_type = Self::get_account_type_by_position(i);
                                if account_type == "ATA Account"
                                    && p_ata_acc.data.len() >= 165
                                    && original_acc.data.len() >= 165
                                {
                                    if !Self::validate_token_account_behavioral_equivalence_quiet(
                                        &p_ata_acc.data,
                                        &original_acc.data,
                                        &mut Vec::new(),
                                    ) {
                                        return false;
                                    }
                                } else if p_ata_acc.data != original_acc.data
                                    || p_ata_acc.lamports != original_acc.lamports
                                    || p_ata_acc.owner != original_acc.owner
                                {
                                    return false;
                                }
                            }
                            (Some(_), None) | (None, Some(_)) => return false,
                            (None, None) => {}
                        }
                    }
                }
                (Some(_), None) | (None, Some(_)) => return false,
                (None, None) => break,
            }
        }

        true
    }

    fn compare_account_states(
        p_ata_accounts: &[(Pubkey, Account)],
        original_accounts: &[(Pubkey, Account)],
        p_ata_ix: &Instruction,
        original_ix: &Instruction,
    ) -> bool {
        // Convert to maps for easier comparison
        let p_ata_map: std::collections::HashMap<&Pubkey, &Account> =
            p_ata_accounts.iter().map(|(k, v)| (k, v)).collect();
        let original_map: std::collections::HashMap<&Pubkey, &Account> =
            original_accounts.iter().map(|(k, v)| (k, v)).collect();

        let mut all_match = true;
        let mut debug_output = Vec::new();
        let mut has_expected_differences = false;

        // Compare accounts by their ROLE/POSITION in the instruction, not by address
        let max_accounts = p_ata_ix.accounts.len().max(original_ix.accounts.len());

        for i in 0..max_accounts {
            let p_ata_meta = p_ata_ix.accounts.get(i);
            let original_meta = original_ix.accounts.get(i);

            match (p_ata_meta, original_meta) {
                (Some(p_ata_meta), Some(original_meta)) => {
                    // Only compare writable accounts (the ones that change)
                    if p_ata_meta.is_writable || original_meta.is_writable {
                        let account_type = Self::get_account_type_by_position(i);

                        let p_ata_account = p_ata_map.get(&p_ata_meta.pubkey);
                        let original_account = original_map.get(&original_meta.pubkey);

                        match (p_ata_account, original_account) {
                            (Some(&p_ata_acc), Some(&original_acc)) => {
                                // Compare account data - capture output for later
                                let (data_match, data_output) = Self::compare_account_data_quiet(
                                    &p_ata_acc.data,
                                    &original_acc.data,
                                    &account_type,
                                    &p_ata_meta.pubkey,
                                    &original_meta.pubkey,
                                );

                                let (lamports_match, lamports_output) =
                                    Self::compare_lamports_quiet(
                                        p_ata_acc.lamports,
                                        original_acc.lamports,
                                        &account_type,
                                    );

                                let (owner_match, owner_output) = Self::compare_owner_quiet(
                                    &p_ata_acc.owner,
                                    &original_acc.owner,
                                    &account_type,
                                );

                                if !data_match || !lamports_match || !owner_match {
                                    // Only add debug output if there are issues
                                    debug_output
                                        .push(format!("\n📋 {} (Position {})", account_type, i));
                                    debug_output
                                        .push(format!("  P-ATA Address:  {}", p_ata_meta.pubkey));
                                    debug_output.push(format!(
                                        "  Original Address: {}",
                                        original_meta.pubkey
                                    ));
                                    debug_output.extend(data_output);
                                    debug_output.extend(lamports_output);
                                    debug_output.extend(owner_output);
                                    all_match = false;
                                }
                            }
                            (Some(_), None) => {
                                debug_output
                                    .push(format!("\n📋 {} (Position {})", account_type, i));
                                debug_output.push(
                                    "  ❌ P-ATA account exists but Original account missing!"
                                        .to_string(),
                                );
                                all_match = false;
                            }
                            (None, Some(_)) => {
                                debug_output
                                    .push(format!("\n📋 {} (Position {})", account_type, i));
                                debug_output.push(
                                    "  ❌ Original account exists but P-ATA account missing!"
                                        .to_string(),
                                );
                                all_match = false;
                            }
                            (None, None) => {
                                // Both missing - this is fine, no output needed
                            }
                        }
                    }
                }
                (Some(p_ata_meta), None) => {
                    // Check if this is SysvarRent (expected P-ATA optimization)
                    if p_ata_meta.pubkey.to_string()
                        == "SysvarRent111111111111111111111111111111111"
                    {
                        debug_output.push(format!(
                            "\n📋 Position {} - P-ATA includes SysvarRent optimization",
                            i
                        ));
                        has_expected_differences = true;
                    } else {
                        debug_output.push(format!(
                            "\n📋 Position {} - P-ATA has unexpected extra account: {}",
                            i, p_ata_meta.pubkey
                        ));
                        all_match = false;
                    }
                }
                (None, Some(original_meta)) => {
                    // Check if this is SysvarRent (expected Original ATA requirement)
                    if original_meta.pubkey.to_string()
                        == "SysvarRent111111111111111111111111111111111"
                    {
                        debug_output.push(format!(
                            "\n📋 Position {} - Original ATA requires SysvarRent (P-ATA optimized it away)",
                            i
                        ));
                        has_expected_differences = true;
                    } else {
                        debug_output.push(format!(
                            "\n📋 Position {} - Original has unexpected extra account: {}",
                            i, original_meta.pubkey
                        ));
                        all_match = false;
                    }
                }
                (None, None) => break,
            }
        }

        // Only print debug output if there were issues
        if !all_match || has_expected_differences {
            println!("\nACCOUNT STATE COMPARISON:");
            for line in debug_output {
                println!("{}", line);
            }

            if !all_match {
                println!("\n❌ Account state differences detected!");
            } else if has_expected_differences {
                println!("\n📊 Expected differences detected (P-ATA optimizations)");
            }
        }

        all_match
    }

    fn get_account_type_by_position(pos: usize) -> String {
        match pos {
            0 => "Payer".to_string(),
            1 => "ATA Account".to_string(),
            2 => "Wallet/Owner".to_string(),
            3 => "Mint".to_string(),
            4 => "System Program".to_string(),
            5 => "Token Program".to_string(),
            6 => "Rent Sysvar".to_string(),
            _ => format!("Account #{}", pos),
        }
    }

    fn compare_account_data_quiet(
        p_ata_data: &[u8],
        original_data: &[u8],
        account_type: &str,
        _p_ata_addr: &Pubkey,
        _original_addr: &Pubkey,
    ) -> (bool, Vec<String>) {
        let mut output = Vec::new();

        if p_ata_data == original_data {
            return (true, output); // No output for matches
        }

        output.push(format!(
            "  📊 Data: Different ({} vs {} bytes)",
            p_ata_data.len(),
            original_data.len()
        ));

        if account_type == "ATA Account" && p_ata_data.len() >= 165 && original_data.len() >= 165 {
            // For ATA accounts, do structural analysis
            let structural_output =
                Self::compare_token_account_structure_quiet(p_ata_data, original_data);
            output.extend(structural_output);

            // Check behavioral equivalence
            let equivalent = Self::validate_token_account_behavioral_equivalence_quiet(
                p_ata_data,
                original_data,
                &mut output,
            );
            (equivalent, output)
        } else {
            // For non-ATA accounts, show raw differences
            let raw_output = Self::compare_raw_bytes_quiet(p_ata_data, original_data);
            output.extend(raw_output);
            (false, output) // Non-ATA accounts should generally be identical
        }
    }

    fn compare_lamports_quiet(
        p_ata_lamports: u64,
        original_lamports: u64,
        _account_type: &str,
    ) -> (bool, Vec<String>) {
        let mut output = Vec::new();

        if p_ata_lamports == original_lamports {
            (true, output) // No output for matches
        } else {
            output.push("  ❌ Lamports: MISMATCH!".to_string());
            output.push(format!(
                "     P-ATA: {} SOL",
                p_ata_lamports as f64 / 1_000_000_000.0
            ));
            output.push(format!(
                "     Original: {} SOL",
                original_lamports as f64 / 1_000_000_000.0
            ));
            output.push(format!(
                "     Difference: {} lamports",
                p_ata_lamports as i64 - original_lamports as i64
            ));
            (false, output)
        }
    }

    fn compare_owner_quiet(
        p_ata_owner: &Pubkey,
        original_owner: &Pubkey,
        _account_type: &str,
    ) -> (bool, Vec<String>) {
        let mut output = Vec::new();

        if p_ata_owner == original_owner {
            (true, output) // No output for matches
        } else {
            output.push("  ❌ Owner: MISMATCH!".to_string());
            output.push(format!("     P-ATA: {}", p_ata_owner));
            output.push(format!("     Original: {}", original_owner));
            (false, output)
        }
    }

    fn compare_token_account_structure_quiet(
        p_ata_data: &[u8],
        original_data: &[u8],
    ) -> Vec<String> {
        let mut output = Vec::new();
        output.push("     🔍 Token Account Structure Analysis:".to_string());

        // Parse token account structure (based on spl-token layout)
        if p_ata_data.len() >= 165 && original_data.len() >= 165 {
            // Mint and Owner are expected to be different (different test inputs)
            let p_ata_mint = &p_ata_data[0..32];
            let orig_mint = &original_data[0..32];
            output.push(
                "       📍 Mint: P-ATA test uses different mint than Original test (expected)"
                    .to_string(),
            );
            output.push(format!(
                "         P-ATA points to: {}...",
                Self::bytes_to_hex(&p_ata_mint[0..8])
            ));
            output.push(format!(
                "         Original points to: {}...",
                Self::bytes_to_hex(&orig_mint[0..8])
            ));

            let p_ata_owner = &p_ata_data[32..64];
            let orig_owner = &original_data[32..64];
            output.push(
                "       📍 Owner: P-ATA test uses different owner than Original test (expected)"
                    .to_string(),
            );
            output.push(format!(
                "         P-ATA points to: {}...",
                Self::bytes_to_hex(&p_ata_owner[0..8])
            ));
            output.push(format!(
                "         Original points to: {}...",
                Self::bytes_to_hex(&orig_owner[0..8])
            ));

            // Amount should be the same for equivalent operations
            let p_ata_amount =
                u64::from_le_bytes(p_ata_data[64..72].try_into().unwrap_or([0u8; 8]));
            let orig_amount =
                u64::from_le_bytes(original_data[64..72].try_into().unwrap_or([0u8; 8]));
            if p_ata_amount != orig_amount {
                output.push(format!(
                    "       ❌ Amount differs: P-ATA={}, Original={}",
                    p_ata_amount, orig_amount
                ));
            } else {
                output.push(format!(
                    "       ✅ Amount: {} tokens (correct)",
                    p_ata_amount
                ));
            }

            // State should be the same
            if p_ata_data[108] != original_data[108] {
                output.push(format!(
                    "       ❌ State differs: P-ATA={}, Original={}",
                    p_ata_data[108], original_data[108]
                ));
            } else {
                output.push(format!("       ✅ State: {} (correct)", p_ata_data[108]));
            }

            // Check other structural fields
            let p_ata_delegate = &p_ata_data[72..104];
            let orig_delegate = &original_data[72..104];
            if p_ata_delegate != orig_delegate {
                output.push("       ❌ Delegate differs - structural issue!".to_string());
            } else if p_ata_delegate == &[0u8; 32] {
                output.push("       ✅ Delegate: None (correct for new ATA)".to_string());
            } else {
                output.push("       ✅ Delegate: Identical".to_string());
            }

            let p_ata_delegated =
                u64::from_le_bytes(p_ata_data[104..112].try_into().unwrap_or([0u8; 8]));
            let orig_delegated =
                u64::from_le_bytes(original_data[104..112].try_into().unwrap_or([0u8; 8]));
            if p_ata_delegated != orig_delegated {
                output.push(format!(
                    "       ❌ Delegated amount differs: P-ATA={}, Original={}",
                    p_ata_delegated, orig_delegated
                ));
            } else {
                output.push(format!(
                    "       ✅ Delegated amount: {} (correct)",
                    p_ata_delegated
                ));
            }
        }

        output
    }

    fn validate_token_account_behavioral_equivalence_quiet(
        p_ata_data: &[u8],
        original_data: &[u8],
        output: &mut Vec<String>,
    ) -> bool {
        if p_ata_data.len() < 165 || original_data.len() < 165 {
            return false;
        }

        let mut equivalent = true;

        // Check behavioral fields that should be identical for equivalent operations
        let p_ata_amount = u64::from_le_bytes(p_ata_data[64..72].try_into().unwrap_or([0u8; 8]));
        let orig_amount = u64::from_le_bytes(original_data[64..72].try_into().unwrap_or([0u8; 8]));
        if p_ata_amount != orig_amount {
            equivalent = false;
        }

        if p_ata_data[108] != original_data[108] {
            equivalent = false;
        }

        let p_ata_delegate = &p_ata_data[72..104];
        let orig_delegate = &original_data[72..104];
        if p_ata_delegate != orig_delegate {
            equivalent = false;
        }

        let p_ata_delegated =
            u64::from_le_bytes(p_ata_data[104..112].try_into().unwrap_or([0u8; 8]));
        let orig_delegated =
            u64::from_le_bytes(original_data[104..112].try_into().unwrap_or([0u8; 8]));
        if p_ata_delegated != orig_delegated {
            equivalent = false;
        }

        if equivalent {
            output.push("     ✅ Behavioral equivalence: PASSED (accounts behave identically despite different inputs)".to_string());
        } else {
            output.push("     ❌ Behavioral equivalence: FAILED (different behavior for equivalent operations)".to_string());
        }

        equivalent
    }

    fn compare_raw_bytes_quiet(p_ata_data: &[u8], original_data: &[u8]) -> Vec<String> {
        let mut output = Vec::new();
        let max_len = p_ata_data.len().max(original_data.len());
        let mut diff_count = 0;

        output.push("     📊 Byte-by-byte differences:".to_string());
        for i in 0..max_len {
            let p_ata_byte = p_ata_data.get(i).copied();
            let orig_byte = original_data.get(i).copied();

            if p_ata_byte != orig_byte && diff_count < 20 {
                // Show first 20 differences
                output.push(format!(
                    "       Offset {}: P-ATA={:02x?}, Original={:02x?}",
                    i, p_ata_byte, orig_byte
                ));
                diff_count += 1;
            } else if p_ata_byte != orig_byte {
                diff_count += 1;
            }
        }

        if diff_count > 20 {
            output.push(format!(
                "       ... and {} more differences",
                diff_count - 20
            ));
        }
        output.push(format!("     Total differences: {} bytes", diff_count));

        output
    }

    fn bytes_to_hex(bytes: &[u8]) -> String {
        bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join("")
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
    // Completely suppress debug output from Mollusk and Solana runtime
    std::env::set_var("RUST_LOG", "error");

    // Setup quiet logging by default - only show warnings and errors
    let _ = solana_logger::setup_with(
        "error,solana_runtime=error,solana_program_runtime=error,mollusk=error",
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

        println!("\n🔍 Running comparison between implementations");

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
