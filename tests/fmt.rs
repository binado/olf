mod common;

use common::{Env, git};
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;

/// A fake tex-fmt that logs its argv (one per line) and exits with `$FAKE_EXIT`.
const FAKE: &str = r#"printf '%s\n' "$@" > "$LOG"; exit "${FAKE_EXIT:-0}""#;

struct Fixture {
    env: Env,
    ws: PathBuf,
    log: PathBuf,
}

fn fixture(enabled: bool) -> Fixture {
    let env = Env::new();
    let ws = env.workspace();
    let checkout = ws.join("paper");
    // Commit a baseline so only later edits count as changed.
    fs::write(checkout.join("kept.tex"), "kept\n").unwrap();
    git(&checkout, &["add", "kept.tex"]);
    git(&checkout, &["commit", "--quiet", "-m", "kept"]);
    if enabled {
        let cfg = ws.join(".olf/config.toml");
        let text = fs::read_to_string(&cfg).unwrap();
        fs::write(&cfg, text.replace("enabled = false", "enabled = true")).unwrap();
    }
    env.fake_tool("tex-fmt", FAKE);
    let log = env.path().join("tex-fmt.log");
    Fixture { env, ws, log }
}

impl Fixture {
    fn olf(&self) -> assert_cmd::Command {
        let mut cmd = self.env.olf();
        cmd.current_dir(&self.ws).env("LOG", &self.log);
        cmd
    }

    fn argv(&self) -> Vec<String> {
        fs::read_to_string(&self.log)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

#[test]
fn refuses_when_disabled() {
    let f = fixture(false);
    f.olf()
        .arg("fmt")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("fmt"))
        .stderr(predicate::str::contains("enabled = true"));
    assert!(!f.log.exists());
}

#[test]
fn missing_tool_exits_6() {
    let f = fixture(true);
    fs::remove_file(f.env.bin().join("tex-fmt")).unwrap();
    f.olf()
        .env("PATH", "/usr/bin:/bin")
        .arg("fmt")
        .assert()
        .code(6)
        .stderr(predicate::str::contains("cargo install tex-fmt"));
}

#[test]
fn default_mode_formats_only_changed_tex_files() {
    let f = fixture(true);
    let checkout = f.ws.join("paper");
    fs::write(checkout.join("main.tex"), "changed\n").unwrap();
    fs::write(checkout.join("new.tex"), "new\n").unwrap();
    fs::write(checkout.join("notes.md"), "ignored\n").unwrap();
    f.olf()
        .arg("fmt")
        .assert()
        .success()
        .stdout(predicate::str::contains("formatted 2 files"));
    assert_eq!(f.argv(), ["--noconfig", "--nowrap", "main.tex", "new.tex"]);
}

#[test]
fn nothing_changed_is_a_noop() {
    let f = fixture(true);
    f.olf()
        .arg("fmt")
        .assert()
        .success()
        .stdout("no changed .tex files\n");
    assert!(!f.log.exists());
}

#[test]
fn all_passes_every_tex_file() {
    let f = fixture(true);
    let checkout = f.ws.join("paper");
    fs::create_dir_all(checkout.join("sec")).unwrap();
    fs::write(checkout.join("sec/new.tex"), "x\n").unwrap();
    f.olf().args(["fmt", "--all"]).assert().success();
    assert_eq!(
        f.argv(),
        [
            "--noconfig",
            "--nowrap",
            "kept.tex",
            "main.tex",
            "sec/new.tex"
        ]
    );
}

#[test]
fn explicit_paths_must_be_inside_the_checkout() {
    let f = fixture(true);
    let checkout = f.ws.join("paper");
    f.olf()
        .current_dir(&checkout)
        .args(["fmt", "main.tex"])
        .assert()
        .success();
    assert_eq!(
        f.argv(),
        [
            "--noconfig".to_owned(),
            "--nowrap".to_owned(),
            checkout.join("main.tex").display().to_string()
        ]
    );

    fs::write(f.ws.join("outside.tex"), "x\n").unwrap();
    f.olf()
        .args(["fmt", "outside.tex"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("outside the checkout"));
}

#[test]
fn wrap_setting_and_project_config() {
    let f = fixture(true);
    let checkout = f.ws.join("paper");
    fs::write(checkout.join("main.tex"), "changed\n").unwrap();
    let cfg = f.ws.join(".olf/config.toml");
    let text = fs::read_to_string(&cfg).unwrap();
    fs::write(&cfg, text.replace("wrap = false", "wrap = true")).unwrap();
    f.olf().arg("fmt").assert().success();
    assert_eq!(f.argv(), ["--noconfig", "main.tex"]);

    fs::write(checkout.join("tex-fmt.toml"), "tabsize = 4\n").unwrap();
    f.olf().arg("fmt").assert().success();
    assert_eq!(
        f.argv(),
        ["main.tex", "tex-fmt.toml"].map(String::from)[..1]
    );
}

#[test]
fn check_failure_exits_5_and_write_failure_exits_1() {
    let f = fixture(true);
    fs::write(f.ws.join("paper/main.tex"), "changed\n").unwrap();
    f.olf()
        .env("FAKE_EXIT", "1")
        .args(["fmt", "--check"])
        .assert()
        .code(5);
    assert_eq!(f.argv(), ["--noconfig", "--nowrap", "--check", "main.tex"]);

    f.olf().env("FAKE_EXIT", "1").arg("fmt").assert().code(1);

    f.olf()
        .args(["fmt", "--check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("already formatted"));
}
