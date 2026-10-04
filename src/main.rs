mod cli;
mod commands;
mod config;
mod error;
mod git;
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
    match &cli.command {
        Command::Init(args) => commands::init::run(args),
        Command::Build(args) => commands::build::run(args, cli.json),
        Command::Open(args) => commands::open::run(args),
        Command::Skill(_) => {
            error::bail!("not implemented yet")
        }
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
