//! Detect and run latexmk / tectonic.

use crate::cli::Engine;
use crate::config::Compiler;
use crate::error::{OlfError, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Latexmk,
    Tectonic,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Latexmk => "latexmk",
            Self::Tectonic => "tectonic",
        }
    }

    fn install_hint(self) -> &'static str {
        match self {
            Self::Latexmk => {
                "install a TeX distribution with latexmk: TeX Live (https://tug.org/texlive/), \
                 MacTeX on macOS (`brew install --cask mactex-no-gui`), or `apt install latexmk`"
            }
            Self::Tectonic => "install tectonic: https://tectonic-typesetting.github.io",
        }
    }
}

#[derive(Debug)]
pub struct Resolved {
    pub kind: Kind,
    pub program: PathBuf,
}

/// Pick an engine available on PATH. `auto` prefers latexmk, which is what
/// Overleaf itself runs.
pub fn select(engine: Engine) -> Result<Resolved> {
    let candidates: &[Kind] = match engine {
        Engine::Auto => &[Kind::Latexmk, Kind::Tectonic],
        Engine::Latexmk => &[Kind::Latexmk],
        Engine::Tectonic => &[Kind::Tectonic],
    };
    for &kind in candidates {
        if let Ok(program) = which::which(kind.name()) {
            return Ok(Resolved { kind, program });
        }
    }
    Err(match engine {
        Engine::Auto => OlfError::MissingTool {
            tool: "latexmk or tectonic".into(),
            hint: format!(
                "{}\n      or {}",
                Kind::Latexmk.install_hint(),
                Kind::Tectonic.install_hint()
            ),
        },
        _ => OlfError::MissingTool {
            tool: candidates[0].name().into(),
            hint: candidates[0].install_hint().into(),
        },
    })
}

/// Everything needed to compile one document.
pub struct Job<'a> {
    pub checkout: &'a Path,
    pub main: &'a Path,
    pub out_dir: &'a Path,
    pub compiler: Compiler,
}

/// Directory of the main file, relative to the checkout. Overleaf compiles
/// from there, so `\input` paths in a `notes/main.tex` are relative to `notes/`.
pub fn main_dir(job: &Job) -> PathBuf {
    job.main.parent().map(Path::to_path_buf).unwrap_or_default()
}

pub fn command(engine: &Resolved, job: &Job) -> Command {
    let main = job
        .main
        .file_name()
        .map_or_else(|| job.main.as_os_str(), |n| n);
    let mut cmd = Command::new(&engine.program);
    cmd.current_dir(job.checkout.join(main_dir(job)))
        .stdin(Stdio::null())
        // Unwrapped log lines keep messages and file paths parseable.
        .env("max_print_line", "10000")
        .env("error_line", "254")
        .env("half_error_line", "238");
    match engine.kind {
        Kind::Latexmk => {
            let mode = match job.compiler {
                Compiler::Pdflatex => "-pdf",
                Compiler::Xelatex => "-xelatex",
                Compiler::Lualatex => "-lualatex",
            };
            cmd.args([
                mode,
                "-interaction=nonstopmode",
                "-file-line-error",
                "-halt-on-error",
            ])
            .arg(format!("-outdir={}", job.out_dir.display()))
            .arg(main);
        }
        Kind::Tectonic => {
            cmd.args(["-X", "compile"])
                .arg(main)
                .arg("--outdir")
                .arg(job.out_dir)
                .arg("--keep-logs");
        }
    }
    cmd
}

/// Where the engine writes `<stem>.<ext>` for the main file.
pub fn output_path(job: &Job, ext: &str) -> PathBuf {
    let stem = job.main.file_stem().unwrap_or_default().to_string_lossy();
    job.out_dir.join(format!("{stem}.{ext}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(main: &Path, compiler: Compiler) -> Job<'_> {
        Job {
            checkout: Path::new("/ws/paper"),
            main,
            out_dir: Path::new("/ws/.olf/build"),
            compiler,
        }
    }

    fn args(cmd: &Command) -> Vec<String> {
        cmd.get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn latexmk_arguments_follow_compiler() {
        let engine = Resolved {
            kind: Kind::Latexmk,
            program: "latexmk".into(),
        };
        let cmd = command(&engine, &job(Path::new("main.tex"), Compiler::Xelatex));
        assert_eq!(
            args(&cmd),
            [
                "-xelatex",
                "-interaction=nonstopmode",
                "-file-line-error",
                "-halt-on-error",
                "-outdir=/ws/.olf/build",
                "main.tex"
            ]
        );
        assert_eq!(cmd.get_current_dir(), Some(Path::new("/ws/paper")));
    }

    #[test]
    fn runs_from_the_main_file_directory() {
        let engine = Resolved {
            kind: Kind::Latexmk,
            program: "latexmk".into(),
        };
        let cmd = command(
            &engine,
            &job(Path::new("notes/main.tex"), Compiler::Pdflatex),
        );
        assert_eq!(cmd.get_current_dir(), Some(Path::new("/ws/paper/notes")));
        assert_eq!(args(&cmd).last().unwrap(), "main.tex");
    }

    #[test]
    fn tectonic_arguments() {
        let engine = Resolved {
            kind: Kind::Tectonic,
            program: "tectonic".into(),
        };
        let cmd = command(&engine, &job(Path::new("main.tex"), Compiler::Pdflatex));
        assert_eq!(
            args(&cmd),
            [
                "-X",
                "compile",
                "main.tex",
                "--outdir",
                "/ws/.olf/build",
                "--keep-logs"
            ]
        );
    }

    #[test]
    fn outputs_are_named_after_the_main_stem() {
        let main = Path::new("src/paper.tex");
        assert_eq!(
            output_path(&job(main, Compiler::Pdflatex), "pdf"),
            Path::new("/ws/.olf/build/paper.pdf")
        );
    }
}
