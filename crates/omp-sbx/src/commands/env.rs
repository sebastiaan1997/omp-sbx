use std::{ffi::OsString, path::Path};

use anyhow::{Context, Result, bail};

use crate::{assets, cli::EnvArgs, paths, policy, preflight, process::CommandSpec, sbx::{self, Sbx}, state, terminal};

fn env_command(subcommand: &str, name: &str, kit: &Path, env_args: &[(String, String)], force: bool) -> CommandSpec {
    let mut command = CommandSpec::new("sbx").args(["env", subcommand]);
    if force {
        command = command.arg("--force");
    }
    command = command.args(["--name", name]);
    for (key, value) in env_args {
        command = command.args([OsString::from("--env-arg"), OsString::from(format!("{key}={value}"))]);
    }
    command.arg(paths::sbx_argument(kit))
}

pub fn execute(args: EnvArgs) -> Result<()> {
    preflight::ensure_sbx("omp-sbx env")?;
    let sbx = Sbx;
    sbx::ensure_minimum_version(&sbx, sbx::MINIMUM_SBX_VERSION)?;
    let workspace = paths::workspace()?;
    let assets = assets::resolve()?;
    let label = workspace.file_name().context("workspace has no directory name")?.to_string_lossy();
    let name = preflight::sandbox_name_for(&label);
    let private_state = state::prepare(&name)?;
    let legacy_omp = paths::omp_dir()?;
    let snapshot = policy::prepare(&workspace, &name, args.refresh_image_policy)?;
    let env_args = vec![
        ("kitDir".to_owned(), paths::sbx_argument(&assets.main_kit)),
        ("workspace".to_owned(), paths::sbx_argument(&workspace)),
        ("ompState".to_owned(), paths::sbx_argument(&private_state.omp_dir)),
        ("dockerPolicy".to_owned(), paths::sbx_argument(&snapshot.directory)),
    ];

    let exists = sbx.exists(&name).unwrap_or(false);
    let incompatible = exists
        && (!sbx.has_workspace(&name, &private_state.omp_dir)?
            || sbx.has_workspace_mount(&name, &legacy_omp)?);
    let stale = exists && preflight::sandbox_kit_is_stale(&sbx, &name, &assets.main_kit)?;
    if exists && (args.new || args.refresh_image_policy || incompatible || stale) {
        if incompatible {
            eprintln!(
                "sandbox '{name}' has incompatible OMP state mounts; recreating it with private state at {}",
                private_state.omp_dir.display()
            );
        }
        let status = env_command("rm", &name, &assets.main_kit, &env_args, true).status()?;
        if !status.success() { bail!("failed to remove sbx environment '{name}': {status}"); }
    }

    if args.omp_args.is_empty() && terminal::stdin_is_terminal() {
        env_command("run", &name, &assets.main_kit, &env_args, false).replace()
    } else {
        let status = env_command("create", &name, &assets.main_kit, &env_args, false).status()?;
        if !status.success() { bail!("failed to create sbx environment '{name}': {status}"); }
        let mut command = env_command("exec", &name, &assets.main_kit, &env_args, false)
            .args(["--", "/usr/local/bin/omp-init.sh"]);
        command.args.extend(args.omp_args.into_iter().map(OsString::from));
        command.replace()
    }
}
