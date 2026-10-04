mod common;

use common::Env;
use predicates::prelude::*;
use std::fs;

const RECORD: &str = r#"printf '%s\n%s\n' "$*" "$PWD" > "$OUT""#;

#[test]
fn runs_editor_in_checkout_with_path_argument() {
    let env = Env::new();
    let ws = env.workspace();
    let out = env.path().join("edit.out");
    let editor = env.fake_tool("fake-editor", RECORD);
    env.olf()
        .current_dir(&ws)
        .env("EDITOR", editor)
        .env("OUT", &out)
        .arg("edit")
        .assert()
        .success();
    let checkout = ws.join("paper");
    let recorded = fs::read_to_string(out).unwrap();
    assert_eq!(recorded, format!("{0}\n{0}\n", checkout.display()));
}

#[test]
fn editor_may_carry_arguments_and_visual_wins() {
    let env = Env::new();
    let ws = env.workspace();
    let out = env.path().join("edit.out");
    let visual = env.fake_tool("visual", RECORD);
    env.olf()
        .current_dir(&ws)
        .env("EDITOR", "false")
        .env("VISUAL", format!("{} -w", visual.display()))
        .env("OUT", &out)
        .arg("edit")
        .assert()
        .success();
    let recorded = fs::read_to_string(out).unwrap();
    assert!(recorded.starts_with("-w "), "{recorded}");
}

#[test]
fn no_editor_fails() {
    let env = Env::new();
    let ws = env.workspace();
    env.olf()
        .current_dir(&ws)
        .arg("edit")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("no editor configured"));
}

#[test]
fn failing_editor_fails() {
    let env = Env::new();
    let ws = env.workspace();
    env.olf()
        .current_dir(&ws)
        .env("EDITOR", "false")
        .arg("edit")
        .assert()
        .code(1);
}
