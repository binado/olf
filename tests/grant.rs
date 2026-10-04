mod common;

use common::{Env, ID};
use predicates::prelude::*;
use serde_json::Value;
use std::fs;

const SETTINGS: &str = ".claude/settings.local.json";

fn dirs(repo: &std::path::Path) -> Value {
    let text = fs::read_to_string(repo.join(SETTINGS)).unwrap();
    let json: Value = serde_json::from_str(&text).unwrap();
    json["permissions"]["additionalDirectories"].clone()
}

#[test]
fn grants_code_repo_and_excludes_settings() {
    let env = Env::new();
    let ws = env.workspace();
    let code = env.code_repo();

    env.olf()
        .current_dir(&code)
        .args(["-p", ID, "grant", "claude"])
        .assert()
        .success()
        .stdout(predicate::str::contains("granted claude"));

    assert_eq!(dirs(&code), serde_json::json!([ws.join("paper")]));
    assert!(
        !ws.join(SETTINGS).exists(),
        "must not write into the workspace"
    );
    let exclude = fs::read_to_string(code.join(".git/info/exclude")).unwrap();
    assert!(
        exclude.contains("/.claude/settings.local.json"),
        "{exclude}"
    );

    env.olf()
        .current_dir(&code)
        .args(["-p", ID, "grant", "claude"])
        .assert()
        .success()
        .stdout(predicate::str::contains("already has access"));
    assert_eq!(dirs(&code).as_array().unwrap().len(), 1);
}

#[test]
fn preserves_existing_settings() {
    let env = Env::new();
    env.workspace();
    let code = env.code_repo();
    fs::create_dir_all(code.join(".claude")).unwrap();
    fs::write(
        code.join(SETTINGS),
        r#"{"model":"x","permissions":{"allow":["Bash(ls)"]}}"#,
    )
    .unwrap();

    env.olf()
        .current_dir(&code)
        .args(["-p", ID, "grant", "claude"])
        .assert()
        .success();
    let json: Value =
        serde_json::from_str(&fs::read_to_string(code.join(SETTINGS)).unwrap()).unwrap();
    assert_eq!(json["model"], "x");
    assert_eq!(json["permissions"]["allow"][0], "Bash(ls)");
}

#[test]
fn skips_exclude_when_already_ignored() {
    let env = Env::new();
    env.workspace();
    let code = env.code_repo();
    fs::write(code.join(".gitignore"), ".claude/settings.local.json\n").unwrap();

    env.olf()
        .current_dir(&code)
        .args(["-p", ID, "grant", "claude"])
        .assert()
        .success();
    let exclude = fs::read_to_string(code.join(".git/info/exclude")).unwrap_or_default();
    assert!(!exclude.contains("settings.local.json"), "{exclude}");
}

#[test]
fn subdir_snaps_to_repo_root() {
    let env = Env::new();
    env.workspace();
    let code = env.code_repo();
    fs::create_dir_all(code.join("src/deep")).unwrap();

    env.olf()
        .current_dir(code.join("src/deep"))
        .args(["-p", ID, "grant", "claude"])
        .assert()
        .success();
    assert!(code.join(SETTINGS).is_file());
    assert!(!code.join("src/deep/.claude").exists());
}

#[test]
fn explicit_repo_flag() {
    let env = Env::new();
    env.workspace();
    let code = env.code_repo();

    env.olf()
        .current_dir(env.home())
        .args(["-p", ID, "grant", "claude", "--repo"])
        .arg(&code)
        .assert()
        .success();
    assert!(code.join(SETTINGS).is_file());
}

#[test]
fn non_git_dir_works_without_exclude() {
    let env = Env::new();
    env.workspace();
    let plain = env.path().join("plain");
    fs::create_dir_all(&plain).unwrap();

    env.olf()
        .current_dir(&plain)
        .args(["-p", ID, "grant", "claude"])
        .assert()
        .success()
        .stdout(predicate::str::contains("excluded").not());
    assert!(plain.join(SETTINGS).is_file());
}

#[test]
fn invalid_json_fails_and_is_untouched() {
    let env = Env::new();
    env.workspace();
    let code = env.code_repo();
    fs::create_dir_all(code.join(".claude")).unwrap();
    fs::write(code.join(SETTINGS), "{ nope").unwrap();

    env.olf()
        .current_dir(&code)
        .args(["-p", ID, "grant", "claude"])
        .assert()
        .code(1);
    assert_eq!(fs::read_to_string(code.join(SETTINGS)).unwrap(), "{ nope");
}

#[test]
fn inside_workspace_is_a_noop() {
    let env = Env::new();
    let ws = env.workspace();

    env.olf()
        .current_dir(&ws)
        .args(["grant", "claude"])
        .assert()
        .success()
        .stdout(predicate::str::contains("nothing to grant"));
    assert!(!ws.join(SETTINGS).exists());
}

#[test]
fn outside_any_workspace_fails() {
    let env = Env::new();
    let code = env.code_repo();
    env.olf()
        .current_dir(&code)
        .args(["grant", "claude"])
        .assert()
        .code(2);
}
