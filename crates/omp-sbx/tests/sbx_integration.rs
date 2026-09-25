use std::{env, fs, process::Command};
use tempfile::TempDir;

fn available(program: &str) -> bool {
    let Some(path) = env::var_os("PATH") else { return false; };
    env::split_paths(&path).collect::<Vec<_>>().into_iter().any(|dir| {
        let candidate = dir.join(program);
        candidate.is_file() && {
            #[cfg(unix)] { std::os::unix::fs::PermissionsExt::mode(&candidate.metadata().unwrap().permissions()) & 0o111 != 0 }
            #[cfg(not(unix))] { true }
        }
    })
}

fn integration_enabled() -> bool { env::var_os("OMP_SBX_RUN_INTEGRATION").is_some() }

#[test]
fn native_configure_dry_run_works_with_sbx() {
    if !available("sbx") || !available("docker") || !integration_enabled() { return; }
    let home = TempDir::new().expect("temporary HOME");
    fs::create_dir_all(home.path().join(".omp/agent")).expect("agent directory");
    let binary = env!("CARGO_BIN_EXE_omp-sbx");
    let output = Command::new(binary)
        .env("HOME", home.path())
        .args(["configure", "--fast", "openai-codex/gpt-5.6-luna", "--standard", "openai-codex/gpt-5.6-sol", "--deep", "openai-codex/gpt-6-astra", "--dry-run"])
        .output()
        .expect("run native configure");
    assert!(output.status.success(), "configure failed: {}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn native_configure_does_not_publish_dry_run_changes() {
    if !available("sbx") || !available("docker") || !integration_enabled() { return; }
    let home = TempDir::new().expect("temporary HOME");
    let agent = home.path().join(".omp/agent");
    fs::create_dir_all(&agent).expect("agent directory");
    let config = agent.join("config.yml");
    fs::write(&config, "unchanged\n").expect("seed config");
    let binary = env!("CARGO_BIN_EXE_omp-sbx");
    let output = Command::new(binary)
        .env("HOME", home.path())
        .args(["configure", "--fast", "openai-codex/gpt-5.6-luna", "--standard", "openai-codex/gpt-5.6-sol", "--deep", "openai-codex/gpt-6-astra", "--dry-run"])
        .output()
        .expect("run native configure");
    assert!(output.status.success(), "configure failed: {}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(fs::read_to_string(config).expect("read config"), "unchanged\n");
}

#[test]
fn native_build_requires_opt_in_runtime() {
    if !available("sbx") || !available("docker") || !integration_enabled() { return; }
    let binary = env!("CARGO_BIN_EXE_omp-sbx");
    let output = Command::new(binary).arg("build-configure").output().expect("run native build");
    assert!(output.status.success(), "build-configure failed: {}", String::from_utf8_lossy(&output.stderr));
}

