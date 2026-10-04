mod common;

use common::{Env, git};
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn has_tex() -> bool {
    ["latexmk", "pdflatex"].iter().all(|tool| {
        Command::new(tool)
            .arg("-v")
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

/// A PATH holding only the given stub scripts.
fn stub_path(env: &Env, stubs: &[(&str, &str)]) -> PathBuf {
    let dir = env.path().join("stub-bin");
    fs::create_dir_all(&dir).unwrap();
    for (name, body) in stubs {
        let path = dir.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    dir
}

/// Writes `$out/main.log` (from $LOG) and `$out/main.pdf`, then exits $CODE.
const FAKE_LATEXMK: &str = r#"
for a; do case "$a" in -outdir=*) out="${a#-outdir=}";; esac; done
printf '%s\n' "$LOG" > "$out/main.log"
[ "$CODE" = 0 ] && printf 'pdf' > "$out/main.pdf"
exit "$CODE"
"#;

const FAKE_TECTONIC: &str = r#"
prev=; for a; do [ "$prev" = --outdir ] && out="$a"; prev="$a"; done
printf '(./main.tex)\n' > "$out/main.log"
printf 'pdf' > "$out/main.pdf"
"#;

#[test]
fn outside_workspace_is_not_a_project() {
    let env = Env::new();
    env.olf()
        .arg("build")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("olf init"));
}

#[test]
fn missing_engines_exit_with_install_hints() {
    let env = Env::new();
    let ws = env.workspace();
    let path = stub_path(&env, &[]);
    env.olf()
        .current_dir(&ws)
        .env("PATH", &path)
        .arg("build")
        .assert()
        .code(6)
        .stderr(predicate::str::contains("latexmk or tectonic"))
        .stderr(predicate::str::contains("tectonic-typesetting"));
}

#[test]
fn auto_falls_back_to_tectonic_with_warning() {
    let env = Env::new();
    let ws = env.workspace();
    let path = stub_path(&env, &[("tectonic", FAKE_TECTONIC)]);
    env.olf()
        .current_dir(&ws)
        .env("PATH", &path)
        .arg("build")
        .assert()
        .success()
        .stderr(predicate::str::contains("tectonic (XeTeX-based)"))
        .stdout(predicate::str::contains("with tectonic"));
    assert!(ws.join(".olf/build/main.pdf").is_file());

    env.olf()
        .current_dir(&ws)
        .env("PATH", &path)
        .args(["build", "--engine", "latexmk"])
        .assert()
        .code(6);
}

/// The saved logs of all builds, sorted oldest first.
fn saved_logs(ws: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(ws.join(".olf/build/logs"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn auto_prefers_latexmk_and_keeps_timestamped_logs() {
    let env = Env::new();
    let ws = env.workspace();
    let path = stub_path(&env, &[("latexmk", FAKE_LATEXMK), ("tectonic", "exit 99")]);
    let log = "./main.tex:3: Undefined control sequence.";
    env.olf()
        .current_dir(ws.join("paper"))
        .env("PATH", &path)
        .env("LOG", log)
        .env("CODE", "12")
        .arg("build")
        .assert()
        .code(4)
        .stdout("")
        .stderr(predicate::str::contains("build failed (latexmk, exit 12)"))
        .stderr(predicate::str::contains(".olf/build/logs/"))
        .stderr(predicate::str::contains("-main.log"));

    let names = saved_logs(&ws);
    assert_eq!(names.len(), 2, "{names:?}");
    let base = names[0].strip_suffix(".log").unwrap();
    assert_eq!(names[1], format!("{base}.out"));
    assert!(base.ends_with("Z-main"), "{base}");
    let saved = fs::read_to_string(ws.join(".olf/build/logs").join(&names[0])).unwrap();
    assert_eq!(saved.trim_end(), log);

    let out = env
        .olf()
        .current_dir(&ws)
        .env("PATH", &path)
        .env("LOG", "fine")
        .env("CODE", "0")
        .args(["--json", "build"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["engine"], "latexmk");
    assert!(
        json["pdf"]
            .as_str()
            .unwrap()
            .ends_with(".olf/build/main.pdf")
    );
    let log = json["log"].as_str().unwrap();
    assert_eq!(fs::read_to_string(log).unwrap().trim_end(), "fine");
    assert!(json["output"].as_str().unwrap().ends_with("-main.out"));
    assert_eq!(saved_logs(&ws).len(), 4);
}

#[test]
fn engine_output_is_kept_when_tex_never_ran() {
    let env = Env::new();
    let ws = env.workspace();
    let path = stub_path(
        &env,
        &[("latexmk", "echo 'biber: command not found' >&2; exit 12")],
    );
    env.olf()
        .current_dir(&ws)
        .env("PATH", &path)
        .arg("build")
        .assert()
        .code(4)
        .stderr(predicate::str::contains("engine output:"))
        .stderr(predicate::str::contains("log:").not());
    let names = saved_logs(&ws);
    assert_eq!(names.len(), 1, "{names:?}");
    let out = fs::read_to_string(ws.join(".olf/build/logs").join(&names[0])).unwrap();
    assert!(out.contains("biber: command not found"));
}

fn write_main(paper: &Path, body: &str) {
    fs::write(
        paper.join("main.tex"),
        format!("\\documentclass{{article}}\n\\begin{{document}}\n{body}\n\\end{{document}}\n"),
    )
    .unwrap();
}

#[test]
fn builds_real_document_with_latexmk() {
    if !has_tex() {
        eprintln!("skipping: latexmk/pdflatex not on PATH");
        return;
    }
    let env = Env::new();
    let ws = env.workspace();
    let paper = ws.join("paper");

    env.olf()
        .current_dir(&ws)
        .arg("build")
        .assert()
        .success()
        .stdout(predicate::str::contains("built"));
    assert!(ws.join(".olf/build/main.pdf").is_file());
    assert_eq!(git(&paper, &["status", "--porcelain"]), "");

    write_main(&paper, "Hello \\undefinedmacro.");
    env.olf().current_dir(&ws).arg("build").assert().code(4);
    // The newest saved log holds the error in -file-line-error form.
    let newest = saved_logs(&ws)
        .into_iter()
        .rfind(|n| Path::new(n).extension().is_some_and(|e| e == "log"))
        .unwrap();
    let log = fs::read_to_string(ws.join(".olf/build/logs").join(newest)).unwrap();
    assert!(log.contains("./main.tex:3: Undefined control sequence."));
    assert_eq!(
        git(&paper, &["status", "--porcelain"]),
        " M main.tex",
        "build must not write into the checkout"
    );
}
