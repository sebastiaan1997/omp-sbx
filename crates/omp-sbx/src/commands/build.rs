use std::{env, fs, path::Path};
use anyhow::{bail, Context, Result};
use tempfile::{NamedTempFile, TempDir};
use crate::{assets, preflight, process::CommandSpec, sbx};

fn build_and_load(kit: &Path, context: &Path, image: &str, version: &str) -> Result<()> {
    eprintln!(">> building {image} (omp v{version})");
    let status = CommandSpec::new("docker").args(["build", "--build-arg", &format!("OMP_VERSION={version}"), "-t", image, "-f"]).arg(kit.join("Dockerfile")).arg(context).status()?;
    if !status.success() { bail!("docker build for {image} exited with {status}"); }
    let archive = NamedTempFile::new().context("create image archive")?;
    let status = CommandSpec::new("docker").args(["image", "save", image, "-o"]).arg(archive.path()).status()?;
    if !status.success() { bail!("docker image save for {image} exited with {status}"); }
    let status = CommandSpec::new("sbx").args(["template", "load"]).arg(archive.path()).status()?;
    if !status.success() { bail!("sbx template load for {image} exited with {status}"); }
    Ok(())
}
fn verify_image(sbx: &crate::sbx::Sbx, name: &str, kit: &Path, image: &str, omp_dir: &Path, workspace: &Path, configure: bool) -> Result<()> {
    // Smoke sandboxes use fixed names. Remove an orphan left by an interrupted
    // build so the next verification can recreate it with the current image.
    remove_smoke_sandbox(sbx, name)?;
    let mounts = vec![crate::paths::sbx_argument(omp_dir).into()];
    let options = vec![std::ffi::OsString::from("--template"), std::ffi::OsString::from(image)];
    let created = sbx.create(name, kit, workspace, &mounts, &options)?;
    if !created.status.success() { bail!("smoke sandbox {name} failed to start: {}", String::from_utf8_lossy(&created.stderr).trim()); }
    let result = (|| -> Result<()> {
        let version = if configure { vec!["omp".to_owned(), "--version".to_owned()] } else { vec!["/usr/local/bin/omp-init.sh".to_owned(), "--version".to_owned()] };
        let version_output = sbx.exec_output(name, Some("/home/agent"), &version)?;
        if !version_output.status.success() { bail!("smoke command in {name} exited with {}", version_output.status); }
        if !configure {
            let output = sbx.exec_output(name, Some("/home/agent"), &["omp".to_owned(), "config".to_owned(), "path".to_owned()])?;
            if !output.status.success() { bail!("omp config path smoke command in {name} exited with {}", output.status); }
            let actual = String::from_utf8_lossy(&output.stdout);
            if actual.trim() != "/home/agent/.omp/agent" {
                bail!("unexpected OMP agent directory in {name}: {}", actual.trim());
            }
            let startup_output = format!("{}\n{}", String::from_utf8_lossy(&version_output.stdout), String::from_utf8_lossy(&version_output.stderr));
            if startup_output.contains("AWS SSO") || startup_output.contains("share ~/.omp/agent/agent.db") {
                bail!("retired authentication/database warning emitted during smoke startup: {startup_output}");
            }
            let status = sbx.exec(name, Some("/home/agent"), &[
                "sh".to_owned(),
                "-c".to_owned(),
                "test \"$OPENAI_CODEX_OAUTH_TOKEN\" = oai-oat01-proxy-managed && test ! -e /opt/omp-sbx/extensions/aws-sso-nudge.ts && test ! -e \"$HOME/.aws/config\"".to_owned(),
            ])?;
            if !status.success() { bail!("retired AWS files remain or Codex OAuth sentinel is missing in {name}"); }
        }
        if configure {
            let status = sbx.exec(name, Some("/home/agent"), &["sh".to_owned(), "-c".to_owned(), "test ! -S /var/run/docker.sock".to_owned()])?;
            if !status.success() { bail!("configure smoke sandbox exposes a Docker socket"); }
        }
        Ok(())
    })();
    remove_smoke_sandbox(sbx, name)?;
    result
}

fn remove_smoke_sandbox(sbx: &crate::sbx::Sbx, name: &str) -> Result<()> {
    if !sbx.exists(name)? {
        return Ok(());
    }
    let status = sbx.remove(name)?;
    if !status.success() {
        bail!("failed to remove smoke sandbox '{name}': {status}");
    }
    Ok(())
}

pub fn execute(configure_only: bool) -> Result<()> {
    preflight::ensure_sbx(if configure_only { "omp-sbx build-configure" } else { "omp-sbx build" })?;
    let sbx = crate::sbx::Sbx;
    sbx::ensure_minimum_version(&sbx, sbx::MINIMUM_SBX_VERSION)?;
    let assets = assets::resolve()?;
    let version = env::var("OMP_VERSION").unwrap_or_else(|_| "18.1.10".to_owned());
    if version == "latest" { bail!("OMP_VERSION=latest is unsupported; supply an exact version"); }
    let main_image = env::var("OMP_SBX_IMAGE").unwrap_or_else(|_| "omp-sbx:latest".to_owned());
    let configure_image = env::var("OMP_SBX_CONFIGURE_IMAGE").unwrap_or_else(|_| "omp-sbx-configure:latest".to_owned());
    if configure_only {
        build_and_load(&assets.configure_kit, &assets.root.join("sbx-configure-kit"), &configure_image, &version)?;
    } else {
        build_and_load(&assets.main_kit, &assets.root, &main_image, &version)?;
        build_and_load(&assets.configure_kit, &assets.root.join("sbx-configure-kit"), &configure_image, &version)?;
    }
    let state = TempDir::new().context("create smoke-test state directory")?;
    let omp_dir = state.path().join(".omp");
    fs::create_dir(&omp_dir)?;
    let workspace = crate::paths::workspace()?;
    if configure_only {
        verify_image(&sbx, "omp-verify-configure-rust", &assets.configure_kit, &configure_image, &omp_dir, &workspace, true)?;
    } else {
        verify_image(&sbx, "omp-verify-configure-rust", &assets.configure_kit, &configure_image, &omp_dir, &workspace, true)?;
        verify_image(&sbx, "omp-verify-rust", &assets.main_kit, &main_image, &omp_dir, &workspace, false)?;
    }
    Ok(())
}
