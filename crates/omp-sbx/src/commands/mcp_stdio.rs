use anyhow::{Context, Result, bail};

use crate::{cli::McpStdioArgs, process::CommandSpec};

pub fn execute(args: McpStdioArgs) -> Result<()> {
    let (program, command_args) = args.command.split_first().context("MCP stdio command is empty")?;
    let mut command = CommandSpec::new(program).args(command_args.iter());
    for assignment in args.environment {
        let (name, value) = assignment.split_once('=').with_context(|| format!("invalid environment assignment: {assignment}"))?;
        if name.is_empty() || !name.bytes().enumerate().all(|(index, byte)| byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())) { bail!("invalid environment name: {name}"); }
        command = command.env(name, value);
    }
    command.replace()
}
