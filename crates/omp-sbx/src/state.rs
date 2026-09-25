use std::{
    ffi::OsStr,
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use fs4::fs_std::FileExt;
use sha2::{Digest, Sha256};
use tempfile::{Builder, NamedTempFile};

use crate::paths;

const CONFIG_SEED_MARKER: &str = ".omp-sbx-config-seed-sha256";
const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxState {
    pub omp_dir: PathBuf,
}

pub fn prepare(sandbox_name: &str) -> Result<SandboxState> {
    prepare_at(&paths::state_root()?, &paths::omp_dir()?, sandbox_name)
}

fn validate_sandbox_name(name: &str) -> Result<()> {
    if name.is_empty()
        || Path::new(name).components().any(|component| !matches!(component, Component::Normal(_)))
        || Path::new(name).components().count() != 1
    {
        bail!("invalid sandbox name for private state: {name:?}");
    }
    Ok(())
}

fn set_directory_mode(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn set_file_mode(path: &Path, executable: bool) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(if executable { 0o700 } else { 0o600 }))?;
    }
    #[cfg(not(unix))]
    let _ = executable;
    Ok(())
}

fn create_private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("create {}", path.display()))?;
    set_directory_mode(path).with_context(|| format!("secure {}", path.display()))
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("private state file has no parent")?;
    create_private_dir(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    set_file_mode(temporary.path(), false)?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn is_quarantine_subtree(relative: &Path) -> bool {
    let mut parts = relative.components().filter_map(|component| match component {
        Component::Normal(value) => Some(value),
        _ => None,
    });
    if parts.next() != Some(OsStr::new("agent")) {
        return false;
    }
    parts.next().is_some_and(|value| {
        let value = value.to_string_lossy();
        value.starts_with("agent.db-quarantine-") || value.starts_with(".agent-db-quarantine-")
    })
}

fn excluded_name(relative: &Path) -> bool {
    if relative == Path::new("aws-config") || relative.starts_with("aws-sso-cache") {
        return true;
    }
    if is_quarantine_subtree(relative) {
        return true;
    }
    if relative == Path::new("agent/.agent-db-repair.lock")
        || relative == Path::new("agent/.agent-db-repairing")
        || relative == Path::new("agent/.agent-db-repairing.tmp")
    {
        return true;
    }
    let Some(name) = relative.file_name().and_then(OsStr::to_str) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".db")
        || lower.ends_with(".sqlite")
        || lower.ends_with(".sqlite3")
        || lower.ends_with("-wal")
        || lower.ends_with("-shm")
        || lower.ends_with("-journal")
        || (relative.starts_with("agent") && lower.starts_with("agent.db.corrupt-"))
}

fn has_sqlite_header(path: &Path) -> Result<bool> {
    let mut file = File::open(path)?;
    let mut header = [0_u8; SQLITE_HEADER.len()];
    let mut read = 0;
    while read < header.len() {
        let count = file.read(&mut header[read..])?;
        if count == 0 {
            break;
        }
        read += count;
    }
    Ok(read == header.len() && &header == SQLITE_HEADER)
}

fn warn_skip(relative: &Path, reason: &str) {
    eprintln!("omp-sbx: not migrating {}: {reason}", relative.display());
}

fn copy_legacy_tree(source_root: &Path, destination_root: &Path, relative: &Path) -> Result<()> {
    let source = source_root.join(relative);
    let mut entries = fs::read_dir(&source)
        .with_context(|| format!("read legacy OMP state directory {}", source.display()))?
        .collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let relative = relative.join(entry.file_name());
        let source = entry.path();
        let metadata = fs::symlink_metadata(&source)?;
        if excluded_name(&relative) {
            warn_skip(&relative, "persistent credentials, SQLite state, or repair artifacts are sandbox-private");
            continue;
        }
        if metadata.file_type().is_symlink() {
            warn_skip(&relative, "symbolic links are not copied");
            continue;
        }
        if metadata.is_dir() {
            let destination = destination_root.join(&relative);
            create_private_dir(&destination)?;
            copy_legacy_tree(source_root, destination_root, &relative)?;
            continue;
        }
        if !metadata.is_file() {
            warn_skip(&relative, "special files are not copied");
            continue;
        }
        if has_sqlite_header(&source)? {
            warn_skip(&relative, "SQLite files are sandbox-private");
            continue;
        }
        let destination = destination_root.join(&relative);
        create_private_dir(destination.parent().context("migrated file has no parent")?)?;
        fs::copy(&source, &destination)
            .with_context(|| format!("copy legacy OMP state {}", relative.display()))?;
        #[cfg(unix)]
        let executable = {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode() & 0o111 != 0
        };
        #[cfg(not(unix))]
        let executable = false;
        set_file_mode(&destination, executable)?;
    }
    Ok(())
}

