use std::collections::HashMap;

// Reuse shared types from the existing benchmark framework
use crate::common::{BaseTestType, ComparisonResult, CompatibilityStatus, TestVariant};

/// Returns the variant that represents "all optimizations enabled" for a given base test.
pub fn get_all_optimizations_variant(base_test: BaseTestType) -> Option<TestVariant> {
    match base_test {
        BaseTestType::Create | BaseTestType::CreateTopup | BaseTestType::CreateTopupNoCap => {
            Some(TestVariant::RENT_BUMP)
        }
        BaseTestType::CreateIdempotent => Some(TestVariant::BASE),
        BaseTestType::CreateToken2022 => Some(TestVariant::RENT_BUMP_LEN),
        BaseTestType::RecoverNested | BaseTestType::RecoverMultisig => Some(TestVariant::BUMP),
        _ => None,
    }
}

/// Nicely print the CU matrix for all test results.
pub fn print_matrix_results(
    matrix_results: &HashMap<BaseTestType, HashMap<TestVariant, ComparisonResult>>,
    display_variants: &[TestVariant],
) {
    println!("\n=== PERFORMANCE MATRIX RESULTS ===");

    // Build the column set: SPL-ATA (base), each requested P-ATA variant, plus an "all opt" variant
    let all_opt_variant = TestVariant {
        rent_arg: true,
        bump_arg: true,
        len_arg: true,
    };
    let mut columns = vec![TestVariant::BASE];
    columns.extend_from_slice(display_variants);
    columns.push(all_opt_variant);

    // Header
    print!("{:<20}", "Test");
    for (i, v) in columns.iter().enumerate() {
        let name = if i == 0 { "SPL ATA" } else { v.column_name() };
        print!(" | {:>15}", name);
    }
    println!();

    // Separator
    print!("{:-<20}", "");
    for _ in &columns {
        print!("-+-{:-<15}", "");
    }
    println!();

    // Rows
    for (base_test, row) in matrix_results {
        print!("{:<20}", base_test.name());
        for (i, variant) in columns.iter().enumerate() {
            let lookup = if *variant == all_opt_variant {
                get_all_optimizations_variant(*base_test)
            } else {
                Some(*variant)
            };

            if let Some(actual) = lookup {
                if let Some(result) = row.get(&actual) {
                    let cu = if i == 0 {
                        if result.spl_ata.success && result.spl_ata.compute_units > 0 {
                            result.spl_ata.compute_units
                        } else {
                            0
                        }
                    } else {
                        if result.p_ata.success && result.p_ata.compute_units > 0 {
                            result.p_ata.compute_units
                        } else {
                            0
                        }
                    };
                    if cu > 0 {
                        print!(" | {:>15}", cu);
                    } else {
                        print!(" | {:>15}", "");
                    }
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

/// Print detailed per-test comparison output.
pub fn print_test_results(result: &ComparisonResult, show_debug: bool) {
    use crate::common;

    print!("--- Testing {} --- ", result.test_name);

    let needs_details = matches!(
        result.compatibility_status,
        common::CompatibilityStatus::AccountMismatch
            | common::CompatibilityStatus::IncompatibleSuccess
            | common::CompatibilityStatus::IncompatibleFailure
    );

    match result.compatibility_status {
        common::CompatibilityStatus::Identical => {
            println!("✅ Byte-for-Byte Identical");
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

    if needs_details || show_debug {
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

        if !result.p_ata.success {
            if let Some(ref err) = result.p_ata.error_message {
                println!("      P-ATA Error: {}", err);
            }
        }
        if !result.spl_ata.success {
            if let Some(ref err) = result.spl_ata.error_message {
                println!("      SPL ATA Error: {}", err);
            }
        }

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

/// Summarise overall compatibility findings across all tests.
pub fn print_compatibility_summary(all_results: &[ComparisonResult]) {
    use crate::common;

    println!("\n=== COMPATIBILITY ANALYSIS SUMMARY ===");

    let mut identical = 0;
    let mut optimized = 0;
    let mut account_mismatch = 0;
    let mut incompatible_failure = 0;
    let mut incompatible_success = 0;
    let mut both_rejected = 0;

    let mut concerning = Vec::new();

    for r in all_results {
        match r.compatibility_status {
            common::CompatibilityStatus::Identical => identical += 1,
            common::CompatibilityStatus::OptimizedBehavior => optimized += 1,
            common::CompatibilityStatus::BothRejected => both_rejected += 1,
            common::CompatibilityStatus::AccountMismatch => {
                account_mismatch += 1;
                concerning.push(r);
            }
            common::CompatibilityStatus::IncompatibleFailure => {
                incompatible_failure += 1;
                concerning.push(r);
            }
            common::CompatibilityStatus::IncompatibleSuccess => {
                incompatible_success += 1;
                concerning.push(r);
            }
        }
    }

    println!("Total Tests: {}", all_results.len());
    println!(
        "  ✅ P-ATA Passed Byte-for-Byte Identical with SPL ATA: {}",
        identical
    );
    println!(
        "  🚀 P-ATA Optimizations Passed (not relevant for SPL ATA): {}",
        optimized
    );
    println!("  ❌ Both Rejected Unexpectedly: {}", both_rejected);

    if !concerning.is_empty() {
        println!("\n⚠️  CONCERNING COMPATIBILITY ISSUES:");
        if account_mismatch > 0 {
            println!("  🔴 Account State Mismatches: {}", account_mismatch);
        }
        if incompatible_failure > 0 {
            println!("  🔴 Incompatible Failure Modes: {}", incompatible_failure);
        }
        if incompatible_success > 0 {
            println!(
                "  🔴 Incompatible Success/Failure: {}",
                incompatible_success
            );
        }

        println!("\n  Detailed Issues:");
        for r in &concerning {
            println!("    {} - {:?}", r.test_name, r.compatibility_status);
            if !r.p_ata.success {
                if let Some(ref e) = r.p_ata.error_message {
                    println!("      P-ATA Error: {}", e);
                }
            }
            if !r.spl_ata.success {
                if let Some(ref e) = r.spl_ata.error_message {
                    println!("      SPL ATA Error: {}", e);
                }
            }
        }
    } else {
        println!("\n✅ All tests show compatible behavior!");
    }
}

/// Emit machine-readable JSON of the performance matrix so that other tools (eg. CI dashboards)
/// can consume the results.
pub fn output_matrix_data(
    matrix_results: &HashMap<BaseTestType, HashMap<TestVariant, ComparisonResult>>,
    display_variants: &[TestVariant],
) {
    let mut json_tests = HashMap::new();

    // Column list: requested variants + all-optimisations variant
    let all_opt_variant = TestVariant {
        rent_arg: true,
        bump_arg: true,
        len_arg: true,
    };
    let mut columns = display_variants.to_vec();
    columns.push(all_opt_variant);

    for (base_test, row) in matrix_results {
        let mut variant_map = HashMap::new();
        for variant in &columns {
            if let Some(result) = row.get(variant) {
                if result.p_ata.success && result.p_ata.compute_units > 0 {
                    let spl_ata_cu = if result.spl_ata.success {
                        result.spl_ata.compute_units
                    } else {
                        0
                    };

                    let compatibility = match result.compatibility_status {
                        CompatibilityStatus::Identical => "identical",
                        CompatibilityStatus::OptimizedBehavior => "optimized",
                        _ => "other",
                    };

                    let spl_ata_cu_str = if spl_ata_cu > 0 {
                        spl_ata_cu.to_string()
                    } else {
                        "null".to_string()
                    };

                    variant_map.insert(
                        variant.column_name().replace(" ", "_"),
                        format!(
                            r#"{{"p_ata_cu": {}, "spl_ata_cu": {}, "compatibility": "{}", "type": "performance_test"}}"#,
                            result.p_ata.compute_units,
                            spl_ata_cu_str,
                            compatibility
                        ),
                    );
                }
            }
        }
        if !variant_map.is_empty() {
            json_tests.insert(base_test.name(), variant_map);
        }
    }

    // Build JSON string manually (avoid pulling in serde just for this)
    let mut json_entries = Vec::new();
    for (test_name, variants) in json_tests {
        let mut variant_entries = Vec::new();
        for (variant_name, data) in variants {
            variant_entries.push(format!("    \"{}\": {}", variant_name, data));
        }
        json_entries.push(format!(
            r#"  \"{}\": {{
{}
  }}"#,
            test_name,
            variant_entries.join(",\n")
        ));
    }

    let output = format!(
        r#"{{
  \"timestamp\": {},
  \"performance_tests\": {{
{}
  }}
}}"#,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        json_entries.join(",\n")
    );

    std::fs::create_dir_all("benchmark_results").ok();
    if let Err(e) = std::fs::write("benchmark_results/performance_results.json", output) {
        eprintln!("Failed to write performance results: {}", e);
    } else {
        println!("\n📊 Matrix results written to benchmark_results/performance_results.json");
    }
}
