mod common;

use common::{Env, ID, git};
use std::fs;

#[test]
fn prints_checkout_from_root_and_subdir() {
    let env = Env::new();
    let ws = env.workspace();
    let expected = format!("{}\n", ws.join("paper").display());

    env.olf()
        .current_dir(&ws)
        .arg("path")
        .assert()
        .success()
        .stdout(expected.clone());

    fs::create_dir_all(ws.join("paper/sub")).unwrap();
    env.olf()
        .current_dir(ws.join("paper/sub"))
        .arg("path")
        .assert()
        .success()
        .stdout(expected.clone());

    env.olf()
        .current_dir(env.home())
        .args(["-p", &ID[..8], "path"])
        .assert()
        .success()
        .stdout(expected);
}

#[test]
fn adopted_clone_prints_the_root() {
    let env = Env::new();
    let remote = env.remote(ID, &[("main.tex", common::MAIN_TEX)]);
    let clone = env.path().join("clone");
    git(
        &env.path(),
        &["clone", "--quiet", remote.to_str().unwrap(), "clone"],
    );
    git(
        &clone,
        &[
            "remote",
            "set-url",
            "origin",
            &format!("{}/{ID}", env.remotes().display()),
        ],
    );
    env.olf().current_dir(&clone).arg("init").assert().success();
    env.olf()
        .current_dir(&clone)
        .arg("path")
        .assert()
        .success()
        .stdout(format!("{}\n", clone.display()));
}
