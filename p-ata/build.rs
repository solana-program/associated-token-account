fn main() {
    println!("cargo:rerun-if-changed=programs/token");
    println!("cargo:rerun-if-changed=programs/token-2022");

    #[cfg(feature = "build-programs")]
    builder::build_programs();
}

#[cfg(feature = "build-programs")]
mod builder {
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

        // Build standard P-ATA (without create-account-prefunded feature)
        build_p_ata_standard(manifest_dir);

        // Build prefunded P-ATA (with create-account-prefunded feature)
        build_p_ata_prefunded(manifest_dir);
    }

    fn build_p_ata_standard(manifest_dir: &str) {
        println!("cargo:warning=Building P-ATA standard variant...");

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

        println!("cargo:warning=P-ATA standard built successfully to target/deploy/");
    }

    fn build_p_ata_prefunded(manifest_dir: &str) {
        println!("cargo:warning=Building P-ATA prefunded variant...");

        let deploy_dir = Path::new(manifest_dir).join("target/deploy");
        let standard_so = deploy_dir.join("pinocchio_ata_program.so");
        let standard_keypair = deploy_dir.join("pinocchio_ata_program-keypair.json");
        let backup_so = deploy_dir.join("pinocchio_ata_program_standard_backup.so");
        let backup_keypair = deploy_dir.join("pinocchio_ata_program_standard_backup-keypair.json");

        // Backup the standard variant files
        if standard_so.exists() {
            if let Err(e) = fs::copy(&standard_so, &backup_so) {
                println!("cargo:warning=Failed to backup standard .so file: {}", e);
                return;
            }
        }

        if standard_keypair.exists() {
            if let Err(e) = fs::copy(&standard_keypair, &backup_keypair) {
                println!(
                    "cargo:warning=Failed to backup standard keypair file: {}",
                    e
                );
                return;
            }
        }

        // Build prefunded variant
        let output = Command::new("cargo")
            .args(["build-sbf", "--features", "create-account-prefunded"])
            .current_dir(manifest_dir)
            .output()
            .expect("Failed to execute cargo build-sbf for P-ATA prefunded");

        if !output.status.success() {
            // If prefunded build fails, warn but don't panic - restore standard files
            println!(
                "cargo:warning=P-ATA prefunded build failed (this is okay if the feature is not available): {}",
                String::from_utf8_lossy(&output.stderr)
            );

            // Restore standard files
            if backup_so.exists() {
                let _ = fs::copy(&backup_so, &standard_so);
                let _ = fs::remove_file(&backup_so);
            }
            if backup_keypair.exists() {
                let _ = fs::copy(&backup_keypair, &standard_keypair);
                let _ = fs::remove_file(&backup_keypair);
            }
            return;
        }

        // Copy the prefunded build to prefunded names
        let prefunded_so = deploy_dir.join("pinocchio_ata_program_prefunded.so");
        let prefunded_keypair = deploy_dir.join("pinocchio_ata_program_prefunded-keypair.json");

        if standard_so.exists() {
            if let Err(e) = fs::copy(&standard_so, &prefunded_so) {
                println!("cargo:warning=Failed to copy prefunded .so file: {}", e);
            }
        }

        if standard_keypair.exists() {
            if let Err(e) = fs::copy(&standard_keypair, &prefunded_keypair) {
                println!("cargo:warning=Failed to copy prefunded keypair file: {}", e);
            }
        }

        // Restore the standard variant files
        if backup_so.exists() {
            if let Err(e) = fs::copy(&backup_so, &standard_so) {
                println!("cargo:warning=Failed to restore standard .so file: {}", e);
            }
            let _ = fs::remove_file(&backup_so);
        }

        if backup_keypair.exists() {
            if let Err(e) = fs::copy(&backup_keypair, &standard_keypair) {
                println!(
                    "cargo:warning=Failed to restore standard keypair file: {}",
                    e
                );
            }
            let _ = fs::remove_file(&backup_keypair);
        }

        println!("cargo:warning=P-ATA prefunded built successfully to target/deploy/");
        println!("cargo:warning=Standard P-ATA files restored");
    }
}
