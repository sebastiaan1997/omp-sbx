use std::{ffi::OsString, fs, io::Write, path::{Path, PathBuf}, thread, time::Duration};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use tempfile::NamedTempFile;

use crate::{assets, cli::ParallelArgs, paths, policy, preflight, process::{CommandSpec, exit_code}, sbx::{self, Sbx}, state, terminal};

use super::launch;

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = CommandSpec::new("git").arg("-C").arg(root).args(args.iter().copied()).checked_output()?;
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn branch_exists(root: &Path, branch: &str) -> bool {
    CommandSpec::new("git").arg("-C").arg(root).args(["show-ref", "--verify", "--quiet", &format!("refs/heads/{branch}")]).status().is_ok_and(|status| status.success())
}

fn branches(root: &Path, current: &str) -> Result<Vec<String>> {
    let output = git(root, &["for-each-ref", "--format=%(refname:short)", "refs/heads/"])?;
    Ok(output.lines().filter(|branch| *branch != current).map(str::to_owned).collect())
}

fn pick_existing(root: &Path, current: &str) -> Result<String> {
    let branches = branches(root, current)?;
    if branches.is_empty() { bail!("No other local branches. Use --new <name> to create one."); }
    if !terminal::stdin_is_terminal() { bail!("no TTY for interactive picker; use --branch <name>"); }
    let options = branches.clone();
    let choice = terminal::select_menu("Select branch", &options, 0, false)?.context("cancelled")?;
    branches.get(choice).cloned().context("invalid selection")
}

fn select_branch(args: &ParallelArgs, root: &Path, current: &str) -> Result<(String, bool)> {
    if let Some(branch) = &args.new {
        if branch_exists(root, branch) { bail!("branch '{branch}' already exists; use --branch {branch} to use it"); }
        if !CommandSpec::new("git").arg("-C").arg(root).args(["check-ref-format", "--normalize", &format!("refs/heads/{branch}")]).status()?.success() { bail!("invalid branch name: {branch}"); }
        return Ok((branch.clone(), true));
    }
    if let Some(branch) = &args.branch {
        if !branch_exists(root, branch) { bail!("branch '{branch}' not found"); }
        return Ok((branch.clone(), false));
    }
    if !terminal::stdin_is_terminal() { bail!("no TTY; specify --branch <name> or --new <name>"); }
    let choices = vec!["Use existing branch".to_owned(), "Create new branch".to_owned()];
    match terminal::select_menu("omp-sbx parallel", &choices, 0, false)?.context("cancelled")? {
        0 => Ok((pick_existing(root, current)?, false)),
        1 => {
            let branch = terminal::read_line("Enter new branch name: ")?;
            if branch.is_empty() { bail!("cancelled"); }
            Ok((branch, true))
        }
        _ => bail!("cancelled"),
    }
}


