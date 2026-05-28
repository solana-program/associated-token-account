//! This build script is used to generate `generated_tests.rs` use the SPL ATA
//! tests and the mollusk adapter in src/tests/utils/mollusk_adapter.rs.
//!
//! If feature `build-programs` is enabled, it also updates submodules and builds
//! module and submodule programs.

#[cfg(feature = "build-programs")]
use std::{fs, path::Path, process::Command};

#[cfg(feature = "build-programs")]
use solana_pubkey::Pubkey;

fn main() {
    println!("cargo:rerun-if-changed=programs/token");
    println!("cargo:rerun-if-changed=programs/token-2022");

    #[cfg(feature = "build-programs")]
    builder::build_programs();
}

#[cfg(feature = "build-programs")]
mod builder {
    use super::*;

    pub fn build_programs() {
        println!("cargo:warning=Building token programs for benchmarking...");

        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");

        update_submodules(&manifest_dir);
        build_p_token(&manifest_dir, Path::new(""));
        build_token_2022(&manifest_dir, Path::new(""));
        build_spl_ata(&manifest_dir, Path::new(""));
        build_p_ata_variants(&manifest_dir);

        println!("cargo:warning=Token programs built successfully!");
    }

    fn update_submodules(manifest_dir: &str) {
        println!("cargo:warning=Updating git submodules...");

        let output = Command::new("git")
            .args(["submodule", "update", "--init", "--recursive"])
            .current_dir(manifest_dir)
            .output()
            .expect("Failed to execute git submodule update");

        if !output.status.success() {
            panic!(
                "Git submodule update failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    fn build_p_token(manifest_dir: &str, _programs_dir: &Path) {
        println!("cargo:warning=Building p-token program...");

        let p_token_dir = Path::new(manifest_dir).join("programs/token/p-token");
        let output = Command::new("cargo")
            .args(["build-sbf"])
            .current_dir(&p_token_dir)
            .output()
            .expect("Failed to execute cargo build-sbf for p-token");

        if !output.status.success() {
            panic!(
                "p-token build failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        println!("cargo:warning=P-token built successfully to programs/token/target/deploy/");
    }

    fn build_token_2022(manifest_dir: &str, _programs_dir: &Path) {
        println!("cargo:warning=Building token-2022 program...");

        let token_2022_dir = Path::new(manifest_dir).join("programs/token-2022/program");

        let output = Command::new("cargo")
            .args(["build-sbf"])
            .current_dir(&token_2022_dir)
            .output()
            .expect("Failed to execute cargo build-sbf for token-2022");

        if !output.status.success() {
            panic!(
                "token-2022 build failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        println!(
            "cargo:warning=Token-2022 built successfully to programs/token-2022/target/deploy/"
        );
    }

    fn build_spl_ata(manifest_dir: &str, _programs_dir: &Path) {
        println!("cargo:warning=Building SPL ATA program...");

        let spl_ata_dir = Path::new(manifest_dir)
            .parent()
            .expect("Failed to get parent directory")
            .join("program");

        if !spl_ata_dir.exists() {
            println!(
                "cargo:warning=SPL ATA program directory not found at {:?}, skipping...",
                spl_ata_dir
            );
            return;
        }

        let output = Command::new("cargo")
            .args(["build-sbf"])
            .current_dir(&spl_ata_dir)
            .output()
            .expect("Failed to execute cargo build-sbf for SPL ATA");

        if !output.status.success() {
            panic!(
                "SPL ATA build failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        println!("cargo:warning=SPL ATA built successfully to ../target/deploy/");
    }

    fn build_p_ata_variants(manifest_dir: &str) {
        println!("cargo:warning=Building P-ATA variants...");

        build_p_ata_prefunded(manifest_dir);
        build_p_ata_legacy(manifest_dir);
    }

    fn build_p_ata_prefunded(manifest_dir: &str) {
        println!("cargo:warning=Building P-ATA prefunded variant...");

        let output = Command::new("cargo")
            .args([
                "build-sbf",
                "--features",
                "create-prefunded-account",
                "--no-default-features",
            ])
            .current_dir(manifest_dir)
            .output()
            .expect("Failed to execute cargo build-sbf for P-ATA prefunded");

        if !output.status.success() {
            panic!(
                "P-ATA prefunded build failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // Read and print the program ID for debugging
        let deploy_dir = Path::new(manifest_dir).join("target/deploy");
        let keypair_path = deploy_dir.join("pinocchio_ata_program-keypair.json");

        if let Ok(keypair_data) = std::fs::read_to_string(&keypair_path) {
            if let Ok(keypair_bytes) = serde_json::from_str::<Vec<u8>>(&keypair_data) {
                if keypair_bytes.len() >= 64 {
                    let pubkey_bytes: [u8; 32] = keypair_bytes[32..64].try_into().unwrap();
                    let program_id = Pubkey::from(pubkey_bytes);
                    println!(
                        "cargo:warning=Built P-ATA prefunded with program ID: {}",
                        program_id
                    );
                }
            }
        }

        // Rename the results to prefunded names
        let default_so = deploy_dir.join("pinocchio_ata_program.so");
        let default_keypair = deploy_dir.join("pinocchio_ata_program-keypair.json");
        let prefunded_so = deploy_dir.join("pinocchio_ata_program_prefunded.so");
        let prefunded_keypair = deploy_dir.join("pinocchio_ata_program_prefunded-keypair.json");

        if let Err(e) = fs::rename(&default_so, &prefunded_so) {
            panic!("Failed to rename prefunded .so file: {}", e);
        }
        if let Err(e) = fs::rename(&default_keypair, &prefunded_keypair) {
            panic!("Failed to rename prefunded keypair file: {}", e);
        }

        // Read and print the prefunded program ID for debugging
        if let Ok(keypair_content) = fs::read_to_string(&prefunded_keypair) {
            if let Ok(keypair_json) = serde_json::from_str::<Vec<u8>>(&keypair_content) {
                if keypair_json.len() >= 64 {
                    let pubkey_bytes = &keypair_json[32..64];
                    let pubkey = Pubkey::new_from_array(pubkey_bytes.try_into().unwrap());
                    println!(
                        "cargo:warning=Built P-ATA prefunded with program ID: {}",
                        pubkey
                    );
                }
            }
        }

        println!("cargo:warning=P-ATA prefunded built and renamed successfully");
    }

    fn build_p_ata_legacy(manifest_dir: &str) {
        println!("cargo:warning=Building P-ATA legacy variant...");

        let output = Command::new("cargo")
            .args(["build-sbf", "--no-default-features"])
            .current_dir(manifest_dir)
            .output()
            .expect("Failed to execute cargo build-sbf for P-ATA legacy");

        if !output.status.success() {
            panic!(
                "P-ATA legacy build failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let deploy_dir = Path::new(manifest_dir).join("target/deploy");
        let keypair_path = deploy_dir.join("pinocchio_ata_program-keypair.json");

        if keypair_path.exists() {
            if let Ok(keypair_content) = fs::read_to_string(&keypair_path) {
                if let Ok(keypair_json) = serde_json::from_str::<Vec<u8>>(&keypair_content) {
                    if keypair_json.len() >= 64 {
                        let pubkey_bytes = &keypair_json[32..64];
                        let pubkey = Pubkey::new_from_array(pubkey_bytes.try_into().unwrap());
                        println!(
                            "cargo:warning=Built P-ATA legacy with program ID: {}",
                            pubkey
                        );
                    }
                }
            }
        }

        println!("cargo:warning=P-ATA legacy built successfully to target/deploy/");
    }
}
