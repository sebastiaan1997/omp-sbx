use std::{ffi::{OsStr, OsString}, path::Path, process::{ExitStatus, Output}};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::{paths, process::{self, CommandSpec}};
pub const MINIMUM_SBX_VERSION: (u64, u64, u64) = (0, 43, 0);


#[derive(Debug, Clone, Default)]
pub struct Sbx;

#[derive(Debug, Deserialize)]
struct SandboxList { #[serde(default)] sandboxes: Vec<Sandbox> }
#[derive(Debug, Deserialize)]
struct Sandbox { name: String, status: String, #[serde(default)] workspaces: Vec<String> }


#[derive(Debug, Deserialize)]
struct Version {
    version: String,
}
#[derive(Debug, Deserialize)]
struct VersionResult {
    client: Version,
}

#[derive(Debug, PartialEq, Eq)]
enum DaemonState {
    Running,
    Stopped,
}

fn parse_version_result(bytes: &[u8]) -> Result<VersionResult> {
    serde_json::from_slice(bytes).context("sbx version --json")
}

fn parse_version_string(version: &str) -> Result<(u64, u64, u64)> {
    let mut parts = version.trim_start_matches('v').split('.').map(|part| part.trim_matches(|c: char| !c.is_ascii_digit()).parse::<u64>());
    Ok((
        parts.next().context("missing sbx major")??,
        parts.next().context("missing sbx minor")??,
        parts.next().unwrap_or(Ok(0))?,
    ))
}


fn daemon_state(output: &[u8]) -> Result<DaemonState> {
    let output = String::from_utf8(output.to_vec()).context("parse sbx daemon status output as UTF-8")?;
    if output.lines().any(|line| line.trim() == "Status: running") {
        return Ok(DaemonState::Running);
    }
    if output.lines().any(|line| line.trim() == "Status: stopped") {
        return Ok(DaemonState::Stopped);
    }
    bail!("unrecognized sbx daemon status output: {}", output.trim())
}

impl Sbx {
    fn daemon_status(&self) -> Result<DaemonState> {
        let output = self.command(["daemon", "status"]).checked_output().context("check sbx daemon status")?;
        daemon_state(&output.stdout)
    }

    fn ensure_daemon_running(&self) -> Result<()> {
        if self.daemon_status()? == DaemonState::Running {
            return Ok(());
        }

        self.command(["daemon", "start", "--detach"]).checked_output().context("start sbx daemon")?;
        if self.daemon_status()? != DaemonState::Running {
            bail!("sbx daemon did not report running after startup");
        }
        Ok(())
    }

    pub fn command<I, S>(&self, args: I) -> CommandSpec where I: IntoIterator<Item = S>, S: Into<OsString> {
        CommandSpec::new("sbx").args(args)
    }
    pub fn output<I, S>(&self, args: I) -> Result<Output> where I: IntoIterator<Item = S>, S: Into<OsString> { self.command(args).output() }
    pub fn status<I, S>(&self, args: I) -> Result<ExitStatus> where I: IntoIterator<Item = S>, S: Into<OsString> { self.command(args).status() }
    pub fn checked<I, S>(&self, args: I) -> Result<()> where I: IntoIterator<Item = S>, S: Into<OsString> { self.command(args).checked() }
    pub fn exists(&self, name: &str) -> Result<bool> {
        let output = self.output(["ls", "--json"])?;
        if output.status.success() {
            if let Ok(list) = serde_json::from_slice::<SandboxList>(&output.stdout) { return Ok(list.sandboxes.iter().any(|sandbox| sandbox.name == name)); }
        }
        let quiet = self.output(["ls", "--quiet"])?;
        if !quiet.status.success() { bail!("sbx ls failed with {}", quiet.status); }
        Ok(String::from_utf8_lossy(&quiet.stdout).lines().any(|line| line.trim() == name))
    }
    pub fn is_running(&self, name: &str) -> Result<bool> {
        let output = self.output(["ls", "--json"])?;
        if output.status.success() {
            if let Ok(list) = serde_json::from_slice::<SandboxList>(&output.stdout) {
                return Ok(list.sandboxes.iter().any(|sandbox| sandbox.name == name && sandbox.status == "running"));
            }
        }
        Ok(false)
    }
    fn sandbox(&self, name: &str) -> Result<Option<Sandbox>> {
        let output = self.output(["ls", "--json"])?;
        if !output.status.success() {
            bail!("sbx ls --json exited with {}", output.status);
        }
        let list: SandboxList = serde_json::from_slice(&output.stdout).context("parse sbx ls --json")?;
        Ok(list.sandboxes.into_iter().find(|sandbox| sandbox.name == name))
    }
    pub fn has_workspace(&self, name: &str, path: &Path) -> Result<bool> {
        let target = paths::sbx_argument(path);
        Ok(self.sandbox(name)?.is_some_and(|sandbox| sandbox.workspaces.iter().any(|workspace| workspace == &target)))
    }
    pub fn has_workspace_mount(&self, name: &str, path: &Path) -> Result<bool> {
        let target = paths::sbx_argument(path);
        Ok(self.sandbox(name)?.is_some_and(|sandbox| sandbox.workspaces.iter().any(|workspace| workspace.strip_suffix(":ro").unwrap_or(workspace) == target)))
    }

