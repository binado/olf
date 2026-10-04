mod common;

use common::{Env, ID, MAIN_TEX, OTHER_ID, git};
use predicates::prelude::*;
use std::fs;

#[test]
fn fresh_clone_sets_up_workspace() {
    let env = Env::new();
    env.remote(ID, &[("main.tex", MAIN_TEX)]);
    env.olf()
        .args([
            "init",
            "--url",
            &format!("https://www.overleaf.com/project/{ID}"),
            "ws",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("cloned"))
        .stdout(predicate::str::contains("workspace ready"));

    let ws = env.path().join("ws");
    let paper = ws.join("paper");
    let config = fs::read_to_string(ws.join(".olf/config.toml")).unwrap();
    assert!(
        config.contains(&format!("project_id = \"{ID}\"")),
        "{config}"
    );
    assert!(config.contains("project_dir = \"paper\""), "{config}");
    assert!(config.contains("main = \"main.tex\""), "{config}");
    assert!(config.contains("engine = \"auto\""), "{config}");

    assert!(paper.join("main.tex").is_file());
    assert_eq!(git(&paper, &["config", "pull.rebase"]), "true");
    let hook = fs::read_to_string(paper.join(".git/hooks/pre-push")).unwrap();
    assert!(hook.contains("# olf-managed"));

    fs::write(paper.join("main.aux"), "").unwrap();
    fs::write(paper.join("main.synctex.gz"), "").unwrap();
    assert_eq!(git(&paper, &["status", "--porcelain"]), "");
}

#[test]
fn pull_rebases_despite_global_ff_only() {
    let env = Env::new();
    let ws = env.workspace();
    let paper = ws.join("paper");

    // A co-author edits another file "in the browser".
    let browser = env.path().join("browser");
    git(
        &env.path(),
        &[
            "clone",
            "--quiet",
            env.remotes().join(ID).to_str().unwrap(),
            "browser",
        ],
    );
    fs::write(browser.join("other.tex"), "browser edit\n").unwrap();
    git(&browser, &["add", "."]);
    git(&browser, &["commit", "--quiet", "-m", "browser"]);
    git(&browser, &["push", "--quiet"]);

    fs::write(paper.join("local.tex"), "local edit\n").unwrap();
    git(&paper, &["add", "."]);
    git(&paper, &["commit", "--quiet", "-m", "local"]);

    let global = env.path().join("gitconfig");
    fs::write(&global, "[pull]\n\tff = only\n").unwrap();
    let output = std::process::Command::new("git")
        .current_dir(&paper)
        .args(["pull", "--quiet"])
        .env("GIT_CONFIG_GLOBAL", &global)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Linear history: local commit rebased on top of the browser edit.
    assert_eq!(
        git(&paper, &["log", "--format=%s", "-3"]),
        "local\nbrowser\ninit"
    );
}

#[test]
fn rerun_is_idempotent_and_keeps_user_config() {
    let env = Env::new();
    let ws = env.workspace();
    let config_path = ws.join(".olf/config.toml");
    let edited = fs::read_to_string(&config_path).unwrap().replace(
        "compiler = \"pdflatex\"",
        "compiler = \"xelatex\" # as on Overleaf",
    );
    fs::write(&config_path, &edited).unwrap();

    env.olf()
        .args(["init"])
        .current_dir(&ws)
        .assert()
        .success()
        .stdout(predicate::str::contains("cloned").not())
        .stdout(predicate::str::contains("wrote").not());
    // From inside the checkout, via -C.
    env.olf()
        .args(["-C", "ws/paper", "init"])
        .assert()
        .success();
    assert_eq!(fs::read_to_string(&config_path).unwrap(), edited);
}

#[test]
fn rerun_reclones_missing_checkout() {
    let env = Env::new();
    let ws = env.workspace();
    fs::remove_dir_all(ws.join("paper")).unwrap();
    env.olf()
        .args(["-C", "ws", "init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cloned"));
    assert!(ws.join("paper/main.tex").is_file());
}

#[test]
fn project_mismatch_is_an_error() {
    let env = Env::new();
    env.workspace();
    env.olf()
        .args(["init", "--id", OTHER_ID, "ws"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("project mismatch"));
}

#[test]
fn refuses_inside_plain_git_repo() {
    let env = Env::new();
    env.remote(ID, &[("main.tex", MAIN_TEX)]);
    let code = env.path().join("code");
    fs::create_dir_all(&code).unwrap();
    git(&code, &["init", "--quiet"]);
    env.olf()
        .args(["init", "--id", ID, "code/paper-ws"])
        .assert()
        .code(7)
        .stderr(predicate::str::contains("code-paper"));
    assert!(!code.join("paper-ws").exists());
}

#[test]
fn adopts_existing_overleaf_clone() {
    let env = Env::new();
    let remote = env.remote(ID, &[("main.tex", MAIN_TEX)]);
    git(
        &env.path(),
        &["clone", "--quiet", remote.to_str().unwrap(), "clone"],
    );
    let clone = env.path().join("clone");

    env.olf()
        .args(["init", "clone"])
        .assert()
        .success()
        .stdout(predicate::str::contains("adopting"));
    let config = fs::read_to_string(clone.join(".olf/config.toml")).unwrap();
    assert!(config.contains("project_dir = \".\""), "{config}");
    assert!(config.contains(ID));
    fs::create_dir_all(clone.join(".claude/skills")).unwrap();
    fs::write(clone.join(".claude/skills/x"), "").unwrap();
    assert_eq!(git(&clone, &["status", "--porcelain"]), "");

    // Subdirectories of the clone aren't workspaces of their own.
    env.olf().args(["init", "clone/sub"]).assert().success();
    assert!(!clone.join("sub").exists());
}

#[test]
fn refuses_subdir_of_unadopted_overleaf_clone() {
    let env = Env::new();
    let remote = env.remote(ID, &[("main.tex", MAIN_TEX)]);
    git(
        &env.path(),
        &["clone", "--quiet", remote.to_str().unwrap(), "clone"],
    );
    env.olf()
        .args(["init", "clone/ws"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("inside the Overleaf clone"));
}

#[test]
fn ambiguous_main_file_requires_flag() {
    let env = Env::new();
    env.remote(ID, &[("a.tex", MAIN_TEX), ("b.tex", MAIN_TEX)]);
    env.olf()
        .args(["init", "--id", ID, "ws"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("a.tex, b.tex"))
        .stderr(predicate::str::contains("--main"));

    env.olf()
        .args(["init", "--id", ID, "--main", "b.tex", "ws"])
        .assert()
        .success();
    let config = fs::read_to_string(env.path().join("ws/.olf/config.toml")).unwrap();
    assert!(config.contains("main = \"b.tex\""), "{config}");

    // An explicit --main replaces the configured one.
    env.olf()
        .args(["init", "--main", "a.tex", "ws"])
        .assert()
        .success();
    let config = fs::read_to_string(env.path().join("ws/.olf/config.toml")).unwrap();
    assert!(config.contains("main = \"a.tex\""), "{config}");
}

#[test]
fn missing_project_is_an_error() {
    let env = Env::new();
    env.olf()
        .args(["init", "ws"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("--url"));
}

#[test]
fn grant_flag_writes_the_invoking_repos_settings() {
    let env = Env::new();
    env.remote(ID, &[("main.tex", MAIN_TEX)]);
    let code = env.code_repo();

    env.olf()
        .current_dir(&code)
        .args(["init", "--id", ID, "../ws", "--grant", "claude"])
        .assert()
        .success()
        .stdout(predicate::str::contains("granted claude"));

    let settings = fs::read_to_string(code.join(".claude/settings.local.json")).unwrap();
    assert!(
        settings.contains(&env.path().join("ws/paper").display().to_string()),
        "{settings}"
    );
}
