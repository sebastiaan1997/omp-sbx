pub mod configure;
pub mod assets;
pub mod cli;
pub mod commands;
pub mod paths;
pub mod policy;
pub mod preflight;
pub mod process;
pub mod sbx;
pub mod state;
pub mod terminal;

use anyhow::Result;
use cli::{Cli, Command};

pub fn dispatch(cli: Cli) -> Result<()> {
    process::set_debug(cli.debug);
    match cli.command {
        Command::Run(args) => commands::run::execute(args),
        Command::Parallel(args) => commands::parallel::execute(args),
        Command::Env(args) => commands::env::execute(args),
        Command::Configure(args) => commands::configure::execute(args),
        Command::McpImport(args) => commands::mcp_import::execute(args),
        Command::Build => commands::build::execute(false),
        Command::BuildConfigure => commands::build::execute(true),
        Command::McpStdio(args) => commands::mcp_stdio::execute(args),
    }
}
