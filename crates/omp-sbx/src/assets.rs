use std::{fs, io::Write, path::{Path, PathBuf}};

use anyhow::{Context, Result, bail};
use fs4::fs_std::FileExt;
use include_dir::{Dir, include_dir};
use sha2::{Digest, Sha256};
use tempfile::Builder;

static ASSETS: Dir<'_> = include_dir!("$OUT_DIR/omp-sbx-assets");

#[derive(Debug, Clone)]
pub struct MaterializedAssets {
    pub root: PathBuf,
    pub main_kit: PathBuf,
    pub configure_kit: PathBuf,
}

fn visit(dir: &Dir<'_>, prefix: &Path, files: &mut Vec<(PathBuf, Vec<u8>)>) {
    for file in dir.files() {
        files.push((prefix.join(file.path()), file.contents().to_vec()));
    }
    for child in dir.dirs() {
        visit(child, prefix, files);
    }
}

fn embedded_files() -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    visit(&ASSETS, Path::new(""), &mut files);
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
}

fn digest(files: &[(PathBuf, Vec<u8>)]) -> String {
    let mut hasher = Sha256::new();
    for (path, bytes) in files {
        let path = path.to_string_lossy();
        hasher.update((path.len() as u64).to_le_bytes());
        hasher.update(path.as_bytes());
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }
    hex::encode(hasher.finalize())
}

fn publish(files: &[(PathBuf, Vec<u8>)], destination: &Path) -> Result<()> {
    let parent = destination.parent().context("asset cache path has no parent")?;
    fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    let temp = Builder::new().prefix(".kits-").tempdir_in(parent)?;
    for (relative, bytes) in files {
        let output = temp.path().join(relative);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::File::create(&output)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    let path = temp.keep();
    match fs::rename(&path, destination) {
        Ok(()) => Ok(()),
        Err(_error) if destination.is_dir() => { fs::remove_dir_all(path)?; Ok(()) }
        Err(error) => Err(error.into()),
    }
}

pub fn materialize() -> Result<MaterializedAssets> {
    let files = embedded_files();
    if files.is_empty() {
        bail!("embedded kit is empty");
    }
    let cache = directories::BaseDirs::new().context("cannot resolve user cache directory")?;
    let kits = cache.cache_dir().join("omp-sbx").join("kits");
    fs::create_dir_all(&kits)?;
    let lock_path = kits.join(".lock");
    let lock = fs::OpenOptions::new().create(true).read(true).write(true).open(&lock_path)?;
    lock.lock_exclusive()?;
    let root = kits.join(digest(&files));
    let result = if !root.is_dir() {
        publish(&files, &root)
    } else {
        Ok(())
    };
    lock.unlock()?;
    result?;

    Ok(MaterializedAssets {
        main_kit: root.join("sbx-kit"),
        configure_kit: root.join("sbx-configure-kit"),
        root,
    })
}

pub fn resolve() -> Result<MaterializedAssets> {
    let mut assets = materialize()?;
    if let Some(path) = std::env::var_os("OMP_SBX_KIT") {
        assets.main_kit = PathBuf::from(path);
    }
    if let Some(path) = std::env::var_os("OMP_SBX_CONFIGURE_KIT") {
        assets.configure_kit = PathBuf::from(path);
    }
    Ok(assets)
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_workspace_has_both_members() {
        let files = super::embedded_files();
        assert!(files.iter().any(|(path, _)| path == std::path::Path::new("crates/omp-sbx/Cargo.toml")));
        assert!(files.iter().any(|(path, _)| path == std::path::Path::new("crates/omp-sbx-guest/Cargo.toml")));
    }
}
