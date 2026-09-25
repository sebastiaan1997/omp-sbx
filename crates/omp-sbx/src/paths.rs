use std::{env, fs, path::{Path, PathBuf}};

use anyhow::{Context, Result, bail};
use directories::BaseDirs;

pub fn home() -> Result<PathBuf> {
    if let Some(home) = env::var_os("HOME").filter(|value| !value.is_empty()) { return Ok(PathBuf::from(home)); }
    if let Some(home) = env::var_os("USERPROFILE").filter(|value| !value.is_empty()) { return Ok(PathBuf::from(home)); }
    BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf()).context("cannot resolve home directory")
}

pub fn workspace() -> Result<PathBuf> {
    fs::canonicalize(env::current_dir()?).context("resolve current workspace")
}

pub fn omp_dir() -> Result<PathBuf> { Ok(home()?.join(".omp")) }

pub fn state_root() -> Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_STATE_HOME") { return Ok(PathBuf::from(path)); }
    #[cfg(windows)]
    if let Some(path) = env::var_os("LOCALAPPDATA") { return Ok(PathBuf::from(path)); }
    Ok(home()?.join(".local").join("state"))
}

pub fn gh_config() -> Result<Option<PathBuf>> {
    if let Some(path) = env::var_os("GH_CONFIG_DIR") { return Ok(Some(PathBuf::from(path))); }
    #[cfg(windows)]
    let path = env::var_os("APPDATA").map(PathBuf::from).map(|root| root.join("GitHub CLI"));
    #[cfg(not(windows))]
    let path = Some(env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or(home()?.join(".config")).join("gh"));
    Ok(path.filter(|path| path.is_dir()))
}

pub fn skills_dir() -> Result<Option<PathBuf>> {
    if let Some(path) = env::var_os("OMP_SBX_SKILLS_DIR") { return Ok(Some(PathBuf::from(path))); }
    #[cfg(windows)]
    let path = env::var_os("LOCALAPPDATA").map(PathBuf::from).map(|root| root.join("omp").join("skills"));
    #[cfg(not(windows))]
    let path = Some(env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or(home()?.join(".local/share")).join("omp").join("skills"));
    Ok(path.filter(|path| path.is_dir()))
}

pub fn executable() -> Result<PathBuf> { env::current_exe().context("resolve omp-sbx executable") }

pub fn require_directory(path: &Path, label: &str) -> Result<()> {
    if !path.is_dir() { bail!("{label} is not a directory: {}", path.display()); }
    Ok(())
}

pub fn sbx_argument(path: &Path) -> String {
    let value = path.to_string_lossy().into_owned();
    #[cfg(windows)]
    { value.replace('\\', "/") }
    #[cfg(not(windows))]
    { value }
}
