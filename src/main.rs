mod agents;
mod cli;
mod commands;
mod config;
mod error;
mod git;
mod index;
mod latex;
mod overleaf;

use clap::Parser;
use cli::{Cli, Command};
use error::{OlfError, Result};
use std::process::ExitCode;

fn run(cli: &Cli) -> Result<()> {
    if let Some(dir) = &cli.dir {
        std::env::set_current_dir(dir)
            .map_err(|e| OlfError::Error(format!("cannot change to {}: {e}", dir.display())))?;
    }
    // Where the user ran olf: `grant` and `exec` act on the repo here, not the workspace.
    let invocation_dir = std::env::current_dir()?;
    if let Some(project) = &cli.project {
        let root = index::resolve(project)?;
        std::env::set_current_dir(&root)
            .map_err(|e| OlfError::Error(format!("cannot change to {}: {e}", root.display())))?;
    }
    match &cli.command {
        Command::Init(args) => commands::init::run(args, &invocation_dir),
        Command::Build(args) => commands::build::run(args, cli.json),
        Command::List(args) => commands::list::run(args, cli.json),
        Command::Path => commands::path::run(),
        Command::Edit => commands::edit::run(),
        Command::Open(args) => commands::open::run(args),
        Command::Grant(args) => commands::grant::run(args, &invocation_dir),
        Command::Exec(args) => commands::exec::run(args, &invocation_dir),
        Command::Skill(command) => commands::skill::run(command),
    }
}

fn main() -> ExitCode {
    match run(&Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(e.exit() as u8)
        }
    }
}
