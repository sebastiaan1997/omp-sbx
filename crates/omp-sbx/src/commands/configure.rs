use std::fs;
use anyhow::{bail, Context, Result};
use tempfile::TempDir;
use crate::{assets, configure::{self, ConfigSnapshot}, cli::ConfigureArgs, paths, preflight, sbx::{self, Sbx}};

fn debug_output(enabled: bool, args: &[String], output: &std::process::Output) {
    if !enabled { return; }
    eprintln!("configure external command: sbx exec {}", args.join(" "));
    eprintln!("configure external status: {}", output.status);
    eprintln!("configure external stdout:\n{}", String::from_utf8_lossy(&output.stdout));
    eprintln!("configure external stderr:\n{}", String::from_utf8_lossy(&output.stderr));
}

fn exec_json(sbx: &Sbx, name: &str, agent_dir: &str, command: &[&str], debug: bool) -> Result<serde_json::Value> {
    let mut args = vec!["env".to_owned(), format!("PI_CODING_AGENT_DIR={agent_dir}"), "omp".to_owned()];
    args.extend(command.iter().map(|value| (*value).to_owned()));
    let output = {
        let mut last = None;
        let mut result = None;
        for _ in 0..60 {
            let attempt = sbx.exec_output(name, Some("/home/agent"), &args)?;
            debug_output(debug, &args, &attempt);
            if attempt.status.success() { result = Some(attempt); break; }
            let text = format!("{}{}", String::from_utf8_lossy(&attempt.stdout), String::from_utf8_lossy(&attempt.stderr));
            if !text.to_ascii_lowercase().contains("not found") && !text.to_ascii_lowercase().contains("not running") { last = Some(attempt); break; }
            last = Some(attempt);
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        result.or(last).context("configure sandbox exec produced no result")?
    };
    serde_json::from_slice(&output.stdout).with_context(|| format!("parse configure command output (stdout: {}; stderr: {})", String::from_utf8_lossy(&output.stdout).trim(), String::from_utf8_lossy(&output.stderr).trim()))
}

fn set_config(sbx: &Sbx, name: &str, agent_dir: &str, key: &str, value: &serde_json::Value, debug: bool) -> Result<()> {
    let encoded = if key == "modelRoleStorage" { value.as_str().map(str::to_owned).context("modelRoleStorage value is not a string")? } else { serde_json::to_string(value)? };
    let args = vec!["env".to_owned(), format!("PI_CODING_AGENT_DIR={agent_dir}"), "omp".to_owned(), "config".to_owned(), "set".to_owned(), key.to_owned(), encoded];
    let output = sbx.exec_output(name, Some("/home/agent"), &args)?;
    debug_output(debug, &args, &output);
    if !output.status.success() { bail!("setting {key} failed: {}", String::from_utf8_lossy(&output.stderr).trim()); }
    Ok(())
}


fn interactive_selector(tier: &str, preferred: &str, models: &[configure::Model]) -> Result<String> {
    if !crate::terminal::stdin_is_terminal() { bail!("{tier} selector is required without an interactive terminal"); }
    let options = models.iter().map(|model| format!("{} — {}", model.name, model.selector)).collect::<Vec<_>>();
    let default = models.iter().position(|model| model.selector == preferred).unwrap_or(0);
    let selected = crate::terminal::select_menu(&format!("Select {tier} model"), &options, default, false)?.context("model selection cancelled")?;
    let model = &models[selected];
    let mut selector = model.selector.clone();
    if let Some(levels) = model.thinking.as_ref().filter(|levels| !levels.is_empty()) {
        let mut thinking = vec!["inherit".to_owned(), "off".to_owned()];
        thinking.extend(levels.iter().cloned());
        let level = crate::terminal::select_menu("Select thinking level", &thinking, 0, false)?.context("thinking selection cancelled")?;
        if level == 1 { selector.push_str(":off"); }
        else if level >= 2 { selector.push(':'); selector.push_str(&thinking[level]); }
    }
    Ok(selector)
}

fn optional_selectors(models: &[configure::Model], standard: &str, fast: &str, deep: &str) -> Result<Vec<(&'static str, Option<String>)>> {
    if !crate::terminal::stdin_is_terminal() || !crate::terminal::confirm("Configure optional roles? [y/N] ")? { return Ok(Vec::new()); }
    let roles = [("plan", standard), ("vision", standard), ("designer", standard), ("commit", fast), ("tiny", fast), ("task", standard), ("advisor", deep)];
    let mut selected = Vec::new();
    for (role, preferred) in roles { let value = interactive_selector(role, preferred, models)?; selected.push((role, Some(value))); }
    Ok(selected)
}
pub fn execute(args: ConfigureArgs) -> Result<()> {
    preflight::ensure_sbx("omp-sbx configure")?;
    let sbx = Sbx;
    sbx::ensure_minimum_version(&sbx, sbx::MINIMUM_SBX_VERSION)?;
    let assets = assets::resolve()?;
    let omp_dir = paths::omp_dir()?;
    let workspace = paths::workspace()?;
    let name = format!("omp-configure-models-{}", std::process::id());
    let template = std::env::var("OMP_SBX_CONFIGURE_TEMPLATE").unwrap_or_else(|_| "omp-sbx-configure:latest".to_owned());
    let staging = TempDir::new().context("create staged configuration directory")?;
    let staged_agent = staging.path().join("agent"); fs::create_dir(&staged_agent)?;
    let host_config = omp_dir.join("agent/config.yml");
    let staged_config = staged_agent.join("config.yml");
    if host_config.is_file() { fs::copy(&host_config, &staged_config)?; }
    let mounts = vec![paths::sbx_argument(staging.path()).into()];
    let options = vec!["--template".into(), template.into()];
    let mut created = sbx.create(&name, &assets.configure_kit, &workspace, &mounts, &options)?;
    if !created.status.success() {
        eprintln!("configure template unavailable; building it before retrying");
        crate::commands::build::execute(true)?;
        let _ = sbx.remove(&name);
        created = sbx.create(&name, &assets.configure_kit, &workspace, &mounts, &options)?;
    }
    if !created.status.success() { bail!("failed to create configure sandbox: {}", String::from_utf8_lossy(&created.stderr).trim()); }
    let cleanup = Cleanup { sbx: sbx.clone(), name: name.clone() };
    let mut ready = false;
    for _ in 0..60 { if sbx.is_running(&name).unwrap_or(false) { ready = true; break; } std::thread::sleep(std::time::Duration::from_secs(1)); }
    if !ready { drop(cleanup); bail!("configure sandbox '{name}' was created but never reached running state"); }
    let agent_path = staged_agent.to_string_lossy().into_owned();
    let models = configure::parse_catalog(&exec_json(&sbx, &name, &agent_path, &["models", "--json"], args.debug)?.to_string())?;
    let snapshot = ConfigSnapshot {
        roles: exec_json(&sbx, &name, &agent_path, &["config", "get", "modelRoles", "--json"], args.debug)?.get("value").and_then(|value| value.as_object()).cloned().context("modelRoles is not an object")?,
        agents: exec_json(&sbx, &name, &agent_path, &["config", "get", "task.agentModelOverrides", "--json"], args.debug)?.get("value").and_then(|value| value.as_object()).cloned().context("agentModelOverrides is not an object")?,
        model_role_storage: exec_json(&sbx, &name, &agent_path, &["config", "get", "modelRoleStorage", "--json"], args.debug)?.get("value").and_then(|value| value.as_str()).context("modelRoleStorage is not a string")?.to_owned(),
    };
    let fast_missing = args.fast.is_none();
    let standard_missing = args.standard.is_none();
    let deep_missing = args.deep.is_none();
    let fast_input = args.fast.unwrap_or_else(|| "openai-codex/gpt-5.6-luna".to_owned());
    let standard_input = args.standard.unwrap_or_else(|| "openai-codex/gpt-5.6-sol".to_owned());
    let deep_input = args.deep.unwrap_or_else(|| "openai-codex/gpt-6-astra".to_owned());
    let fast_input = if fast_missing { interactive_selector("fast", &fast_input, &models)? } else { fast_input };
    let standard_input = if standard_missing { interactive_selector("standard", &standard_input, &models)? } else { standard_input };
    let deep_input = if deep_missing { interactive_selector("deep", &deep_input, &models)? } else { deep_input };
    let fast = configure::normalize_selector(&fast_input, &models)?.0;
    let standard = configure::normalize_selector(&standard_input, &models)?.0;
    let deep = configure::normalize_selector(&deep_input, &models)?.0;
    let optional = optional_selectors(&models, &standard, &fast, &deep)?;
    let optional_refs = optional.iter().map(|(role, selector)| (*role, selector.as_deref())).collect::<Vec<_>>();
    let merged = configure::merged_config(&snapshot, &configure::intended_changes(&snapshot, &fast, &standard, &deep, &optional_refs));
    eprintln!("Resolved model tiers: fast={fast}, standard={standard}, deep={deep}");
    if args.dry_run { drop(cleanup); return Ok(()); }
    set_config(&sbx, &name, &agent_path, "modelRoles", &merged.roles.clone().into(), args.debug)?;
    set_config(&sbx, &name, &agent_path, "task.agentModelOverrides", &merged.agents.clone().into(), args.debug)?;
    set_config(&sbx, &name, &agent_path, "modelRoleStorage", &serde_json::Value::String("project".to_owned()), args.debug)?;
    configure::publish_config(&staged_config, &host_config)?;
    drop(cleanup);
    println!("Global OMP model routing updated.");
    Ok(())
}

struct Cleanup { sbx: Sbx, name: String }
impl Drop for Cleanup { fn drop(&mut self) { let _ = self.sbx.remove(&self.name); } }
