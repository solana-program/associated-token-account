//! Codama IDL generation binary.
use {
    codama::Codama,
    std::{env, fs, path::Path},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR")?;
    let crate_path = Path::new(&manifest_dir);

    let codama = Codama::load(crate_path)?;
    let idl_json = codama.get_json_idl()?;

    let parsed: serde_json::Value = serde_json::from_str(&idl_json)?;
    let mut formatted_json = serde_json::to_string_pretty(&parsed)?;
    formatted_json.push('\n');

    let idl_path = crate_path.join("idl.json");
    fs::write(&idl_path, formatted_json)?;

    println!("IDL written to: {}", idl_path.display());
    Ok(())
}
