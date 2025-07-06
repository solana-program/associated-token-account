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
}
