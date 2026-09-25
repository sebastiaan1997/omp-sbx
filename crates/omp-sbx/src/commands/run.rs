use std::{ffi::OsString, path::{Path, PathBuf}};

use anyhow::{Context, Result};

use crate::{assets, cli::RunArgs, paths, policy, preflight, process::CommandSpec, sbx::{self, Sbx}, state, terminal};

use super::launch;

fn git_output(workspace: &Path, args: &[&str]) -> Result<String> {
    let output = CommandSpec::new("git").arg("-C").arg(workspace).args(args.iter().copied()).checked_output()?;
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn linked_worktree_root(workspace: &Path) -> Result<Option<PathBuf>> {
    if !workspace.join(".git").is_file() { return Ok(None); }
    #[cfg(windows)]
    {
        let version = git_output(workspace, &["--version"])?;
        let numeric = version.split_whitespace().last().context("parse git version")?;
        let mut parts = numeric.split('.').filter_map(|part| part.parse::<u64>().ok());
        let installed = (parts.next().unwrap_or(0), parts.next().unwrap_or(0));
        if installed < (2, 48) { bail!("Git 2.48 or newer is required for Windows linked worktrees"); }
        CommandSpec::new("git").args(["worktree", "repair", "--relative-paths"]).arg(workspace).checked()?;
    }
    let common = PathBuf::from(git_output(workspace, &["rev-parse", "--git-common-dir"])?);
    let common = if common.is_absolute() { common } else { workspace.join(common) };
    Ok(common.parent().and_then(Path::parent).map(Path::to_path_buf).filter(|root| root != workspace && root.is_dir()))
}

pub fn execute(args: RunArgs) -> Result<()> {
    preflight::ensure_sbx("omp-sbx")?;
    let sbx = Sbx;
    sbx::ensure_minimum_version(&sbx, sbx::MINIMUM_SBX_VERSION)?;
    let workspace = paths::workspace()?;
    let assets = assets::resolve()?;
    let label = workspace.file_name().context("workspace has no directory name")?.to_string_lossy();
    let name = preflight::sandbox_name_for(&label);
    let private_state = state::prepare(&name)?;
    let legacy_omp = paths::omp_dir()?;

    if (args.new || args.refresh_image_policy) && sbx.exists(&name).unwrap_or(false) { let _ = sbx.remove(&name)?; }
    let mut exists = sbx.exists(&name).unwrap_or(false);
    if exists && preflight::drop_stale_sandbox(&sbx, &name, &assets.main_kit)? { exists = false; }
    if exists && preflight::drop_incompatible_state_sandbox(&sbx, &name, &private_state.omp_dir, &legacy_omp)? { exists = false; }
    let snapshot = policy::prepare(&workspace, &name, args.refresh_image_policy)?;

    let mut mounts = vec![OsString::from(paths::sbx_argument(&private_state.omp_dir))];
    if let Some(gh) = paths::gh_config()? { mounts.push(OsString::from(paths::sbx_argument(&gh))); }
    if let Some(skills) = paths::skills_dir()? { mounts.push(OsString::from(format!("{}:ro", paths::sbx_argument(&skills)))); }
    let main_repo = linked_worktree_root(&workspace)?;
    if let Some(root) = &main_repo { mounts.push(OsString::from(paths::sbx_argument(root))); }
    mounts.push(OsString::from(format!("{}:ro", paths::sbx_argument(&snapshot.directory))));

    let template = std::env::var("OMP_SBX_TEMPLATE").unwrap_or_else(|_| "omp-sbx:latest".to_owned());
    let mut options = vec![OsString::from("--template"), OsString::from(&template)];
    if let Ok(mcp) = std::env::var("OMP_SBX_STATIC_MCP") {
        if !mcp.is_empty() { options.extend([OsString::from("--static-mcp"), OsString::from(mcp)]); }
    }

    eprintln!("omp-sbx\n  template  : {template}\n  workspace : {} -> /home/agent/workspace\n  images    : {} (snapshot: {})\n  state     : {} (private to {})\n  sandbox   : {} ({})\n  mode      : {}",
        workspace.display(), snapshot.origin, snapshot.directory.display(), private_state.omp_dir.display(), name, name,
        if exists { "resuming" } else { "new" },
        if args.omp_args.is_empty() { "interactive TUI".to_owned() } else { format!("omp {}", args.omp_args.join(" ")) });

    if args.omp_args.is_empty() && terminal::stdin_is_terminal() {
        launch::interactive(&sbx, &name, &assets.main_kit, &workspace, &mounts, &options, exists, args.yes)
    } else {
        launch::one_shot(&sbx, &name, &assets.main_kit, &workspace, &mounts, &options, exists, &args.omp_args)
    }
}
