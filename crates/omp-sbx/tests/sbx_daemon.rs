#![cfg(unix)]

use std::{env, fs, os::unix::fs::PermissionsExt, path::PathBuf, process::{Command, Output}};

use tempfile::TempDir;

struct Fixture {
    _root: TempDir,
    fake_bin: PathBuf,
    home: PathBuf,
    cache: PathBuf,
    state: PathBuf,
    workspace: PathBuf,
    log: PathBuf,
    running: PathBuf,
    created: PathBuf,
    mode: Option<&'static str>,
}

impl Fixture {
    fn new(mode: Option<&'static str>) -> Self {
        let root = TempDir::new().expect("temporary fixture");
        let fake_bin = root.path().join("bin");
        let home = root.path().join("home");
        let cache = root.path().join("cache");
        let state = root.path().join("state");
        let workspace = root.path().join("project");
        fs::create_dir_all(&fake_bin).expect("fake bin directory");
        fs::create_dir_all(&home).expect("temporary home");
        fs::create_dir_all(&cache).expect("temporary cache");
        fs::create_dir_all(&state).expect("temporary state");
        fs::create_dir_all(&workspace).expect("temporary workspace");

        let log = root.path().join("sbx.log");
        fs::write(&log, "").expect("fake log");
        let running = root.path().join("daemon.running");
        let created = root.path().join("sandbox.created");
        let fake_sbx = fake_bin.join("sbx");
        fs::write(&fake_sbx, FAKE_SBX).expect("fake sbx");
        let mut permissions = fs::metadata(&fake_sbx).expect("fake sbx metadata").permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&fake_sbx, permissions).expect("make fake sbx executable");

        Self { _root: root, fake_bin, home, cache, state, workspace, log, running, created, mode }
    }

    fn run(&self) -> Output {
        let mut path_entries = vec![self.fake_bin.clone()];
        if let Some(path) = env::var_os("PATH") {
            path_entries.extend(env::split_paths(&path));
        }
        let path = env::join_paths(path_entries).expect("fixture PATH");
        let mut command = Command::new(env!("CARGO_BIN_EXE_omp-sbx"));
        command
            .current_dir(&self.workspace)
            .env("PATH", path)
            .env("HOME", &self.home)
            .env("XDG_CACHE_HOME", &self.cache)
            .env("XDG_STATE_HOME", &self.state)
            .env("FAKE_SBX_LOG", &self.log)
            .env("FAKE_SBX_RUNNING", &self.running)
            .env("FAKE_SBX_STATE", &self.state)
            .env("FAKE_SBX_CREATED", &self.created)
            .args(["run", "--", "--version"]);
        if let Some(mode) = self.mode {
            command.env("FAKE_SBX_MODE", mode);
        }
        command.output().expect("run omp-sbx")
    }

    fn calls(&self) -> Vec<String> {
        fs::read_to_string(&self.log).expect("read fake log").lines().map(str::to_owned).collect()
    }
}

#[test]
fn starts_stopped_daemon_and_reuses_it() {
    let fixture = Fixture::new(None);

    let first = fixture.run();
    assert!(first.status.success(), "first run failed: {}", String::from_utf8_lossy(&first.stderr));
    assert!(String::from_utf8_lossy(&first.stdout).contains("OMP_LAUNCHED"));
    assert!(fixture.running.is_file(), "daemon was not started");
    assert_eq!(fixture.calls().iter().filter(|call| *call == "daemon start --detach").count(), 1);

    let second = fixture.run();
    assert!(second.status.success(), "second run failed: {}", String::from_utf8_lossy(&second.stderr));
    assert!(String::from_utf8_lossy(&second.stdout).contains("OMP_LAUNCHED"));
    assert_eq!(fixture.calls().iter().filter(|call| *call == "daemon start --detach").count(), 1);
}

#[test]
fn reports_daemon_start_failure_before_sandbox_work() {
    let fixture = Fixture::new(Some("fail-start"));

    let output = fixture.run();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("start sbx daemon"), "missing startup context: {stderr}");
    assert!(stderr.contains("cannot start daemon"), "missing startup diagnostic: {stderr}");
    let calls = fixture.calls();
    assert!(calls.contains(&"daemon status".to_owned()));
    assert!(calls.contains(&"daemon start --detach".to_owned()));
    assert!(!calls.iter().any(|call| call.starts_with("version ")));
    assert!(!calls.iter().any(|call| call.starts_with("create ")));
}

#[test]
fn rejects_unknown_daemon_status_before_starting() {
    let fixture = Fixture::new(Some("unknown-status"));

    let output = fixture.run();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unrecognized sbx daemon status output"), "missing status diagnostic: {stderr}");
    let calls = fixture.calls();
    assert_eq!(calls, vec!["daemon status"]);
}

const FAKE_SBX: &str = r####"#!/bin/sh
set -eu

printf '%s\n' "$*" >> "$FAKE_SBX_LOG"

case "${1-} ${2-}" in
  "daemon status")
    if [ "${FAKE_SBX_MODE-}" = "unknown-status" ]; then
      printf '%s\n' "Status: paused"
    elif [ -f "$FAKE_SBX_RUNNING" ]; then
      printf '%s\n' "Status: running"
    else
      printf '%s\n' "Status: stopped"
    fi
    ;;
  "daemon start")
    if [ "${FAKE_SBX_MODE-}" = "fail-start" ]; then
      printf '%s\n' "cannot start daemon" >&2
      exit 7
    fi
    touch "$FAKE_SBX_RUNNING"
    ;;
  "version --json")
    if [ ! -f "$FAKE_SBX_RUNNING" ]; then
      printf '%s\n' "daemon is not running" >&2
      exit 1
    fi
    printf '%s\n' '{"client":{"version":"v0.43.0"}}'
    ;;
  "ls --json")
    if [ -f "$FAKE_SBX_CREATED" ]; then
      printf '{"sandboxes":[{"name":"omp-project","status":"stopped","workspaces":["%s"]}]}\n' "$FAKE_SBX_STATE/omp-sbx/sandboxes/omp-project/.omp"
    else
      printf '%s\n' '{"sandboxes":[]}'
    fi
    ;;
  "create "*)
    touch "$FAKE_SBX_CREATED"
    ;;
  "exec "*)
    if [ -f "$FAKE_SBX_RUNNING" ] && [ -f "$FAKE_SBX_CREATED" ]; then
      printf '%s\n' "OMP_LAUNCHED"
    else
      printf '%s\n' "sandbox is unavailable" >&2
      exit 1
    fi
    ;;
  *)
    printf 'unsupported fake sbx command: %s\n' "$*" >&2
    exit 1
    ;;
esac
"####;

