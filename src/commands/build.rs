//! `olf build`: compile locally into `.olf/build/` and condense the log.

use crate::cli::BuildArgs;
use crate::config::{Compiler, Workspace};
use crate::error::{OlfError, Result, bail};
use crate::latex::engine::{self, Job, Kind};
use crate::latex::log::{self, Diagnostic};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
struct JsonReport<'a> {
    ok: bool,
    engine: Kind,
    pdf: Option<&'a Path>,
    log: Option<&'a Path>,
    errors: &'a [Diagnostic],
    warnings: &'a [Diagnostic],
}

pub fn run(args: &BuildArgs, json: bool) -> Result<()> {
    let ws = Workspace::discover(&std::env::current_dir()?)?;
    let checkout = ws.checkout_dir();
    let Some(main) = args.main.clone().or_else(|| ws.config.build.main.clone()) else {
        bail!("no main file configured\nhint: set build.main in .olf/config.toml or pass --main");
    };
    if !checkout.join(&main).is_file() {
        bail!(
            "main file {} not found in {}",
            main.display(),
            checkout.display()
        );
    }

    let engine = engine::select(args.engine.unwrap_or(ws.config.build.engine))?;
    let compiler = ws.config.build.compiler;
    if engine.kind == Kind::Tectonic && compiler != Compiler::Xelatex {
        eprintln!(
            "warning: building with tectonic (XeTeX-based) but the project uses {}; \
             results may differ from Overleaf",
            compiler.name()
        );
    }

    let out_dir = ws.build_dir();
    fs::create_dir_all(&out_dir)?;
    let job = Job {
        checkout: &checkout,
        main: &main,
        out_dir: &out_dir,
        compiler,
    };
    let output = engine::command(&engine, &job)
        .output()
        .map_err(|e| OlfError::Error(format!("cannot run {}: {e}", engine.program.display())))?;

    let log_path = engine::output_path(&job, "log");
    let report = fs::read_to_string(&log_path)
        .map(|text| log::parse(&text))
        .unwrap_or_default();
    let log_path = log_path.is_file().then_some(log_path);
    let pdf = engine::output_path(&job, "pdf");
    let ok = output.status.success();
    let pdf = (ok && pdf.is_file()).then_some(pdf);

    if json {
        let json = JsonReport {
            ok,
            engine: engine.kind,
            pdf: pdf.as_deref(),
            log: log_path.as_deref(),
            errors: &report.errors,
            warnings: &report.warnings,
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&json).expect("report serializes")
        );
    } else {
        print_human(&report, args.warnings);
    }

    let log_note = log_path
        .as_ref()
        .map(|p| format!("full log: {}", p.display()));
    if !ok {
        let mut message = format!(
            "build failed ({}, {} error{})",
            engine.kind.name(),
            report.errors.len(),
            if report.errors.len() == 1 { "" } else { "s" }
        );
        if report.errors.is_empty() {
            // Nothing parseable (e.g. biber failed): show what the engine said.
            message.push('\n');
            message.push_str(&tail(&output.stdout, &output.stderr, 20));
        }
        if let Some(note) = log_note {
            message.push('\n');
            message.push_str(&note);
        }
        return Err(OlfError::BuildFailed(message));
    }
    if !json {
        let shown = if args.warnings || report.warnings.is_empty() {
            String::new()
        } else {
            " (--warnings to show)".into()
        };
        println!(
            "built {} with {}, {} warning{}{shown}",
            pdf.as_deref()
                .map_or_else(|| PathBuf::from("?"), Path::to_path_buf)
                .display(),
            engine.kind.name(),
            report.warnings.len(),
            if report.warnings.len() == 1 { "" } else { "s" },
        );
        if let Some(note) = log_note {
            println!("{note}");
        }
    }
    Ok(())
}

fn print_human(report: &log::Report, warnings: bool) {
    for error in &report.errors {
        println!("{error}");
        if let Some(context) = &error.context {
            println!("    {context}");
        }
    }
    if warnings {
        for warning in &report.warnings {
            println!("warning: {warning}");
        }
    }
}

/// The last `n` lines of the engine's combined output.
fn tail(stdout: &[u8], stderr: &[u8], n: usize) -> String {
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(stdout),
        String::from_utf8_lossy(stderr)
    );
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}
