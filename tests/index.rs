mod common;

use common::{Env, ID, MAIN_TEX, OTHER_ID};
use predicates::prelude::*;
use std::fs;

fn link(env: &Env, id: &str) -> std::path::PathBuf {
    env.olf_home().join("projects").join(id)
}

#[test]
fn init_registers_and_reinit_is_a_noop() {
    let env = Env::new();
    let ws = env.workspace();
    assert_eq!(fs::read_link(link(&env, ID)).unwrap(), ws);

    env.olf()
        .args(["init", "ws"])
        .assert()
        .success()
        .stdout(predicate::str::contains("registered in").not());
    assert_eq!(fs::read_link(link(&env, ID)).unwrap(), ws);
}

#[test]
fn second_workspace_for_same_id_needs_force() {
    let env = Env::new();
    let ws = env.workspace();

    env.olf()
        .args(["init", "--id", ID, "ws2"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("--force"));
    assert!(!env.path().join("ws2").exists(), "no side effects");
    assert_eq!(fs::read_link(link(&env, ID)).unwrap(), ws);

    env.olf()
        .args(["init", "--id", ID, "--force", "ws2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("registered in"));
    assert_eq!(
        fs::read_link(link(&env, ID)).unwrap(),
        env.path().join("ws2")
    );
}

#[test]
fn list_shows_ok_and_missing_and_prune_removes() {
    let env = Env::new();
    let ws = env.workspace();
    env.remote(OTHER_ID, &[("main.tex", MAIN_TEX)]);
    env.olf()
        .args(["init", "--id", OTHER_ID, "gone"])
        .assert()
        .success();
    fs::remove_dir_all(env.path().join("gone")).unwrap();

    env.olf()
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "{ID}\t{}\n",
            ws.display()
        )))
        .stdout(predicate::str::contains("(missing)"))
        .stderr(predicate::str::contains("olf list --prune"));

    env.olf()
        .args(["list", "--prune"])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!("pruned {OTHER_ID} ->")))
        .stdout(predicate::str::contains(ID));
    assert!(!link(&env, OTHER_ID).exists());
    assert!(fs::symlink_metadata(link(&env, OTHER_ID)).is_err());
}

#[test]
fn list_empty_and_json() {
    let env = Env::new();
    env.olf()
        .arg("list")
        .assert()
        .success()
        .stderr(predicate::str::contains("no projects registered"));

    let ws = env.workspace();
    let out = env.olf().args(["list", "--json"]).output().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        json,
        serde_json::json!([{ "id": ID, "path": ws, "status": "ok" }])
    );
}

#[test]
fn project_flag_resolves_id_prefix_and_name() {
    let env = Env::new();
    env.workspace();
    let url = format!("https://www.overleaf.com/project/{ID}\n");
    for query in [ID, &ID[..6], "ws"] {
        env.olf()
            .current_dir(env.home())
            .args(["-p", query, "open", "--print"])
            .assert()
            .success()
            .stdout(url.clone());
    }
}

#[test]
fn unknown_and_ambiguous_projects_exit_8() {
    let env = Env::new();
    env.workspace();
    env.remote("64f0c0ffee9999999999abcd", &[("main.tex", MAIN_TEX)]);
    env.olf()
        .args(["init", "--id", "64f0c0ffee9999999999abcd", "ws-b"])
        .assert()
        .success();

    env.olf()
        .args(["-p", "nope", "path"])
        .assert()
        .code(8)
        .stderr(predicate::str::contains("olf list"));
    env.olf()
        .args(["-p", "64f0", "path"])
        .assert()
        .code(8)
        .stderr(predicate::str::contains(ID));
}