fn regular_non_symlink(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(metadata.is_file() && !metadata.file_type().is_symlink()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn seed_digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn converge_config_seed(legacy_omp: &Path, private_omp: &Path) -> Result<()> {
    let agent = private_omp.join("agent");
    if fs::symlink_metadata(&agent).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        bail!("refusing to replace configuration through symlinked private agent directory: {}", agent.display());
    }
    let source = legacy_omp.join("agent/config.yml");
    match fs::symlink_metadata(&source) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            warn_skip(Path::new("agent/config.yml"), "symbolic configuration seed is not copied");
            return Ok(());
        }
        Ok(metadata) if !metadata.is_file() => {
            warn_skip(Path::new("agent/config.yml"), "configuration seed is not a regular file");
            return Ok(());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }

    create_private_dir(&agent)?;
    let bytes = fs::read(&source).with_context(|| format!("read configuration seed {}", source.display()))?;
    let digest = seed_digest(&bytes);
    let marker = private_omp.join(CONFIG_SEED_MARKER);
    if fs::read_to_string(&marker).is_ok_and(|saved| saved.trim() == digest) {
        return Ok(());
    }
    atomic_write(&agent.join("config.yml"), &bytes)?;
    atomic_write(&marker, format!("{digest}\n").as_bytes())?;
    Ok(())
}

