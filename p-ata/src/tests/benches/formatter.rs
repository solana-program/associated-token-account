#![cfg(any(test, feature = "std"))]
#![cfg_attr(feature = "std", allow(dead_code, unused_imports))]

use {
    crate::tests::benches::common::{
        BaseTestType, ComparisonResult, CompatibilityStatus, TestVariant,
    },
    comfy_table::{presets::UTF8_FULL, ContentArrangement, Table},
    serde::Serialize,
    std::{
        collections::HashMap,
        eprintln, print, println,
        string::{String, ToString},
        vec,
        vec::Vec,
    },
};

/// Returns the variant that represents "all optimizations enabled" for a given base test.
pub fn get_all_optimizations_variant(base_test: BaseTestType) -> Option<TestVariant> {
    match base_test {
        BaseTestType::Create | BaseTestType::CreateTopup | BaseTestType::CreateTopupNoCap => {
            Some(TestVariant::RENT_BUMP)
        }
        BaseTestType::CreateIdempotent => Some(TestVariant::BUMP),
        BaseTestType::CreateToken2022 | BaseTestType::CreateExtended => {
            Some(TestVariant::RENT_BUMP_LEN)
        }
        BaseTestType::RecoverNested | BaseTestType::RecoverMultisig => None,
        _ => None,
    }
}

/// Nicely print the CU matrix for all test results.
pub fn print_matrix_results(
    matrix_results: &HashMap<BaseTestType, HashMap<TestVariant, ComparisonResult>>,
    display_variants: &[TestVariant],
) {
    let mut columns = vec![TestVariant::BASE];
    columns.extend_from_slice(display_variants);
    columns.push(TestVariant {
        rent_arg: true,
        bump_arg: true,
        token_account_len_arg: true,
    });

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);

    table.set_header(
        std::iter::once("Test".to_string())
            .chain(columns.iter().enumerate().map(|(i, v)| {
                if i == 0 {
                    "SPL ATA".to_string()
                } else {
                    v.column_name().to_string()
                }
            }))
            .collect::<Vec<String>>(),
    );

    for (base_test, row) in matrix_results {
        let mut cells = Vec::with_capacity(columns.len() + 1);
        cells.push(base_test.to_string());
        for (i, variant) in columns.iter().enumerate() {
            let lookup = if *variant
                == (TestVariant {
                    rent_arg: true,
                    bump_arg: true,
                    token_account_len_arg: true,
                }) {
                get_all_optimizations_variant(*base_test)
            } else {
                Some(*variant)
            };
            let cu = lookup.and_then(|actual| row.get(&actual)).map(|result| {
                if i == 0 {
                    if result.spl_ata.success && result.spl_ata.compute_units > 0 {
                        result.spl_ata.compute_units
                    } else {
                        0
                    }
                } else if result.p_ata.success && result.p_ata.compute_units > 0 {
                    result.p_ata.compute_units
                } else {
                    0
                }
            });
            cells.push(cu.map(|v| v.to_string()).unwrap_or_default());
        }
        table.add_row(cells);
    }

    println!("\n=== PERFORMANCE MATRIX RESULTS ===");
    println!("{}", table);
}

/// Print the compatibility status message for a test result
fn print_compatibility_status(result: &ComparisonResult) {
    use super::common;
    match result.compatibility_status {
        common::CompatibilityStatus::Identical => println!("✅ Byte-for-Byte Identical"),
        common::CompatibilityStatus::BothRejected => println!("❌ Both failed (compatible)"),
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
        common::CompatibilityStatus::OptimizedBehavior => println!("🚀 P-ATA optimization working"),
    }
}

/// Print detailed per-test comparison output.
pub fn print_test_results(result: &ComparisonResult, show_debug: bool) {
    use super::common;

    print!("--- Testing {} --- ", result.test_name);

    let needs_details = matches!(
        result.compatibility_status,
        common::CompatibilityStatus::AccountMismatch
            | common::CompatibilityStatus::IncompatibleSuccess
            | common::CompatibilityStatus::IncompatibleFailure
    );

    print_compatibility_status(result);

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

        for (label, result_data) in [("P-ATA", &result.p_ata), ("SPL ATA", &result.spl_ata)] {
            if !result_data.success {
                if let Some(ref err) = result_data.error_message {
                    println!("      {} Error: {}", label, err);
                }
            }
            if !result_data.captured_output.is_empty() {
                println!("      {} Debug Output:", label);
                for line in result_data.captured_output.lines() {
                    println!("        {}", line);
                }
            }
        }
    }
}

/// Summarise overall compatibility findings across all tests.
pub fn print_compatibility_summary(all_results: &[ComparisonResult]) {
    use super::common;

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
            for (label, result_data) in [("P-ATA", &r.p_ata), ("SPL ATA", &r.spl_ata)] {
                if !result_data.success {
                    if let Some(ref e) = result_data.error_message {
                        println!("      {} Error: {}", label, e);
                    }
                }
            }
        }
    } else {
        println!("\n✅ All tests show compatible behavior!");
    }
}

/// Emit machine-readable JSON of the performance matrix using serde_json.
#[derive(Serialize)]
struct VariantData {
    p_ata_cu: Option<u64>,
    spl_ata_cu: Option<u64>,
    compatibility: String,
    #[serde(rename = "type")]
    record_type: &'static str,
}

#[derive(Serialize)]
struct Output {
    timestamp: u64,
    performance_tests:
        std::collections::HashMap<String, std::collections::HashMap<String, VariantData>>,
}

pub fn output_matrix_data(
    matrix_results: &HashMap<BaseTestType, HashMap<TestVariant, ComparisonResult>>,
    display_variants: &[TestVariant],
) {
    use std::collections::HashMap;

    let all_opt_variant = TestVariant {
        rent_arg: true,
        bump_arg: true,
        token_account_len_arg: true,
    };

    let mut columns = display_variants.to_vec();
    columns.push(all_opt_variant);

    let mut performance_tests: HashMap<String, HashMap<String, VariantData>> = HashMap::new();

    for (base_test, row) in matrix_results {
        let mut per_variant: HashMap<String, VariantData> = HashMap::new();
        for variant in &columns {
            if let Some(res) = row.get(variant) {
                if res.p_ata.success && res.p_ata.compute_units > 0 {
                    let spl_cu = if res.spl_ata.success {
                        Some(res.spl_ata.compute_units)
                    } else {
                        None
                    };
                    let compatibility = match res.compatibility_status {
                        CompatibilityStatus::Identical => "identical",
                        CompatibilityStatus::OptimizedBehavior => "optimized",
                        _ => "other",
                    }
                    .to_string();

                    per_variant.insert(
                        variant.column_name().replace(' ', "_"),
                        VariantData {
                            p_ata_cu: Some(res.p_ata.compute_units),
                            spl_ata_cu: spl_cu,
                            compatibility,
                            record_type: "performance_test",
                        },
                    );
                }
            }
        }
        if !per_variant.is_empty() {
            performance_tests.insert(base_test.to_string(), per_variant);
        }
    }

    let output = Output {
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        performance_tests,
    };

    if let Ok(json) = serde_json::to_string_pretty(&output) {
        std::fs::create_dir_all("benchmark_results").ok();
        if std::fs::write("benchmark_results/performance_results.json", &json).is_ok() {
            println!("\n📊 Matrix results written to benchmark_results/performance_results.json");
        } else {
            eprintln!("Failed to write performance results");
        }
    } else {
        eprintln!("Failed to serialise performance results");
    }
}
