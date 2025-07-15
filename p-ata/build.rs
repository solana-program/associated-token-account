use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("generated_tests.rs");

    // List of test files to process
    let test_files = [
        "create_idempotent.rs",
        "extended_mint.rs",
        "process_create_associated_token_account.rs",
        "recover_nested.rs",
        "spl_token_create.rs",
    ];

    let mut generated_content = String::new();

    for test_file in &test_files {
        let original_path = format!("../program/tests/{}", test_file);

        // Read the original test file
        if let Ok(content) = fs::read_to_string(&original_path) {
            // Extract module name from filename
            let module_name = test_file.strip_suffix(".rs").unwrap();

            // Generate a wrapper module that provides the program_test module
            // and includes the original test content with modified imports
            let modified_content = modify_test_imports(&content);

            generated_content.push_str(&format!(
                r#"
pub mod {} {{
    // Provide the program_test module that the original test expects
    pub mod program_test {{
        pub use crate::tests::mollusk_adapter::{{
            mollusk_program_test as program_test,
            mollusk_program_test_2022 as program_test_2022,
            BanksClient, ProgramTestContext,
        }};
    }}
    
    // Import additional items needed by the tests
    use std::vec;
    use alloc::vec::Vec;
    
    // Re-export mollusk types at the module level to override solana_program_test imports
    pub use crate::tests::mollusk_adapter::{{BanksClient, ProgramTestContext}};
    
    // Modified original test content
{}
}}
"#,
                module_name, modified_content
            ));
        }
    }

    // Add fixtures module
    generated_content.push_str(
        r#"
pub mod fixtures {
    pub const TOKEN_MINT_DATA_BIN: &str = "../program/tests/fixtures/token-mint-data.bin";
}
"#,
    );

    fs::write(&dest_path, generated_content).unwrap();

    // Tell Cargo to rerun this build script if the original test files change
    for test_file in &test_files {
        println!("cargo:rerun-if-changed=../program/tests/{}", test_file);
    }
}

fn modify_test_imports(content: &str) -> String {
    // Remove the "mod program_test;" line since we provide it in the wrapper
    // Also replace problematic import patterns
    content
        .lines()
        .filter(|line| !line.trim().starts_with("mod program_test;"))
        .map(|line| {
            // Replace solana_program_test::* imports to avoid conflicts
            if line.trim().starts_with("use solana_program_test::*;") {
                "    // solana_program_test::* import replaced by local mollusk types"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
