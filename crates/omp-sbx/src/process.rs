use std::{collections::BTreeMap, ffi::{OsStr, OsString}, io, path::{Path, PathBuf}, process::{Command, ExitStatus, Output}, sync::atomic::{AtomicBool, Ordering}};

use anyhow::{Context, Result, bail};

static DEBUG: AtomicBool = AtomicBool::new(false);

pub fn set_debug(enabled: bool) { DEBUG.store(enabled, Ordering::Relaxed); }

#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
    pub env: BTreeMap<OsString, Option<OsString>>,
}

impl CommandSpec {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self { program: program.into(), args: Vec::new(), cwd: None, env: BTreeMap::new() }
    }

    pub fn arg(mut self, arg: impl Into<OsString>) -> Self { self.args.push(arg.into()); self }
    pub fn args<I, S>(mut self, args: I) -> Self where I: IntoIterator<Item = S>, S: Into<OsString> { self.args.extend(args.into_iter().map(Into::into)); self }
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self { self.cwd = Some(cwd.into()); self }
    pub fn env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self { self.env.insert(key.into(), Some(value.into())); self }
    pub fn env_remove(mut self, key: impl Into<OsString>) -> Self { self.env.insert(key.into(), None); self }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.args);
        if let Some(cwd) = &self.cwd { command.current_dir(cwd); }
        for (key, value) in &self.env {
            if let Some(value) = value { command.env(key, value); } else { command.env_remove(key); }
        }
        command
    }

    fn debug_enabled() -> bool { DEBUG.load(Ordering::Relaxed) }
    fn debug_command(&self) {
        if Self::debug_enabled() { eprintln!("external command: {} {}", self.program.display(), self.args.iter().map(|arg| arg.to_string_lossy()).collect::<Vec<_>>().join(" ")); }
    }

    pub fn status(&self) -> Result<ExitStatus> {
        self.debug_command();
        let status = self.command().status().with_context(|| format!("execute {}", self.program.display()))?;
        if Self::debug_enabled() { eprintln!("external status: {status}"); }
        Ok(status)
    }

    pub fn output(&self) -> Result<Output> {
        self.debug_command();
        let output = self.command().output().with_context(|| format!("execute {}", self.program.display()))?;
        if Self::debug_enabled() {
            eprintln!("external status: {}", output.status);
            eprintln!("external stdout:\n{}", String::from_utf8_lossy(&output.stdout));
            eprintln!("external stderr:\n{}", String::from_utf8_lossy(&output.stderr));
        }
        Ok(output)
    }

    pub fn checked(&self) -> Result<()> {
        let status = self.status()?;
        if !status.success() { bail!("{} exited with {status}", self.program.display()); }
        Ok(())
    }

    pub fn checked_output(&self) -> Result<Output> {
        let output = self.output()?;
        if !output.status.success() {
            bail!("{} exited with {}: {}", self.program.display(), output.status, String::from_utf8_lossy(&output.stderr).trim());
        }
        Ok(output)
    }

    pub fn replace(&self) -> Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let error = self.command().exec();
            Err(error).with_context(|| format!("exec {}", self.program.display()))
        }
        #[cfg(windows)]
        {
            let status = self.status()?;
            std::process::exit(status.code().unwrap_or(1));
        }
    }
}

pub fn exists(program: impl AsRef<OsStr>) -> bool {
    let program = program.as_ref();
    let path = Path::new(program);
    if path.components().count() > 1 { return path.is_file(); }
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|directory| {
            let candidate = directory.join(program);
            if candidate.is_file() { return true; }
            #[cfg(windows)]
            { ["exe", "cmd", "bat"].iter().any(|ext| candidate.with_extension(ext).is_file()) }
            #[cfg(not(windows))]
            { false }
        })
    })
}

pub fn exit_code(status: ExitStatus) -> i32 {
    status.code().unwrap_or_else(|| {
        #[cfg(unix)] { use std::os::unix::process::ExitStatusExt; 128 + status.signal().unwrap_or(0) }
        #[cfg(not(unix))] { 1 }
    })
}

pub fn read_utf8(output: Output) -> Result<String> {
    String::from_utf8(output.stdout).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error).into())
}
