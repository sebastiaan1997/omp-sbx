use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "omp-sbx", version, about = "Run Oh My Pi in Docker Sandboxes")]
pub struct Cli {
    #[arg(long, global = true, help = "Print external command output for debugging")]
    pub debug: bool,
    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    fn compat_args(mut args: Vec<std::ffi::OsString>) -> Vec<std::ffi::OsString> {
        if args.len() == 1 {
            args.push("run".into());
            return args;
        }
        let first = args.get(1).and_then(|value| value.to_str()).unwrap_or("");
        let commands = ["run", "parallel", "env", "configure", "mcp-import", "build", "build-configure", "__mcp-stdio", "repair-agent-db", "aws-login", "help"];
        if first == "--configure" {
            args[1] = "configure".into();
        } else if first == "--version" || first == "--new" || first == "--refresh-image-policy" || first == "--yes" || (!commands.contains(&first) && first != "--help" && !first.is_empty()) {
            let direct = args.drain(1..).collect::<Vec<_>>();
            args.push("run".into());
            if direct.iter().any(|value| value == "--version") { args.push("--".into()); }
            args.extend(direct);
        }
        args
    }
    pub fn parse_compat() -> Self { <Self as clap::Parser>::parse_from(Self::compat_args(std::env::args_os().collect())) }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create or resume the persistent sandbox for this directory.
    Run(RunArgs),
    /// Work in a disposable sandbox backed by a Git worktree.
    Parallel(ParallelArgs),
    /// Run through the experimental declarative sbx environment.
    Env(EnvArgs),
    /// Configure OMP model roles using an isolated capture sandbox.
    Configure(ConfigureArgs),
    /// Import Claude MCP servers into Docker Sandboxes.
    McpImport(McpImportArgs),
    /// Build, load, and verify both sandbox images.
    Build,
    /// Build and load only the model-configuration image.
    BuildConfigure,
    #[command(name = "__mcp-stdio", hide = true)]
    McpStdio(McpStdioArgs),
}


#[derive(Debug, Args)]
pub struct RunArgs {
    #[arg(long)]
    pub new: bool,
    #[arg(long)]
    pub refresh_image_policy: bool,
    #[arg(long)]
    pub yes: bool,
    #[arg(last = true, value_name = "OMP_ARGS")]
    pub omp_args: Vec<String>,
}

#[derive(Debug, Args)]
pub struct ParallelArgs {
    #[arg(long, value_name = "NAME", conflicts_with = "new")]
    pub branch: Option<String>,
    #[arg(long, value_name = "NAME", conflicts_with = "branch")]
    pub new: Option<String>,
    #[arg(long)]
    pub refresh_image_policy: bool,
    #[arg(long)]
    pub yes: bool,
    #[arg(last = true, value_name = "OMP_ARGS")]
    pub omp_args: Vec<String>,
}

#[derive(Debug, Args)]
pub struct EnvArgs {
    #[arg(long)]
    pub new: bool,
    #[arg(long)]
    pub refresh_image_policy: bool,
    #[arg(last = true, value_name = "OMP_ARGS")]
    pub omp_args: Vec<String>,
}

#[derive(Debug, Args)]
pub struct ConfigureArgs {
    #[arg(long, value_name = "SELECTOR")]
    pub fast: Option<String>,
    #[arg(long, value_name = "SELECTOR")]
    pub standard: Option<String>,
    #[arg(long, value_name = "SELECTOR")]
    pub deep: Option<String>,
    #[arg(long, help = "Print sandbox commands and their stdout/stderr")]
    pub debug: bool,
    #[arg(long)]
    pub dry_run: bool,
}


#[derive(Debug, Args)]
pub struct McpImportArgs {
    #[arg(long)]
    pub auth: bool,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub force: bool,
    #[arg(long, conflicts_with = "sandbox")]
    pub load: bool,
    #[arg(long, value_name = "NAME", conflicts_with = "load")]
    pub sandbox: Option<String>,
    #[arg(long, value_name = "PATH")]
    pub file: Vec<PathBuf>,
}

#[derive(Debug, Args)]
pub struct McpStdioArgs {
    #[arg(long = "env", value_name = "NAME=VALUE")]
    pub environment: Vec<String>,
    #[arg(last = true, required = true, value_name = "COMMAND")]
    pub command: Vec<String>,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, Command};

    #[test]
    fn bare_invocation_maps_to_run() {
        let args = Cli::compat_args(["omp-sbx"].into_iter().map(Into::into).collect());
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(matches!(
            cli.command,
            Command::Run(args)
                if !args.new
                    && !args.refresh_image_policy
                    && !args.yes
                    && args.omp_args.is_empty()
        ));
    }

    #[test]
    fn forwarded_flags_require_separator() {
        assert!(Cli::try_parse_from(["omp-sbx", "run", "--version"]).is_err());
        let cli = Cli::try_parse_from(["omp-sbx", "run", "--", "--version"]).unwrap();
        assert!(matches!(cli.command, Command::Run(args) if args.omp_args == ["--version"]));
    }

    #[test]
    fn direct_launcher_flags_map_to_run() {
        let args = Cli::compat_args(["omp-sbx", "--new", "--", "prompt"].into_iter().map(Into::into).collect());
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(matches!(cli.command, Command::Run(args) if args.new && args.omp_args == ["prompt"]));
    }

    #[test]
    fn parallel_modes_conflict() {
        let result = Cli::try_parse_from([
            "omp-sbx", "parallel", "--branch", "main", "--new", "other",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn mcp_targets_conflict() {
        let result = Cli::try_parse_from([
            "omp-sbx", "mcp-import", "--load", "--sandbox", "named",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn retired_commands_are_not_treated_as_prompts() {
        for command in ["repair-agent-db", "aws-login"] {
            let args = Cli::compat_args(["omp-sbx", command].into_iter().map(Into::into).collect());
            assert!(Cli::try_parse_from(args).is_err());
        }
    }
}
