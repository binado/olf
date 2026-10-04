//! Shared fixtures: a sandboxed HOME, local bare "Overleaf" remotes reached
//! through `OLF_GIT_BASE`, and git isolated from the developer's config.

#![allow(dead_code)] // each test binary uses a different subset

use assert_cmd::Command;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

pub const ID: &str = "64f0c0ffee0123456789abcd";
pub const OTHER_ID: &str = "0123456789abcdef01234567";
pub const MAIN_TEX: &str = "\\documentclass{article}\n\\begin{document}\nHello\n\\end{document}\n";

pub struct Env {
    pub tmp: tempfile::TempDir,
}

const GIT_ENV: &[(&str, &str)] = &[
    ("GIT_CONFIG_GLOBAL", "/dev/null"),
    ("GIT_CONFIG_NOSYSTEM", "1"),
    ("GIT_AUTHOR_NAME", "olf test"),
    ("GIT_AUTHOR_EMAIL", "test@example.com"),
    ("GIT_COMMITTER_NAME", "olf test"),
    ("GIT_COMMITTER_EMAIL", "test@example.com"),
];

impl Env {
    pub fn new() -> Self {
        let env = Self {
            tmp: tempfile::tempdir().unwrap(),
        };
        fs::create_dir_all(env.home()).unwrap();
        fs::create_dir_all(env.remotes()).unwrap();
        env
    }

    pub fn path(&self) -> PathBuf {
        // Canonical, so paths compare equal to what git reports (/private/var on macOS).
        self.tmp.path().canonicalize().unwrap()
    }

    pub fn home(&self) -> PathBuf {
        self.path().join("home")
    }

    pub fn remotes(&self) -> PathBuf {
        self.path().join("remotes")
    }

    pub fn olf(&self) -> Command {
        let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("olf"));
        cmd.current_dir(self.path())
            .env("HOME", self.home())
            .env("OLF_GIT_BASE", self.remotes())
            .env_remove("OVERLEAF_GIT_TOKEN")
            .env_remove("CLAUDE_CONFIG_DIR")
            .envs(GIT_ENV.iter().copied());
        cmd
    }

    /// Create a bare remote for `id` whose master holds `files`.
    pub fn remote(&self, id: &str, files: &[(&str, &str)]) -> PathBuf {
        let remote = self.remotes().join(id);
        fs::create_dir_all(&remote).unwrap();
        git(&remote, &["init", "--quiet", "--bare", "-b", "master"]);
        let seed = self.path().join(format!("seed-{id}"));
        fs::create_dir_all(&seed).unwrap();
        git(&seed, &["init", "--quiet", "-b", "master"]);
        for (name, content) in files {
            let path = seed.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
        git(&seed, &["add", "."]);
        git(&seed, &["commit", "--quiet", "-m", "init"]);
        git(
            &seed,
            &["push", "--quiet", remote.to_str().unwrap(), "master"],
        );
        fs::remove_dir_all(&seed).unwrap();
        remote
    }

    /// `olf init --id ID ws` against a remote holding a minimal document.
    pub fn workspace(&self) -> PathBuf {
        self.remote(ID, &[("main.tex", MAIN_TEX)]);
        self.olf()
            .args(["init", "--id", ID, "ws"])
            .assert()
            .success();
        self.path().join("ws")
    }
}

pub fn git(dir: &Path, args: &[&str]) -> String {
    let output = StdCommand::new("git")
        .current_dir(dir)
        .args(args)
        .envs(GIT_ENV.iter().copied())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .trim_end()
        .to_owned()
}
