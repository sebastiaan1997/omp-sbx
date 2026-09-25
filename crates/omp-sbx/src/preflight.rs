use std::{io::Write, path::Path};

use anyhow::{Context, Result, bail};
use tempfile::NamedTempFile;

use crate::{process::{self, CommandSpec}, sbx::Sbx, terminal};

pub fn sandbox_name_for(label: &str) -> String {
    let slug: String = label.chars().filter_map(|character| match character {
        '_' => Some('-'),
        '-' if character.is_ascii() => Some(character),
        value if value.is_ascii_alphanumeric() => Some(value),
        _ => None,
    }).collect();
    if slug == "omp" || slug.starts_with("omp-") { slug } else { format!("omp-{slug}") }
}

pub fn ensure_sbx(command_name: &str) -> Result<()> {
    if process::exists("sbx") { return Ok(()); }
    eprintln!("{command_name}: sbx CLI not found.");
    if !terminal::stdin_is_terminal() { bail!("Install sbx and re-run. See: https://github.com/docker/sbx"); }

    #[cfg(windows)]
    {
        eprintln!("Install plan:\n  1) winget install -h Docker.sbx\n  2) sbx login");
        if !terminal::confirm("Run these steps now? [y/N] ")? { bail!("aborted"); }
        CommandSpec::new("winget").args(["install", "-h", "Docker.sbx"]).checked()?;
        CommandSpec::new("sbx").arg("login").checked()?;
    }
    #[cfg(target_os = "linux")]
    {
        eprintln!("Install plan:\n  1) install the Docker apt repository\n  2) install docker-sbx\n  3) add the user to kvm\n  4) sbx login");
        if !terminal::confirm("Run these steps now? [y/N] ")? { bail!("aborted"); }
        let response = reqwest::blocking::get("https://get.docker.com")?.error_for_status()?;
        let mut installer = NamedTempFile::new()?;
        installer.write_all(&response.bytes()?)?;
        installer.as_file().sync_all()?;
        CommandSpec::new("sudo").args(["env", "REPO_ONLY=1", "sh"]).arg(installer.path()).checked()?;
        CommandSpec::new("sudo").args(["apt-get", "install", "-y", "docker-sbx"]).checked()?;
        let user = std::env::var("USER").context("USER is not set")?;
        CommandSpec::new("sudo").args(["usermod", "-aG", "kvm", &user]).checked()?;
        eprintln!("Note: run 'newgrp kvm' (or log out/in) for kvm access.");
        CommandSpec::new("sbx").arg("login").checked()?;
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    bail!("only native Windows and Linux hosts are supported");
    if !process::exists("sbx") { bail!("sbx installed but is not on PATH; open a new terminal and re-run"); }
    Ok(())
}

pub fn require_docker() -> Result<()> {
    if !process::exists("docker") { bail!("docker is required for image builds"); }
    Ok(())
}

pub fn sandbox_kit_is_stale(sbx: &Sbx, name: &str, kit: &Path) -> Result<bool> {
    let output = sbx.inspect(name)?;
    if !output.status.success() { return Ok(false); }
    let text = String::from_utf8_lossy(&output.stdout);
    let Some(recorded) = text.lines().find_map(|line| line.trim().strip_prefix("Kits:").map(str::trim)) else { return Ok(false); };
    let current = kit.to_string_lossy();
    Ok(!recorded.is_empty() && recorded != current)
}

pub fn drop_stale_sandbox(sbx: &Sbx, name: &str, kit: &Path) -> Result<bool> {
    if !sandbox_kit_is_stale(sbx, name, kit)? { return Ok(false); }
    eprintln!("sandbox '{name}' was created from a different kit; recreating it because sbx cannot repoint an existing sandbox");
    let status = sbx.remove(name)?;
    if !status.success() { bail!("failed to remove stale sandbox '{name}': {status}"); }
    Ok(true)
}

pub fn drop_incompatible_state_sandbox(sbx: &Sbx, name: &str, expected: &Path, legacy: &Path) -> Result<bool> {
    if !sbx.exists(name)? {
        return Ok(false);
    }
    let has_expected = sbx.has_workspace(name, expected)?;
    let has_legacy = sbx.has_workspace_mount(name, legacy)?;
    if has_expected && !has_legacy {
        return Ok(false);
    }
    eprintln!(
        "sandbox '{name}' has incompatible OMP state mounts; recreating it with private state at {}",
        expected.display()
    );
    let status = sbx.remove(name)?;
    if !status.success() {
        bail!("failed to remove sandbox '{name}' with incompatible OMP state mounts: {status}");
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::sandbox_name_for;
    #[test]
    fn naming_is_ascii_and_stable() {
        assert_eq!(sandbox_name_for("hello_world"), "omp-hello-world");
        assert_eq!(sandbox_name_for("omp-sbx"), "omp-sbx");
        assert_eq!(sandbox_name_for("hé llö"), "omp-hll");
    }
}
