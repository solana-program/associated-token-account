use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=programs/token");
    println!("cargo:rerun-if-changed=programs/token-2022");

    // Only run this build script when building benchmarks, not during clippy/check
    if is_clippy_or_check() {
        return;
    }

    // This build script runs when building benchmarks to compile token programs

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

fn is_clippy_or_check() -> bool {
    // Check multiple ways to detect clippy or check
    std::env::var("RUSTC_WRAPPER")
        .map(|wrapper| wrapper.contains("clippy-driver"))
        .unwrap_or(false)
        || std::env::var("CARGO_CFG_CLIPPY").is_ok()
        || std::env::var("CARGO_PRIMARY_PACKAGE").is_err() // Not building primary package
        || std::env::args().any(|arg| arg.contains("check") || arg.contains("clippy"))
        || std::env::var("RUSTC").is_ok_and(|rustc| rustc.contains("clippy"))
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

    // Copy the binary to programs directory (build creates target at parent level)
    let token_target_dir = Path::new(manifest_dir).join("programs/token");
    let source_so =
        token_target_dir.join("target/sbpf-solana-solana/release/pinocchio_token_program.so");
    let source_keypair =
        token_target_dir.join("target/deploy/pinocchio_token_program-keypair.json");
    let dest_so = programs_dir.join("pinocchio_token_program.so");
    let dest_keypair = programs_dir.join("pinocchio_token_program-keypair.json");

    if source_so.exists() {
        fs::copy(&source_so, &dest_so).expect("Failed to copy pinocchio_token_program.so");
        println!("cargo:warning=Copied pinocchio_token_program.so to programs/");
    } else {
        panic!("pinocchio_token_program.so not found after build");
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

    // Copy the binary to programs directory (build creates target at parent level)
    let token_2022_target_dir = Path::new(manifest_dir).join("programs/token-2022");
    let source_so =
        token_2022_target_dir.join("target/sbpf-solana-solana/release/spl_token_2022.so");
    let source_keypair = token_2022_target_dir.join("target/deploy/spl_token_2022-keypair.json");
    let dest_so = programs_dir.join("spl_token_2022.so");
    let dest_keypair = programs_dir.join("spl_token_2022-keypair.json");

    if source_so.exists() {
        fs::copy(&source_so, &dest_so).expect("Failed to copy spl_token_2022.so");
        println!("cargo:warning=Copied spl_token_2022.so to programs/");
    } else {
        panic!("spl_token_2022.so not found after build");
    }

    if source_keypair.exists() {
        fs::copy(&source_keypair, &dest_keypair)
            .expect("Failed to copy spl_token_2022-keypair.json");
    }
}
