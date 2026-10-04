mod cli;
mod error;
// Not wired into commands yet; the allow goes away with `init`.
#[allow(dead_code)]
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
        Command::Init(_) | Command::Build(_) | Command::Open(_) | Command::Skill(_) => {
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
