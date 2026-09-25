use std::{ffi::OsString, path::Path, thread, time::Duration};

use anyhow::{Result, bail};

use crate::{process::{CommandSpec, exit_code}, sbx::Sbx, terminal};

fn create_collision(output: &[u8]) -> bool {
    let text = String::from_utf8_lossy(output).to_ascii_lowercase();
    text.contains("already exists") || text.contains("given new workspaces")
}

pub fn create_if_needed(sbx: &Sbx, name: &str, kit: &Path, workspace: &Path, mounts: &[OsString], options: &[OsString], exists: bool) -> Result<bool> {
    if exists { return Ok(false); }
    let output = sbx.create(name, kit, workspace, mounts, options)?;
    if !output.stdout.is_empty() { eprint!("{}", String::from_utf8_lossy(&output.stdout)); }
    if !output.stderr.is_empty() { eprint!("{}", String::from_utf8_lossy(&output.stderr)); }
    if output.status.success() {
        thread::sleep(Duration::from_secs(2));
        return Ok(true);
    }
    if create_collision(&output.stderr) || create_collision(&output.stdout) {
        eprintln!("omp-sbx: sandbox already exists (stopped?), will re-attach");
        return Ok(false);
    }
    bail!("failed to create sandbox '{name}'")
}

pub fn interactive(sbx: &Sbx, name: &str, kit: &Path, workspace: &Path, mounts: &[OsString], options: &[OsString], exists: bool, yes: bool) -> Result<()> {
    if !exists {
        sbx.create_interactive(name, kit, workspace, mounts, options)?;
        thread::sleep(Duration::from_secs(2));
    }
    if !yes { terminal::pause_key("Press any key to launch omp (or --yes to skip this)...")?; }
    if sbx.run(name)?.success() { return Ok(()); }
    eprintln!("omp-sbx: re-attach failed; restarting sandbox...");
    let _ = sbx.stop(name);
    if sbx.run(name)?.success() { return Ok(()); }
    eprintln!("omp-sbx: sandbox unresponsive, recreating...");
    let _ = sbx.remove(name);
    sbx.create_interactive(name, kit, workspace, mounts, options)?;
    thread::sleep(Duration::from_secs(2));
    CommandSpec::new("sbx").args(["run", "--name", name]).replace()
}

pub fn one_shot(sbx: &Sbx, name: &str, kit: &Path, workspace: &Path, mounts: &[OsString], options: &[OsString], exists: bool, omp_args: &[String]) -> Result<()> {
    let _ = create_if_needed(sbx, name, kit, workspace, mounts, options, exists);
    let mut command = vec!["/usr/local/bin/omp-init.sh".to_owned()];
    command.extend_from_slice(omp_args);
    let status = sbx.exec(name, Some("/home/agent"), &command)?;
    let code = exit_code(status);
    if code != 0 { std::process::exit(code); }
    Ok(())
}
