fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=Cargo.toml");

    use std::io::Write;
    std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(std::path::PathBuf::from(std::env::var("OUT_DIR")?).join("deps.rs"))?
        .write_all(
            // read the dependencines from the manifest
            cargo_toml::Manifest::from_path(
                std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?).join("Cargo.toml"),
            )?
            .dependencies
            .iter()
            .map(|(name, meta)| {
                format!("pub use {};\n", {
                    // if alias specified, use that
                    meta.package()
                        .map(|n| n.to_owned())
                        // or use the pacakge name otherwise
                        // replace dashes with underscores
                        .unwrap_or_else(|| name.replace("-", "_"))
                })
            })
            .collect::<String>()
            .as_bytes(),
        )?;
    Ok(())
}