    fn create_command(
        &self,
        name: &str,
        kit: &Path,
        workspace: &Path,
        mounts: &[OsString],
        options: &[OsString],
    ) -> CommandSpec {
        let mut args = vec![OsString::from("create")];
        args.extend_from_slice(options);
        args.extend([
            OsString::from("--name"),
            OsString::from(name),
            OsString::from(paths::sbx_argument(kit)),
            OsString::from(paths::sbx_argument(workspace)),
        ]);
        args.extend_from_slice(mounts);
        self.command(args)
    }
    pub fn create(
        &self, name: &str, kit: &Path, workspace: &Path,
        mounts: &[OsString], options: &[OsString],
    ) -> Result<Output> {
        self.create_command(name, kit, workspace, mounts, options).output()
    }
    pub fn create_interactive(
        &self, name: &str, kit: &Path, workspace: &Path,
        mounts: &[OsString], options: &[OsString],
    ) -> Result<()> {
        // output() closes stdin and pipes both output streams, preventing
        // sbx from asking for third-party kit credential approval.
        if !crate::terminal::stdin_is_terminal() {
            bail!("interactive sandbox creation requires a terminal");
        }
        self.create_command(name, kit, workspace, mounts, options).checked()
    }
    pub fn run(&self, name: &str) -> Result<ExitStatus> {
        self.status(["run", "--name", name])
    }
    pub fn exec(&self, name: &str, working_directory: Option<&str>, args: &[String]) -> Result<ExitStatus> {
        let mut argv = vec![OsString::from("exec")];
        if let Some(directory) = working_directory {
            argv.extend([OsString::from("-w"), OsString::from(directory)]);
        }
        argv.extend([OsString::from(name), OsString::from("--")]);
        argv.extend(args.iter().map(OsString::from));
        self.status(argv)
    }
    pub fn exec_output(&self, name: &str, working_directory: Option<&str>, args: &[String]) -> Result<Output> {
        let mut argv = vec![OsString::from("exec")];
        if let Some(directory) = working_directory { argv.extend([OsString::from("-w"), OsString::from(directory)]); }
        argv.extend([OsString::from(name), OsString::from("--")]);
        argv.extend(args.iter().map(OsString::from));
        self.output(argv)
    }
    pub fn stop(&self, name: &str) -> Result<ExitStatus> { self.status(["stop", name]) }
    pub fn remove(&self, name: &str) -> Result<ExitStatus> { self.status(["rm", "-f", name]) }
    pub fn inspect(&self, name: &str) -> Result<Output> { self.output(["inspect", name]) }

    pub fn version(&self) -> Result<(u64, u64, u64)> {
        let output = self.command(["version", "--json"]).checked_output()?;
        let parsed = parse_version(&output.stdout)?;
        println!("Found version: {}.{}.{}", parsed.0, parsed.1, parsed.2);
        Ok(parsed)
    }
}

pub fn ensure_minimum_version(sbx: &Sbx, required: (u64, u64, u64)) -> Result<()> {
    sbx.ensure_daemon_running()?;
    let installed = sbx.version()?;
    if installed < required { bail!("sbx {}.{}.{} is installed; {}.{}.{} or newer is required", installed.0, installed.1, installed.2, required.0, required.1, required.2); }
    Ok(())
}

pub fn executable_exists() -> bool { process::exists(OsStr::new("sbx")) }



#[cfg(test)]
mod tests {
    use super::parse_version;

    #[test]
    fn parses_client_only_version() {
        assert_eq!(parse_version(br#"{"client":{"version":"v0.43.0"}}"#).unwrap(), (0, 43, 0));
    }

    #[test]
    fn parses_unavailable_server_version() {
        assert_eq!(parse_version(br#"{"client":{"version":"0.43.1"},"server":{"state":"unavailable"}}"#).unwrap(), (0, 43, 1));
    }

    #[test]
    fn rejects_missing_client_version() {
        assert!(parse_version(br#"{"server":{"state":"unavailable"}}"#).is_err());
    }

    #[test]
    fn rejects_malformed_version() {
        assert!(parse_version(br#"{"client":{"version":"not-a-version"}}"#).is_err());
    }
}


