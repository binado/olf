//! `olf fmt`: run tex-fmt over the files you changed (opt-in per project).

use crate::cli::FmtArgs;
use crate::config::{FmtConfig, Workspace};
use crate::error::{OlfError, Result, bail};
use crate::git;
use std::path::{Path, PathBuf};
use std::process::Command;

const INSTALL_HINT: &str = "cargo install tex-fmt, brew install tex-fmt, or \
                            https://github.com/WGUNDERWOOD/tex-fmt";

pub fn run(args: &FmtArgs) -> Result<()> {
    let ws = Workspace::discover(&std::env::current_dir()?)?;
    if !ws.config.fmt.enabled {
        bail!(
            "formatting is disabled for this project\n\
             Reformatting whole files in a shared Overleaf project bloats its history and \
             conflicts with co-authors' edits, so `olf fmt` is opt-in.\n\
             hint: set `enabled = true` under [fmt] in {} to turn it on",
            Workspace::config_path(&ws.root).display()
        );
    }
    let program = which::which("tex-fmt").map_err(|_| OlfError::MissingTool {
        tool: "tex-fmt".into(),
        hint: INSTALL_HINT.into(),
    })?;

    let checkout = ws.checkout_dir();
    let files = select_files(args, &checkout)?;
    if files.is_empty() {
        println!("no changed .tex files");
        return Ok(());
    }

    let status = command(&program, &checkout, &ws.config.fmt, args.check, &files)
        .status()
        .map_err(|e| OlfError::Error(format!("cannot run tex-fmt: {e}")))?;
    let n = files.len();
    let noun = if n == 1 { "file" } else { "files" };
    match (status.success(), args.check) {
        (true, true) => println!("{n} {noun} already formatted"),
        (true, false) => println!("formatted {n} {noun}"),
        // tex-fmt exits 1 both for unformatted files and for real errors.
        (false, true) => {
            return Err(OlfError::Unformatted(
                "some files are not formatted (run `olf fmt` to fix)".into(),
            ));
        }
        (false, false) => bail!("tex-fmt failed ({status})"),
    }
    Ok(())
}

fn select_files(args: &FmtArgs, checkout: &Path) -> Result<Vec<PathBuf>> {
    if !args.paths.is_empty() {
        let cwd = std::env::current_dir()?;
        let root = checkout.canonicalize()?;
        return args
            .paths
            .iter()
            .map(|path| {
                let abs = std::path::absolute(cwd.join(path))?;
                let inside = abs
                    .canonicalize()
                    .unwrap_or_else(|_| abs.clone())
                    .starts_with(&root);
                if inside {
                    Ok(abs)
                } else {
                    Err(OlfError::Error(format!(
                        "{} is outside the checkout {}",
                        path.display(),
                        checkout.display()
                    )))
                }
            })
            .collect();
    }
    let files = if args.all {
        let out = git::run(
            checkout,
            &[
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
                "--",
                "*.tex",
            ],
        )?;
        out.split('\0')
            .filter(|f| !f.is_empty())
            .map(PathBuf::from)
            .collect()
    } else {
        git::changed_files(checkout)?
    };
    let mut files: Vec<PathBuf> = files
        .into_iter()
        .filter(|f| f.extension().is_some_and(|e| e == "tex"))
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
}

pub fn command(
    program: &Path,
    checkout: &Path,
    cfg: &FmtConfig,
    check: bool,
    files: &[PathBuf],
) -> Command {
    let mut cmd = Command::new(program);
    cmd.current_dir(checkout);
    if !checkout.join("tex-fmt.toml").is_file() {
        // Ignore ~/.config/tex-fmt so a co-author's setup can't change the result.
        cmd.arg("--noconfig");
        if !cfg.wrap {
            cmd.arg("--nowrap");
        }
    }
    if check {
        cmd.arg("--check");
    }
    cmd.args(files);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::fs;

    fn args_of(cmd: &Command) -> Vec<&OsStr> {
        cmd.get_args().collect()
    }

    fn files() -> Vec<PathBuf> {
        vec!["a.tex".into(), "sec/b.tex".into()]
    }

    #[test]
    fn defaults_ignore_user_config_and_nowrap() {
        let tmp = tempfile::tempdir().unwrap();
        let cmd = command(
            Path::new("tex-fmt"),
            tmp.path(),
            &FmtConfig::default(),
            false,
            &files(),
        );
        assert_eq!(cmd.get_current_dir(), Some(tmp.path()));
        assert_eq!(
            args_of(&cmd),
            ["--noconfig", "--nowrap", "a.tex", "sec/b.tex"]
        );
    }

    #[test]
    fn wrap_true_drops_nowrap_and_check_is_added() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = FmtConfig {
            enabled: true,
            wrap: true,
        };
        let cmd = command(Path::new("tex-fmt"), tmp.path(), &cfg, true, &files());
        assert_eq!(
            args_of(&cmd),
            ["--noconfig", "--check", "a.tex", "sec/b.tex"]
        );
    }

    #[test]
    fn project_tex_fmt_toml_wins() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("tex-fmt.toml"), "tabsize = 4\n").unwrap();
        let cmd = command(
            Path::new("tex-fmt"),
            tmp.path(),
            &FmtConfig::default(),
            true,
            &files(),
        );
        assert_eq!(args_of(&cmd), ["--check", "a.tex", "sec/b.tex"]);
    }
}
