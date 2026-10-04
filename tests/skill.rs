mod common;

use common::{Env, git};
use predicates::prelude::*;
use std::fs;

const NAMES: [&str; 4] = ["olf-sync", "olf-editing", "olf-build", "olf-access"];

#[test]
fn lists_bundled_skills() {
    let env = Env::new();
    let mut assert = env.olf().args(["skill", "list"]).assert().success();
    for name in NAMES {
        assert = assert.stdout(predicate::str::contains(format!("{name}\t")));
    }
}

#[test]
fn installs_into_home_and_respects_force() {
    let env = Env::new();
    env.olf()
        .args(["skill", "install"])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed").count(4));
    let skills = env.home().join(".claude/skills");
    for name in NAMES {
        assert!(skills.join(name).join("SKILL.md").is_file(), "{name}");
    }

    env.olf()
        .args(["skill", "install"])
        .assert()
        .success()
        .stdout(predicate::str::contains("up to date").count(4));

    let sync = skills.join("olf-sync/SKILL.md");
    fs::write(&sync, "my edits").unwrap();
    env.olf()
        .args(["skill", "install"])
        .assert()
        .success()
        .stderr(predicate::str::contains("--force"));
    assert_eq!(fs::read_to_string(&sync).unwrap(), "my edits");

    env.olf()
        .args(["skill", "install", "--force"])
        .assert()
        .success();
    assert!(
        fs::read_to_string(&sync)
            .unwrap()
            .contains("name: olf-sync")
    );
}

#[test]
fn honours_claude_config_dir() {
    let env = Env::new();
    let config = env.path().join("claude-config");
    env.olf()
        .env("CLAUDE_CONFIG_DIR", &config)
        .args(["skill", "install"])
        .assert()
        .success();
    assert!(config.join("skills/olf-build/SKILL.md").is_file());
}

#[test]
fn project_install_goes_into_workspace() {
    let env = Env::new();
    let ws = env.workspace();
    env.olf()
        .current_dir(ws.join("paper"))
        .args(["skill", "install", "--project"])
        .assert()
        .success();
    assert!(ws.join(".claude/skills/olf-sync/SKILL.md").is_file());
    assert!(!env.home().join(".claude").exists());
    assert_eq!(git(&ws.join("paper"), &["status", "--porcelain"]), "");
}

#[test]
fn project_install_requires_workspace() {
    let env = Env::new();
    env.olf()
        .args(["skill", "install", "--project"])
        .assert()
        .code(2);
}