fn write_workspace(root: &Path, repo_name: &str, branch: &str, worktree: &Path, add: bool) -> Result<()> {
    let path = root.join(format!("{repo_name}.code-workspace"));
    let mut document: Value = if path.is_file() { serde_json::from_slice(&fs::read(&path)?).context("parse .code-workspace")? } else { json!({"folders": [{"path": ".", "name": repo_name}], "settings": {}}) };
    let folders = document.get_mut("folders").and_then(Value::as_array_mut).context(".code-workspace folders must be an array")?;
    let name = format!("{repo_name} {branch}");
    folders.retain(|entry| entry.get("name").and_then(Value::as_str) != Some(&name));
    if add { folders.push(json!({"path": format!("../{}", worktree.file_name().context("worktree has no name")?.to_string_lossy()), "name": name})); }
    if folders.first().and_then(|entry| entry.get("path")).and_then(Value::as_str) != Some(".") { folders.insert(0, json!({"path": ".", "name": repo_name})); }
    let parent = path.parent().context("workspace file has no parent")?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut temporary, &document)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn cleanup_menu(root: &Path, repo_name: &str, current: &str, branch: &str, worktree: &Path) -> Result<()> {
    let options = vec![
        format!("Merge '{branch}' into '{current}' and remove worktree"),
        "Remove worktree only (keep branch)".to_owned(),
        format!("Remove worktree AND branch '{branch}' (no merge) [destructive]"),
        "Keep worktree as-is".to_owned(),
    ];
    let choice = terminal::select_menu("omp-sbx parallel: session ended", &options, 3, true)?;
    match choice {
        Some(0) => { CommandSpec::new("git").arg("-C").arg(root).args(["merge", branch]).checked()?; CommandSpec::new("git").arg("-C").arg(root).args(["worktree", "remove"]).arg(worktree).checked()?; CommandSpec::new("git").arg("-C").arg(root).args(["branch", "-d", branch]).checked()?; write_workspace(root, repo_name, branch, worktree, false)?; }
        Some(1) => { CommandSpec::new("git").arg("-C").arg(root).args(["worktree", "remove"]).arg(worktree).checked()?; write_workspace(root, repo_name, branch, worktree, false)?; }
        Some(2) => { eprintln!("Type 'yes' to confirm (3s guard before input)..."); thread::sleep(Duration::from_secs(3)); if terminal::read_line("confirm> ")? == "yes" { CommandSpec::new("git").arg("-C").arg(root).args(["worktree", "remove"]).arg(worktree).checked()?; CommandSpec::new("git").arg("-C").arg(root).args(["branch", "-D", branch]).checked()?; write_workspace(root, repo_name, branch, worktree, false)?; } else { eprintln!("aborted: worktree and branch left intact"); } }
        Some(3) => eprintln!("worktree kept at {}", worktree.display()),
        None => {}
        _ => unreachable!(),
    }
    Ok(())
}
pub fn execute(args: ParallelArgs) -> Result<()> {
    preflight::ensure_sbx("omp-sbx parallel")?;
    let sbx = Sbx;
    sbx::ensure_minimum_version(&sbx, sbx::MINIMUM_SBX_VERSION)?;
    if !crate::process::exists("git") { bail!("git not found"); }
    let start = paths::workspace()?;
    if git(&start, &["rev-parse", "--is-inside-work-tree"]).is_err() { bail!("not inside a git repository"); }

    let root = PathBuf::from(git(&start, &["rev-parse", "--show-toplevel"])?);
    let repo_name = root.file_name().context("repository has no name")?.to_string_lossy().into_owned();
    let current = git(&root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let (branch, new_branch) = select_branch(&args, &root, &current)?;
    let base = root.to_string_lossy().split('@').next().unwrap_or(&repo_name).to_owned();
    let worktree = PathBuf::from(format!("{}@{}", base, branch.replace('/', "-")));

    #[cfg(windows)]
    let relative_flag = Some("--relative-paths");
    #[cfg(not(windows))]
    let relative_flag: Option<&str> = None;
    if worktree.is_dir() {
        let listed = git(&root, &["worktree", "list", "--porcelain"])?;
        if !listed.lines().any(|line| line == format!("worktree {}", worktree.display())) { bail!("path exists but is not a git worktree: {}", worktree.display()); }
        #[cfg(windows)]
        CommandSpec::new("git").arg("-C").arg(&root).args(["worktree", "repair", "--relative-paths"]).arg(&worktree).checked()?;
    } else {
        let mut command = CommandSpec::new("git").arg("-C").arg(&root).args(["worktree", "add"]);
        if let Some(flag) = relative_flag { command = command.arg(flag); }
        if new_branch { command = command.args(["-b", &branch]).arg(&worktree); } else { command = command.arg(&worktree).arg(&branch); }
        command.checked()?;
    }
    write_workspace(&root, &repo_name, &branch, &worktree, true)?;

    let assets = assets::resolve()?;
    let name = preflight::sandbox_name_for(&format!("{repo_name}-{branch}"));
    let private_state = state::prepare(&name)?;
    let legacy_omp = paths::omp_dir()?;
    if args.refresh_image_policy && sbx.exists(&name).unwrap_or(false) {
        let status = sbx.remove(&name)?;
        if !status.success() { bail!("failed to remove sandbox '{name}': {status}"); }
    }
    let mut exists = sbx.exists(&name).unwrap_or(false);
    if exists && preflight::drop_stale_sandbox(&sbx, &name, &assets.main_kit)? { exists = false; }
    if exists && preflight::drop_incompatible_state_sandbox(&sbx, &name, &private_state.omp_dir, &legacy_omp)? { exists = false; }
    let snapshot = policy::prepare(&worktree, &name, args.refresh_image_policy)?;
    let mounts = vec![OsString::from(paths::sbx_argument(&private_state.omp_dir)), OsString::from(paths::sbx_argument(&root.join(".git"))), OsString::from(format!("{}:ro", paths::sbx_argument(&snapshot.directory)))];
    let template = std::env::var("OMP_SBX_TEMPLATE").unwrap_or_else(|_| "omp-sbx:latest".to_owned());
    let options = vec![OsString::from("--template"), OsString::from(template)];
    let interactive = args.omp_args.is_empty() && terminal::stdin_is_terminal();
    let result = if interactive { launch::interactive(&sbx, &name, &assets.main_kit, &worktree, &mounts, &options, exists, args.yes) } else {
        let _ = launch::create_if_needed(&sbx, &name, &assets.main_kit, &worktree, &mounts, &options, exists);
        let mut command = vec!["/usr/local/bin/omp-init.sh".to_owned()]; command.extend_from_slice(&args.omp_args);
        let status = sbx.exec(&name, Some("/home/agent"), &command)?;
        if !status.success() { Err(anyhow::anyhow!("omp exited with status {}", exit_code(status))) } else { Ok(()) }
    };
    if result.is_ok() && interactive { cleanup_menu(&root, &repo_name, &current, &branch, &worktree)?; }
    let _ = sbx.remove(&name);
    result
}
