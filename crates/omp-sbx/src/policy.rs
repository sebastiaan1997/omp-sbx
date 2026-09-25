use std::{fs, io::Write, path::{Path, PathBuf}};

use anyhow::{Context, Result, bail};
use fs4::fs_std::FileExt;
use tempfile::NamedTempFile;

use crate::paths;

const DENY_ALL: &[u8] = b"schemaVersion: 1\nallowedImages: []\n";

#[derive(Debug, Clone)]
pub struct PolicySnapshot { pub directory: PathBuf, pub origin: String }

fn set_private_directory(path: &Path) -> Result<()> {
    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; fs::set_permissions(path, fs::Permissions::from_mode(0o700))?; }
    Ok(())
}
fn set_read_only(path: &Path) -> Result<()> {
    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; fs::set_permissions(path, fs::Permissions::from_mode(0o444))?; }
    #[cfg(windows)] { let mut permissions = fs::metadata(path)?.permissions(); permissions.set_readonly(true); fs::set_permissions(path, permissions)?; }
    Ok(())
}

fn atomic_publish(parent: &Path, destination: &Path, bytes: &[u8]) -> Result<()> {
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    set_read_only(temporary.path())?;
    temporary.persist(destination).map_err(|error| error.error)?;
    Ok(())
}

pub fn prepare(workspace: &Path, sandbox: &str, refresh: bool) -> Result<PolicySnapshot> {
    let workspace = fs::canonicalize(workspace)?;
    let source = workspace.join(".omp-sbx-docker-images.yaml");
    let root = paths::state_root()?.join("omp-sbx").join("docker-image-policies");
    let directory = root.join(sandbox);
    fs::create_dir_all(&directory)?;
    set_private_directory(&root)?;
    set_private_directory(&directory)?;
    let lock = fs::OpenOptions::new().create(true).read(true).write(true).open(directory.join(".lock"))?;
    lock.lock_exclusive()?;
    let snapshot = directory.join(".omp-sbx-docker-images.yaml");
    let origin_file = directory.join(".policy-origin");

    let result = if snapshot.is_file() && !refresh {
        let origin = fs::read_to_string(&origin_file).unwrap_or_else(|_| "snapshot".to_owned());
        Ok(PolicySnapshot { directory: directory.clone(), origin: origin.trim().to_owned() })
    } else {
        let (bytes, origin) = match fs::symlink_metadata(&source) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.file_type().is_file() => bail!("Docker image policy must be a regular non-symlink file: {}", source.display()),
            Ok(_) => (fs::read(&source).with_context(|| format!("read {}", source.display()))?, source.display().to_string()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (DENY_ALL.to_vec(), "deny-all".to_owned()),
            Err(error) => return Err(error.into()),
        };
        atomic_publish(&directory, &snapshot, &bytes)?;
        atomic_publish(&directory, &origin_file, format!("{origin}\n").as_bytes())?;
        Ok(PolicySnapshot { directory: directory.clone(), origin })
    };
    lock.unlock()?;
    result
}

#[cfg(test)]
mod tests {
    use super::DENY_ALL;
    #[test]
    fn deny_all_is_exact() { assert_eq!(DENY_ALL, b"schemaVersion: 1\nallowedImages: []\n"); }
}
