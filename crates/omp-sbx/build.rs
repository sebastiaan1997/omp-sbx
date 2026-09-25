use std::{env, fs, io, path::{Path, PathBuf}};

fn copy_tree(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    let mut entries = fs::read_dir(source)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name();
        if name == "target" || name == ".git" {
            continue;
        }
        let source_path = entry.path();
        let destination_path = destination.join(name);
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_tree(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.join("../..").canonicalize()?;
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("omp-sbx-assets");
    if output.exists() {
        fs::remove_dir_all(&output)?;
    }
    fs::create_dir_all(&output)?;

    for file in ["Cargo.toml", "Cargo.lock"] {
        let source = root.join(file);
        if source.is_file() {
            fs::copy(source, output.join(file))?;
            println!("cargo:rerun-if-changed={}", root.join(file).display());
        }
    }
    for directory in ["crates/omp-sbx", "crates/omp-sbx-guest", "sbx-kit", "sbx-configure-kit"] {
        copy_tree(&root.join(directory), &output.join(directory))?;
        println!("cargo:rerun-if-changed={}", root.join(directory).display());
    }
    Ok(())
}
