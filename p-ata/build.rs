fn main() {
    println!("cargo:rerun-if-changed=programs/token");
    println!("cargo:rerun-if-changed=programs/token-2022");

    #[cfg(feature = "build-programs")]
    builder::build_programs();
}

#[cfg(feature = "build-programs")]
mod builder {
    use serde_json;
    use solana_pubkey::Pubkey;
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    pub fn build_programs() {
        println!("cargo:warning=Building token programs for benchmarking...");

        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");

        // Update submodules
        update_submodules(&manifest_dir);

        // Build p-token program
        build_p_token(&manifest_dir, &Path::new(""));

        // Build token-2022 program
        build_token_2022(&manifest_dir, &Path::new(""));

        // Build original ATA program for comparison
        build_original_ata(&manifest_dir, &Path::new(""));

        // Build P-ATA program variants
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

    fn build_original_ata(manifest_dir: &str, _programs_dir: &Path) {
        println!("cargo:warning=Building original ATA program...");

        // The original ATA program is in the root program/ directory
        let original_ata_dir = Path::new(manifest_dir)
            .parent()
            .expect("Failed to get parent directory")
            .join("program");

        if !original_ata_dir.exists() {
            println!(
                "cargo:warning=Original ATA program directory not found at {:?}, skipping...",
                original_ata_dir
            );
            return;
        }

        let output = Command::new("cargo")
            .args(["build-sbf"])
            .current_dir(&original_ata_dir)
            .output()
            .expect("Failed to execute cargo build-sbf for original ATA");

        if !output.status.success() {
            panic!(
                "Original ATA build failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        println!("cargo:warning=Original ATA built successfully to ../target/deploy/");
    }

    fn build_p_ata_variants(manifest_dir: &str) {
        println!("cargo:warning=Building P-ATA variants...");

        // Build prefunded variant first
        build_p_ata_prefunded(manifest_dir);

        // Build standard variant second
        build_p_ata_standard(manifest_dir);
    }

    fn build_p_ata_prefunded(manifest_dir: &str) {
        println!("cargo:warning=Building P-ATA prefunded variant...");

        // Build with create-account-prefunded feature
        let output = Command::new("cargo")
            .args(["build-sbf", "--features", "create-account-prefunded"])
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
                if keypair_bytes.len() >= 32 {
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

    fn build_p_ata_standard(manifest_dir: &str) {
        println!("cargo:warning=Building P-ATA standard variant...");

        // Build standard variant (without create-account-prefunded feature)
        let output = Command::new("cargo")
            .args(["build-sbf"])
            .current_dir(manifest_dir)
            .output()
            .expect("Failed to execute cargo build-sbf for P-ATA standard");

        if !output.status.success() {
            panic!(
                "P-ATA standard build failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // Read and print the program ID for debugging
        let deploy_dir = Path::new(manifest_dir).join("target/deploy");
        let keypair_path = deploy_dir.join("pinocchio_ata_program-keypair.json");

        if keypair_path.exists() {
            if let Ok(keypair_content) = fs::read_to_string(&keypair_path) {
                if let Ok(keypair_json) = serde_json::from_str::<Vec<u8>>(&keypair_content) {
                    if keypair_json.len() >= 64 {
                        let pubkey_bytes = &keypair_json[32..64];
                        let pubkey = Pubkey::new_from_array(pubkey_bytes.try_into().unwrap());
                        println!(
                            "cargo:warning=Built P-ATA standard with program ID: {}",
                            pubkey
                        );
                    }
                }
            }
        }

        println!("cargo:warning=P-ATA standard built successfully to target/deploy/");
    }
}
