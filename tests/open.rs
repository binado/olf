mod common;

use common::{Env, ID};
use predicates::prelude::*;

#[test]
fn prints_project_url() {
    let env = Env::new();
    let ws = env.workspace();
    env.olf()
        .current_dir(ws.join("paper"))
        .args(["open", "--print"])
        .assert()
        .success()
        .stdout(format!("https://www.overleaf.com/project/{ID}\n"));
}

#[test]
fn requires_a_workspace() {
    let env = Env::new();
    env.olf()
        .args(["open", "--print"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("not inside an olf workspace"));
}

#[test]
fn prints_project_url_via_project_flag() {
    let env = Env::new();
    env.workspace();
    env.olf()
        .current_dir(env.home())
        .args(["-p", ID, "open", "--print"])
        .assert()
        .success()
        .stdout(format!("https://www.overleaf.com/project/{ID}\n"));
}
