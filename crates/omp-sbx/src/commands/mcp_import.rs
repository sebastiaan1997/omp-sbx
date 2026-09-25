use std::{collections::BTreeMap, fs, path::PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::Value;

use crate::{cli::McpImportArgs, paths, preflight, process::CommandSpec};

#[derive(Debug, Deserialize, Default)]
struct Document { #[serde(rename = "mcpServers", default)] servers: BTreeMap<String, Server> }

#[derive(Debug, Clone, Deserialize, Default)]
struct Server {
    #[serde(rename = "type", default)] kind: Option<String>,
    #[serde(default)] url: Option<String>,
    #[serde(default)] command: Option<String>,
    #[serde(default)] args: Vec<String>,
    #[serde(default, alias = "dir")] cwd: Option<String>,
    #[serde(default)] env: BTreeMap<String, Value>,
}

fn read_servers(files: &[PathBuf]) -> BTreeMap<String, Server> {
    let mut servers = BTreeMap::new();
    for path in files {
        if !path.is_file() { continue; }
        match fs::read(path).and_then(|bytes| serde_json::from_slice::<Document>(&bytes).map_err(std::io::Error::other)) {
            Ok(document) => for (name, server) in document.servers { servers.entry(name).or_insert(server); },
            Err(error) => eprintln!("skip\t{}\tunreadable: {error}", path.display()),
        }
    }
    servers
}

fn value_string(value: &Value) -> String {
    value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string())
}

fn redacted(args: &[String]) -> String {
    args.iter().map(|argument| {
        if let Some((name, _)) = argument.split_once('=') {
            if !name.is_empty() && name.bytes().enumerate().all(|(index, byte)| byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())) { return format!("{name}=..."); }
        }
        argument.clone()
    }).collect::<Vec<_>>().join(" ")
}

fn inspect(name: &str) -> bool {
    CommandSpec::new("sbx").args(["mcp", "inspect", name]).status().is_ok_and(|status| status.success())
}

pub fn execute(args: McpImportArgs) -> Result<()> {
    preflight::ensure_sbx("omp-sbx mcp-import")?;
    let workspace = paths::workspace()?;
    let mut files = args.file;
    if files.is_empty() {
        files.push(paths::home()?.join(".claude.json"));
        let project = workspace.join(".mcp.json");
        if project.is_file() { files.push(project); }
    }
    let servers = read_servers(&files);
    if servers.is_empty() {
        eprintln!("no MCP servers found in:");
        for file in files { eprintln!("  {}", file.display()); }
        return Ok(());
    }
    let load_target = if args.load { Some(preflight::sandbox_name_for(&workspace.file_name().context("workspace has no name")?.to_string_lossy())) } else { args.sandbox };
    let executable = paths::executable()?;
    let mut registered = Vec::new();
    let mut oauth = Vec::new();
    let mut skipped = 0_usize;

    eprintln!("Claude MCP servers -> sbx registry\n");
    for (name, server) in servers {
        let kind = server.kind.clone().unwrap_or_else(|| if server.url.as_deref().is_some_and(|value| !value.is_empty()) { "http".to_owned() } else { "stdio".to_owned() });
        if inspect(&name) && !args.force {
            eprintln!("= {name} already registered");
            registered.push(name.clone());
            if matches!(kind.as_str(), "http" | "sse") { oauth.push(name); }
            continue;
        }
        if args.force && !args.dry_run && inspect(&name) { let _ = CommandSpec::new("sbx").args(["mcp", "rm", &name]).status(); }
        let mut add = vec!["mcp".to_owned(), "add".to_owned(), name.clone()];
        match kind.as_str() {
            "http" | "sse" => {
                let Some(url) = server.url.filter(|url| !url.is_empty()) else { eprintln!("! {name} skipped - remote server with no URL"); skipped += 1; continue; };
                add.extend(["--url".to_owned(), url, "--skip_auth".to_owned()]);
                oauth.push(name.clone());
            }
            "stdio" => {
                let Some(program) = server.command.filter(|command| !command.is_empty()) else { eprintln!("! {name} skipped - stdio server with no command"); skipped += 1; continue; };
                if server.env.is_empty() {
                    add.extend(["--command".to_owned(), program]);
                } else {
                    add.extend(["--command".to_owned(), executable.to_string_lossy().into_owned(), "--args".to_owned(), "__mcp-stdio".to_owned()]);
                    for (key, value) in &server.env { add.extend(["--args".to_owned(), "--env".to_owned(), "--args".to_owned(), format!("{key}={}", value_string(value))]); }
                    add.extend(["--args".to_owned(), "--".to_owned(), "--args".to_owned(), program]);
                }
                for argument in server.args { add.extend(["--args".to_owned(), argument]); }
                if let Some(cwd) = server.cwd.filter(|cwd| !cwd.is_empty()) { add.extend(["--dir".to_owned(), cwd]); }
            }
            unsupported => { eprintln!("! {name} skipped - unsupported type '{unsupported}'"); skipped += 1; continue; }
        }
        if args.dry_run {
            eprintln!("+ sbx {}", redacted(&add));
            registered.push(name);
            continue;
        }
        let status = CommandSpec::new("sbx").args(add.iter()).status()?;
        if status.success() { eprintln!("+ {name} registered ({kind})"); registered.push(name); } else { eprintln!("x {name} failed to register"); skipped += 1; }
    }

    if args.auth && !args.dry_run {
        for name in &oauth {
            let output = CommandSpec::new("sbx").args(["mcp", "auth", "status", name]).output()?;
            let status = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
            if !status.contains("unauthorized") && status.contains("authorized") { eprintln!("= {name} already authorized"); continue; }
            if !CommandSpec::new("sbx").args(["mcp", "auth", name]).status()?.success() { eprintln!("! {name} not authorized"); }
        }
    }
    if let Some(sandbox) = &load_target {
        if !args.dry_run {
            for name in &registered {
                if CommandSpec::new("sbx").args(["mcp", "load", name, "--sandbox", sandbox]).status()?.success() { eprintln!("> {name} attached"); } else { eprintln!("x {name} failed to attach"); }
            }
        }
    }
    if registered.is_empty() { bail!("nothing registered"); }
    eprintln!("Registered: {}", registered.join(","));
    if skipped > 0 { eprintln!("Skipped: {skipped}"); }
    if !args.auth && !oauth.is_empty() { eprintln!("Remote servers still need authorizing. Re-run with --auth, or:\n  sbx mcp auth --all"); }
    if load_target.is_none() { eprintln!("They reach no sandbox yet. Attach them with 'omp-sbx mcp-import --load' or set OMP_SBX_STATIC_MCP={} before 'omp-sbx run --new'.", registered.join(",")); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::redacted;
    #[test]
    fn dry_run_redacts_environment_values() {
        assert_eq!(redacted(&["TOKEN=secret".to_owned(), "arg".to_owned()]), "TOKEN=... arg");
    }
}
