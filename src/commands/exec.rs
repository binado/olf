//! `olf exec`: launch an agent in the current directory with checkout access.

use crate::agents;
use crate::cli::ExecArgs;
use crate::config::Workspace;
use crate::error::{OlfError, Result};
use std::path::Path;
use std::process::Command;

pub fn run(args: &ExecArgs, invocation_dir: &Path) -> Result<()> {
    let ws = Workspace::discover(&std::env::current_dir()?)?;
    let checkout = ws.checkout_dir();
    let (program, rest) = args.command.split_first().expect("clap requires a command");
    if agents::exec_args(program, &checkout).is_none() {
        eprintln!(
            "warning: olf doesn't know how to grant {program} access; \
             only OLF_WORKSPACE/OLF_PROJECT_DIR are set"
        );
    }
    launch(command(program, rest, &ws, invocation_dir), program)
}

fn command(program: &str, rest: &[String], ws: &Workspace, invocation_dir: &Path) -> Command {
    let checkout = ws.checkout_dir();
    let mut cmd = Command::new(program);
    if let Some(flags) = agents::exec_args(program, &checkout) {
        cmd.args(flags);
    }
    cmd.args(rest)
        .current_dir(invocation_dir)
        .env("OLF_WORKSPACE", &ws.root)
        .env("OLF_PROJECT_DIR", &checkout);
    cmd
}

/// Replace the process so the TTY and signals behave as if the agent ran directly.
#[cfg(unix)]
fn launch(mut cmd: Command, program: &str) -> Result<()> {
    use std::os::unix::process::CommandExt;
    let e = cmd.exec();
    Err(spawn_error(program, &e))
}

#[cfg(not(unix))]
fn launch(mut cmd: Command, program: &str) -> Result<()> {
    let status = cmd.status().map_err(|e| spawn_error(program, &e))?;
    std::process::exit(status.code().unwrap_or(1));
}

fn spawn_error(program: &str, e: &std::io::Error) -> OlfError {
    if e.kind() == std::io::ErrorKind::NotFound {
        OlfError::MissingTool {
            tool: program.to_owned(),
            hint: format!("check that {program} is installed and on PATH"),
        }
    } else {
        OlfError::Error(format!("cannot run {program}: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::ffi::OsStr;

    fn ws() -> Workspace {
        Workspace {
            root: "/ws".into(),
            config: Config::new("64f0c0ffee0123456789abcd".into(), "paper".into()),
        }
    }

    #[test]
    fn known_agent_gets_flags_before_user_args() {
        let cmd = command("claude", &["--resume".into()], &ws(), Path::new("/code"));
        let args: Vec<_> = cmd.get_args().collect();
        assert_eq!(args, ["--add-dir", "/ws/paper", "--resume"]);
        assert_eq!(cmd.get_current_dir(), Some(Path::new("/code")));
        let env: Vec<_> = cmd.get_envs().collect();
        assert!(env.contains(&(OsStr::new("OLF_WORKSPACE"), Some(OsStr::new("/ws")))));
        assert!(env.contains(&(OsStr::new("OLF_PROJECT_DIR"), Some(OsStr::new("/ws/paper")))));
    }

    #[test]
    fn unknown_agent_gets_only_env() {
        let cmd = command(
            "sh",
            &["-c".into(), "true".into()],
            &ws(),
            Path::new("/code"),
        );
        let args: Vec<_> = cmd.get_args().collect();
        assert_eq!(args, ["-c", "true"]);
    }
}
