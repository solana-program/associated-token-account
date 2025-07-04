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
        let programs_dir = Path::new(&manifest_dir).join("programs");

        // Ensure programs directory exists
        fs::create_dir_all(&programs_dir).expect("Failed to create programs directory");

        // Update submodules
        update_submodules(&manifest_dir);

        // Build p-token program
        build_p_token(&manifest_dir, &programs_dir);

        // Build token-2022 program
        build_token_2022(&manifest_dir, &programs_dir);

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

    fn build_p_token(manifest_dir: &str, programs_dir: &Path) {
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

        // Copy the binary to programs directory (build creates target in the parent workspace directory)
        let token_workspace_dir = Path::new(manifest_dir).join("programs/token");
        let source_so = token_workspace_dir
            .join("target/sbpf-solana-solana/release/pinocchio_token_program.so");
        let source_keypair =
            token_workspace_dir.join("target/deploy/pinocchio_token_program-keypair.json");
        let dest_so = programs_dir.join("pinocchio_token_program.so");
        let dest_keypair = programs_dir.join("pinocchio_token_program-keypair.json");

        if source_so.exists() {
            fs::copy(&source_so, &dest_so).expect("Failed to copy pinocchio_token_program.so");
            println!("cargo:warning=Copied pinocchio_token_program.so to programs/");
        } else {
            panic!(
                "pinocchio_token_program.so not found after build at: {:?}",
                source_so
            );
        }

        if source_keypair.exists() {
            fs::copy(&source_keypair, &dest_keypair)
                .expect("Failed to copy pinocchio_token_program-keypair.json");
        }
    }

    fn build_token_2022(manifest_dir: &str, programs_dir: &Path) {
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

        // Copy the binary to programs directory (build creates target in the parent workspace directory)
        let token_2022_workspace_dir = Path::new(manifest_dir).join("programs/token-2022");
        let source_so =
            token_2022_workspace_dir.join("target/sbpf-solana-solana/release/spl_token_2022.so");
        let source_keypair =
            token_2022_workspace_dir.join("target/deploy/spl_token_2022-keypair.json");
        let dest_so = programs_dir.join("spl_token_2022.so");
        let dest_keypair = programs_dir.join("spl_token_2022-keypair.json");

        if source_so.exists() {
            fs::copy(&source_so, &dest_so).expect("Failed to copy spl_token_2022.so");
            println!("cargo:warning=Copied spl_token_2022.so to programs/");
        } else {
            panic!(
                "spl_token_2022.so not found after build at: {:?}",
                source_so
            );
        }

        if source_keypair.exists() {
            fs::copy(&source_keypair, &dest_keypair)
                .expect("Failed to copy spl_token_2022-keypair.json");
        }
    }
}
