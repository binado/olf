//! `olf build`: compile locally into `.olf/build/`, keeping each run's logs.
//!
//! olf doesn't interpret the TeX log: success is the engine's exit status, and
//! each run's log is kept under `.olf/build/logs/` for the agent to read.

use crate::cli::BuildArgs;
use crate::config::{Compiler, Workspace};
use crate::error::{OlfError, Result, bail};
use crate::latex::engine::{self, Job, Kind};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Builds whose logs are kept; older ones are deleted.
const KEEP_BUILDS: usize = 20;

#[derive(Serialize)]
struct JsonReport<'a> {
    ok: bool,
    engine: Kind,
    pdf: Option<&'a Path>,
    /// The TeX log of this run (absent if the engine never got that far).
    log: Option<&'a Path>,
    /// The engine's console output (latexmk, biber, ...).
    output: &'a Path,
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
    let ok = output.status.success();

    let logs = save_logs(&job, &output.stdout, &output.stderr)?;
    prune_logs(&out_dir.join("logs"), KEEP_BUILDS)?;
    let pdf = engine::output_path(&job, "pdf");
    // A failed run may leave the previous PDF behind; never report that one.
    let pdf = (ok && pdf.is_file()).then_some(pdf);

    if json {
        let report = JsonReport {
            ok,
            engine: engine.kind,
            pdf: pdf.as_deref(),
            log: logs.tex.as_deref(),
            output: &logs.output,
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("report serializes")
        );
    }

    let log_line = logs
        .tex
        .as_ref()
        .map(|log| format!("\nlog: {}", log.display()))
        .unwrap_or_default();
    let paths = format!("{log_line}\nengine output: {}", logs.output.display());
    if !ok {
        let status = output
            .status
            .code()
            .map_or_else(|| "killed".into(), |c| format!("exit {c}"));
        return Err(OlfError::BuildFailed(format!(
            "build failed ({}, {status}){paths}",
            engine.kind.name()
        )));
    }
    if !json {
        let pdf = pdf.map_or_else(|| "no PDF produced".into(), |p| p.display().to_string());
        println!("built {pdf} with {}{paths}", engine.kind.name());
    }
    Ok(())
}

struct SavedLogs {
    tex: Option<PathBuf>,
    output: PathBuf,
}

/// Copy this run's TeX log and console output to `logs/<timestamp>-<stem>.*`.
fn save_logs(job: &Job, stdout: &[u8], stderr: &[u8]) -> Result<SavedLogs> {
    let dir = job.out_dir.join("logs");
    fs::create_dir_all(&dir)?;
    let stem = job
        .main
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let stamp = timestamp(SystemTime::now());
    // Two builds within a second get distinct names; `.` sorts after `-`, so
    // lexical order stays chronological.
    let mut base = format!("{stamp}-{stem}");
    let mut n = 1;
    while dir.join(format!("{base}.out")).exists() {
        n += 1;
        base = format!("{stamp}.{n}-{stem}");
    }

    let output = dir.join(format!("{base}.out"));
    fs::write(&output, [stdout, stderr].concat())?;
    let tex_log = engine::output_path(job, "log");
    let tex = if tex_log.is_file() {
        let saved = dir.join(format!("{base}.log"));
        fs::copy(&tex_log, &saved)?;
        Some(saved)
    } else {
        None
    };
    Ok(SavedLogs { tex, output })
}

/// Keep only the newest `keep` builds' logs (names sort chronologically).
fn prune_logs(dir: &Path, keep: usize) -> Result<()> {
    let mut outs: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "out"))
        .collect();
    outs.sort();
    let excess = outs.len().saturating_sub(keep);
    for out in &outs[..excess] {
        fs::remove_file(out)?;
        let log = out.with_extension("log");
        if log.is_file() {
            fs::remove_file(log)?;
        }
    }
    Ok(())
}

/// UTC time as `2026-10-04T15-30-12Z` (no colons, so it's a valid file name
/// everywhere, and lexical order is chronological).
fn timestamp(time: SystemTime) -> String {
    let secs = time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}-{:02}-{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Days since 1970-01-01 to a (year, month, day) date, after Howard Hinnant's
/// `civil_from_days` (restricted to dates after the epoch).
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn timestamps_are_utc_and_file_name_safe() {
        assert_eq!(timestamp(UNIX_EPOCH), "1970-01-01T00-00-00Z");
        let t = UNIX_EPOCH + Duration::from_secs(1_791_127_812);
        assert_eq!(timestamp(t), "2026-10-04T15-30-12Z");
        let leap = UNIX_EPOCH + Duration::from_secs(1_709_208_000);
        assert_eq!(timestamp(leap), "2024-02-29T12-00-00Z");
    }

    #[test]
    fn prune_keeps_newest_builds() {
        let tmp = tempfile::tempdir().unwrap();
        for i in 0..5 {
            fs::write(
                tmp.path().join(format!("2026-01-0{i}T00-00-00Z-main.out")),
                "",
            )
            .unwrap();
            fs::write(
                tmp.path().join(format!("2026-01-0{i}T00-00-00Z-main.log")),
                "",
            )
            .unwrap();
        }
        prune_logs(tmp.path(), 2).unwrap();
        let mut left: Vec<String> = fs::read_dir(tmp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(
            left,
            [
                "2026-01-03T00-00-00Z-main.log",
                "2026-01-03T00-00-00Z-main.out",
                "2026-01-04T00-00-00Z-main.log",
                "2026-01-04T00-00-00Z-main.out",
            ]
        );
    }
}
