mod common;

use common::{Env, ID};
use predicates::prelude::*;
use std::fs;

const RECORDER: &str = r#"{
  echo "args=$*"
  echo "pwd=$(pwd -P)"
  echo "ws=$OLF_WORKSPACE"
  echo "proj=$OLF_PROJECT_DIR"
} > "$OLF_RECORD"
exit "${OLF_EXIT:-0}""#;

#[test]
fn known_agent_gets_add_dir_first_and_runs_in_invocation_dir() {
    let env = Env::new();
    let ws = env.workspace();
    let code = env.code_repo();
    env.fake_tool("claude", RECORDER);
    let record = env.path().join("record");

    env.olf()
        .current_dir(&code)
        .env("OLF_RECORD", &record)
        .args(["-p", ID, "exec", "--", "claude", "--resume", "x"])
        .assert()
        .success();

    let out = fs::read_to_string(&record).unwrap();
    let paper = ws.join("paper");
    assert!(
        out.contains(&format!("args=--add-dir {} --resume x\n", paper.display())),
        "{out}"
    );
    assert!(out.contains(&format!("pwd={}\n", code.display())), "{out}");
    assert!(out.contains(&format!("ws={}\n", ws.display())), "{out}");
    assert!(
        out.contains(&format!("proj={}\n", paper.display())),
        "{out}"
    );
}

#[test]
fn agent_exit_code_propagates() {
    let env = Env::new();
    env.workspace();
    env.fake_tool("claude", RECORDER);
    env.olf()
        .env("OLF_RECORD", env.path().join("record"))
        .env("OLF_EXIT", "7")
        .args(["-p", ID, "exec", "--", "claude"])
        .assert()
        .code(7);
}

#[test]
fn unknown_agent_warns_and_gets_only_env() {
    let env = Env::new();
    let ws = env.workspace();
    env.fake_tool("mystery", RECORDER);
    let record = env.path().join("record");

    env.olf()
        .env("OLF_RECORD", &record)
        .args(["-p", ID, "exec", "--", "mystery", "go"])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "doesn't know how to grant mystery",
        ));
    let out = fs::read_to_string(&record).unwrap();
    assert!(out.contains("args=go\n"), "{out}");
    assert!(
        out.contains(&format!("proj={}\n", ws.join("paper").display())),
        "{out}"
    );
}

#[test]
fn missing_program_exits_6() {
    let env = Env::new();
    env.workspace();
    env.olf()
        .args(["-p", ID, "exec", "--", "no-such-agent-xyz"])
        .assert()
        .code(6);
}