pub(crate) fn prepare_at(state_root: &Path, legacy_omp: &Path, sandbox_name: &str) -> Result<SandboxState> {
    validate_sandbox_name(sandbox_name)?;
    let application_root = state_root.join("omp-sbx");
    let sandboxes_root = application_root.join("sandboxes");
    let locks_root = sandboxes_root.join(".locks");
    create_private_dir(&application_root)?;
    create_private_dir(&sandboxes_root)?;
    create_private_dir(&locks_root)?;

    let lock_path = locks_root.join(format!("{sandbox_name}.lock"));
    let lock = fs::OpenOptions::new().create(true).read(true).write(true).open(&lock_path)?;
    set_file_mode(&lock_path, false)?;
    lock.lock_exclusive()?;

    let sandbox_dir = sandboxes_root.join(sandbox_name);
    let private_omp = sandbox_dir.join(".omp");
    let result = (|| -> Result<SandboxState> {
        if !sandbox_dir.exists() {
            let temporary = Builder::new().prefix(".new-").tempdir_in(&sandboxes_root)?;
            set_directory_mode(temporary.path())?;
            let temporary_omp = temporary.path().join(".omp");
            create_private_dir(&temporary_omp)?;
            if legacy_omp.is_dir() {
                copy_legacy_tree(legacy_omp, &temporary_omp, Path::new(""))?;
                if regular_non_symlink(&legacy_omp.join("agent/agent.db"))?
                    && !regular_non_symlink(&legacy_omp.join("agent/config.yml"))?
                    && !regular_non_symlink(&legacy_omp.join("agent/config.yaml"))?
                {
                    eprintln!("omp-sbx: legacy agent/agent.db exists without config.yml or config.yaml; DB-backed settings are intentionally not imported");
                }
            }
            converge_config_seed(legacy_omp, &temporary_omp)?;
            let temporary_path = temporary.keep();
            fs::rename(&temporary_path, &sandbox_dir).with_context(|| {
                format!("publish private state {}", sandbox_dir.display())
            })?;
        }
        let metadata = fs::symlink_metadata(&sandbox_dir)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            bail!("private sandbox state is not a directory: {}", sandbox_dir.display());
        }
        create_private_dir(&sandbox_dir)?;
        match fs::symlink_metadata(&private_omp) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                bail!("private OMP state is not a directory: {}", private_omp.display());
            }
            Ok(_) => set_directory_mode(&private_omp)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => create_private_dir(&private_omp)?,
            Err(error) => return Err(error.into()),
        }
        converge_config_seed(legacy_omp, &private_omp)?;
        Ok(SandboxState { omp_dir: private_omp })
    })();
    lock.unlock()?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn prepare_fixture(root: &TempDir, name: &str) -> SandboxState {
        prepare_at(&root.path().join("state"), &root.path().join("legacy"), name).unwrap()
    }

    #[test]
    fn sandbox_names_have_distinct_persistent_roots() {
        let root = TempDir::new().unwrap();
        let first = prepare_fixture(&root, "omp-one");
        let second = prepare_fixture(&root, "omp-two");
        assert_ne!(first.omp_dir, second.omp_dir);
        assert!(first.omp_dir.is_dir());
        assert!(second.omp_dir.is_dir());
    }

    #[test]
    fn migrates_regular_files_but_not_databases_aws_or_repair_artifacts() {
        let root = TempDir::new().unwrap();
        let legacy = root.path().join("legacy");
        fs::create_dir_all(legacy.join("agent/agent.db-quarantine-old")).unwrap();
        fs::create_dir_all(legacy.join("agent/.agent-db-quarantine-old")).unwrap();
        fs::create_dir_all(legacy.join("aws-sso-cache")).unwrap();
        fs::create_dir_all(legacy.join("sessions")).unwrap();
        fs::write(legacy.join("settings.yml"), "theme: dark\n").unwrap();
        fs::write(legacy.join("sessions/session.jsonl"), "{}\n").unwrap();
        fs::write(legacy.join("agent/agent.db"), SQLITE_HEADER).unwrap();
        fs::write(legacy.join("history.sqlite3"), "not sqlite").unwrap();
        fs::write(legacy.join("agent.db-wal"), "wal").unwrap();
        fs::write(legacy.join("agent/agent.db.corrupt-1"), "bad").unwrap();
        fs::write(legacy.join("agent/.agent-db-repair.lock"), "lock").unwrap();
        fs::write(legacy.join("agent/agent.db-quarantine-old/copy"), "bad").unwrap();
        fs::write(legacy.join("agent/.agent-db-quarantine-old/copy"), "bad").unwrap();
        fs::write(legacy.join("aws-config"), "secret").unwrap();
        fs::write(legacy.join("aws-sso-cache/token"), "secret").unwrap();
        fs::write(legacy.join("opaque"), [SQLITE_HEADER.as_slice(), b"payload"].concat()).unwrap();

        let state = prepare_at(&root.path().join("state"), &legacy, "omp-test").unwrap();
        assert_eq!(fs::read_to_string(state.omp_dir.join("settings.yml")).unwrap(), "theme: dark\n");
        assert!(state.omp_dir.join("sessions/session.jsonl").is_file());
        for relative in [
            "agent/agent.db",
            "history.sqlite3",
            "agent.db-wal",
            "agent/agent.db.corrupt-1",
            "agent/.agent-db-repair.lock",
            "agent/agent.db-quarantine-old",
            "agent/.agent-db-quarantine-old",
            "aws-config",
            "aws-sso-cache",
            "opaque",
        ] {
            assert!(!state.omp_dir.join(relative).exists(), "{relative} was migrated");
        }
    }

    #[test]
    fn unchanged_seed_preserves_private_edits_and_changed_seed_applies_once() {
        let root = TempDir::new().unwrap();
        let legacy = root.path().join("legacy/agent");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("config.yml"), "value: one\n").unwrap();
        let first = prepare_fixture(&root, "omp-test");
        let private = first.omp_dir.join("agent/config.yml");
        fs::write(&private, "private: edit\n").unwrap();
        prepare_fixture(&root, "omp-test");
        assert_eq!(fs::read_to_string(&private).unwrap(), "private: edit\n");

        fs::write(legacy.join("config.yml"), "value: two\n").unwrap();
        prepare_fixture(&root, "omp-test");
        assert_eq!(fs::read_to_string(&private).unwrap(), "value: two\n");
        fs::write(&private, "private: second\n").unwrap();
        prepare_fixture(&root, "omp-test");
        assert_eq!(fs::read_to_string(&private).unwrap(), "private: second\n");
    }
    #[test]
    fn concurrent_prepare_serializes_first_publication() {
        let root = TempDir::new().unwrap();
        let state_root = root.path().join("state");
        let legacy = root.path().join("legacy");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("settings.yml"), "value: stable\n").unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let threads = (0..8).map(|_| {
            let state_root = state_root.clone();
            let legacy = legacy.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                prepare_at(&state_root, &legacy, "omp-test").unwrap().omp_dir
            })
        }).collect::<Vec<_>>();
        let paths = threads.into_iter().map(|thread| thread.join().unwrap()).collect::<Vec<_>>();
        assert!(paths.iter().all(|path| path == &paths[0]));
        assert_eq!(fs::read_to_string(paths[0].join("settings.yml")).unwrap(), "value: stable\n");
    }


    #[cfg(unix)]
    #[test]
    fn skips_symlinks_and_preserves_only_executable_class() {
        use std::{os::unix::{fs::{PermissionsExt, symlink}, net::UnixListener}};
        let root = TempDir::new().unwrap();
        let legacy = root.path().join("legacy");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("tool"), "#!/bin/sh\n").unwrap();
        fs::set_permissions(legacy.join("tool"), fs::Permissions::from_mode(0o755)).unwrap();
        symlink(legacy.join("tool"), legacy.join("linked-tool")).unwrap();
        let _socket = UnixListener::bind(legacy.join("socket")).unwrap();
        let state = prepare_fixture(&root, "omp-test");
        assert_eq!(fs::metadata(state.omp_dir.join("tool")).unwrap().permissions().mode() & 0o777, 0o700);
        assert!(!state.omp_dir.join("linked-tool").exists());
        assert!(!state.omp_dir.join("socket").exists());
        assert_eq!(fs::metadata(&state.omp_dir).unwrap().permissions().mode() & 0o777, 0o700);
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinked_private_agent_parent() {
        use std::os::unix::fs::symlink;
        let root = TempDir::new().unwrap();
        let legacy_agent = root.path().join("legacy/agent");
        fs::create_dir_all(&legacy_agent).unwrap();
        fs::write(legacy_agent.join("config.yml"), "seed: true\n").unwrap();
        let state = prepare_fixture(&root, "omp-test");
        fs::remove_dir_all(state.omp_dir.join("agent")).unwrap();
        let outside = root.path().join("outside");
        fs::create_dir(&outside).unwrap();
        symlink(&outside, state.omp_dir.join("agent")).unwrap();
        fs::remove_file(legacy_agent.join("config.yml")).unwrap();
        assert!(prepare_at(&root.path().join("state"), &root.path().join("legacy"), "omp-test").is_err());
        assert!(!outside.join("config.yml").exists());
    }
}
