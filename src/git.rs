//! Thin wrapper over the `git` binary.
//!
//! We shell out instead of using libgit2 so that credential helpers and hooks
//! behave exactly as they do for the agent's own `git` invocations.

use crate::error::{OlfError, Result};
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const BLOCK_START: &str = "# >>> olf (managed by `olf init`; edits inside are overwritten)";
const BLOCK_END: &str = "# <<< olf";
pub const HOOK_MARKER: &str = "# olf-managed";

fn command(dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(dir);
    // Agents can't answer prompts; humans at a terminal still can.
    if !std::io::stdin().is_terminal() {
        cmd.env("GIT_TERMINAL_PROMPT", "0");
    }
    #[cfg(test)]
    tests::isolate(&mut cmd);
    cmd
}

fn spawn_error(e: &std::io::Error) -> OlfError {
    if e.kind() == std::io::ErrorKind::NotFound {
        OlfError::MissingTool {
            tool: "git".into(),
            hint: "install git from https://git-scm.com".into(),
        }
    } else {
        OlfError::Error(format!("cannot run git: {e}"))
    }
}

/// Run git in `dir`, returning trimmed stdout; failures carry stderr.
pub fn run(dir: &Path, args: &[&str]) -> Result<String> {
    let output = output(dir, args)?;
    check(args, &output)?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_owned())
}

/// Run git in `dir` without interpreting the exit status.
pub fn output(dir: &Path, args: &[&str]) -> Result<Output> {
    command(dir)
        .args(args)
        .output()
        .map_err(|e| spawn_error(&e))
}

fn check(args: &[&str], output: &Output) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if is_auth_failure(&stderr) {
        return Err(OlfError::AuthFailed(stderr));
    }
    Err(OlfError::Error(format!(
        "`git {}` failed:\n{stderr}",
        args.join(" ")
    )))
}

/// Whether git's stderr says the server rejected our credentials.
pub fn is_auth_failure(stderr: &str) -> bool {
    let s = stderr.to_ascii_lowercase();
    [
        "authentication failed",
        "could not read username",
        "could not read password",
        "invalid username or password",
        "the requested url returned error: 401",
        "the requested url returned error: 403",
    ]
    .iter()
    .any(|needle| s.contains(needle))
}

pub fn is_inside_work_tree(dir: &Path) -> bool {
    output(dir, &["rev-parse", "--is-inside-work-tree"])
        .is_ok_and(|o| o.status.success() && o.stdout.starts_with(b"true"))
}

pub fn toplevel(dir: &Path) -> Result<PathBuf> {
    run(dir, &["rev-parse", "--show-toplevel"]).map(PathBuf::from)
}

pub fn origin_url(dir: &Path) -> Option<String> {
    run(dir, &["remote", "get-url", "origin"])
        .ok()
        .filter(|s| !s.is_empty())
}

/// Resolve a path inside the git dir (worktree- and `core.hooksPath`-aware).
pub fn git_path(dir: &Path, name: &str) -> Result<PathBuf> {
    let path = PathBuf::from(run(dir, &["rev-parse", "--git-path", name])?);
    Ok(if path.is_absolute() {
        path
    } else {
        dir.join(path)
    })
}

pub fn set_config(dir: &Path, key: &str, value: &str) -> Result<()> {
    run(dir, &["config", "--local", key, value]).map(drop)
}

/// Make `lines` the content of olf's block in `info/exclude`, leaving any
/// other patterns untouched. Returns whether the file changed.
pub fn ensure_exclude_block(dir: &Path, lines: &[&str]) -> Result<bool> {
    let path = git_path(dir, "info/exclude")?;
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let updated = replace_block(&existing, lines);
    if updated == existing {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, updated)?;
    Ok(true)
}

fn replace_block(existing: &str, lines: &[&str]) -> String {
    let mut block = format!("{BLOCK_START}\n");
    for line in lines {
        block.push_str(line);
        block.push('\n');
    }
    block.push_str(BLOCK_END);
    block.push('\n');

    let start = existing.find(BLOCK_START);
    let end = existing.find(BLOCK_END).map(|i| i + BLOCK_END.len());
    match (start, end) {
        (Some(s), Some(e)) if s < e => {
            let rest = existing[e..].strip_prefix('\n').unwrap_or(&existing[e..]);
            format!("{}{block}{rest}", &existing[..s])
        }
        _ if existing.is_empty() || existing.ends_with('\n') => format!("{existing}{block}"),
        _ => format!("{existing}\n{block}"),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum HookStatus {
    Installed,
    Unchanged,
    /// A hook olf didn't write is in place; it was left alone.
    Foreign(PathBuf),
}

const PRE_PUSH_HOOK: &str = r#"#!/bin/sh
# olf-managed: pre-push guard for the Overleaf git bridge.
# Re-run `olf init` to update it. Bypass (not recommended): git push --no-verify
#
# The bridge has a single branch (the remote's HEAD, e.g. `main` or
# `master`) and linear history, so reject pushes to
# other refs, deletions, and non-fast-forward (force) pushes.

remote="$1"
branch=$(git symbolic-ref --quiet --short "refs/remotes/$remote/HEAD" 2>/dev/null)
branch=${branch#"$remote"/}
allowed=${branch:+refs/heads/$branch}
# Pushing by URL or without a known remote HEAD: accept either default name.
allowed=${allowed:-refs/heads/main refs/heads/master}

status=0
while read -r local_ref local_sha remote_ref remote_sha; do
  ok=
  for ref in $allowed; do
    [ "$remote_ref" = "$ref" ] && ok=1
  done
  if [ -z "$ok" ]; then
    echo "olf: refusing to push to $remote_ref: this Overleaf project only has $allowed" >&2
    echo "olf: pull, then push that branch with plain \`git push\`" >&2
    status=1
    continue
  fi
  case "$local_sha" in
    *[!0]*) ;;
    *)
      echo "olf: refusing to delete $remote_ref on Overleaf" >&2
      status=1
      continue
      ;;
  esac
  case "$remote_sha" in
    *[!0]*) ;;
    *) continue ;; # new ref on the remote: nothing to overwrite
  esac
  if ! git merge-base --is-ancestor "$remote_sha" "$local_sha" 2>/dev/null; then
    echo "olf: refusing non-fast-forward push to $remote_ref (would overwrite co-authors' work)" >&2
    echo "olf: run \`git pull\` (rebases onto Overleaf) and push again" >&2
    status=1
  fi
done
exit $status
"#;

/// Install olf's pre-push hook unless a hook olf didn't write is present.
pub fn install_pre_push_hook(dir: &Path) -> Result<HookStatus> {
    let path = git_path(dir, "hooks")?.join("pre-push");
    match fs::read_to_string(&path) {
        Ok(current) if current == PRE_PUSH_HOOK => return Ok(HookStatus::Unchanged),
        Ok(current) if !current.contains(HOOK_MARKER) => return Ok(HookStatus::Foreign(path)),
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, PRE_PUSH_HOOK)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    }
    Ok(HookStatus::Installed)
}

/// One-shot credential helper answering with `$OLF_TOKEN`, so the token never
/// appears in argv or on disk.
const INLINE_HELPER: &str = "credential.helper=!f() { test \"$1\" = get && echo username=git && echo \"password=$OLF_TOKEN\"; }; f";

/// Clone `url` into `dest`, authenticating with `token` if given.
pub fn clone(cwd: &Path, url: &str, dest: &Path, token: Option<&str>) -> Result<()> {
    let mut cmd = command(cwd);
    if let Some(token) = token {
        // Reset inherited helpers so the token isn't stored before we decide where.
        cmd.args(["-c", "credential.helper=", "-c", INLINE_HELPER])
            .env("OLF_TOKEN", token);
    }
    let args = ["clone", "--quiet", "--origin", "origin", url];
    let output = cmd
        .args(args)
        .arg(dest)
        .stdin(Stdio::inherit())
        .output()
        .map_err(|e| spawn_error(&e))?;
    check(&args, &output)
}

#[derive(Debug, PartialEq, Eq)]
pub enum CredentialStatus {
    Stored {
        helper: String,
    },
    /// No helper is configured, so git will prompt on the next pull/push.
    NoHelper,
    /// The remote isn't http(s) (e.g. local test remotes); nothing to store.
    NotHttp,
}

/// Persist `token` for `url` in the user's credential helper, configuring the
/// macOS Keychain for this checkout when no helper is set up.
pub fn store_credential(checkout: &Path, url: &str, token: &str) -> Result<CredentialStatus> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Ok(CredentialStatus::NotHttp);
    }
    let mut helper = run(checkout, &["config", "--get-all", "credential.helper"])
        .unwrap_or_default()
        .lines()
        .rfind(|l| !l.trim().is_empty())
        .map(str::to_owned);
    if helper.is_none() && cfg!(target_os = "macos") {
        set_config(checkout, "credential.helper", "osxkeychain")?;
        helper = Some("osxkeychain".into());
    }
    let Some(helper) = helper else {
        return Ok(CredentialStatus::NoHelper);
    };

    let mut child = command(checkout)
        .args(["credential", "approve"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| spawn_error(&e))?;
    let input = format!("url={url}\nusername=git\npassword={token}\n\n");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(input.as_bytes())?;
    let output = child.wait_with_output()?;
    check(&["credential", "approve"], &output)?;
    Ok(CredentialStatus::Stored { helper })
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Shield unit tests from the developer's global git config.
    pub fn isolate(cmd: &mut Command) {
        cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "olf test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "olf test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com");
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        run(dir, args).unwrap()
    }

    fn remote_and_clone(tmp: &Path) -> (PathBuf, PathBuf) {
        remote_and_clone_on(tmp, "master")
    }

    /// A bare remote with one commit on `branch` (its HEAD), and a clone of it.
    fn remote_and_clone_on(tmp: &Path, branch: &str) -> (PathBuf, PathBuf) {
        let remote = tmp.join("remote.git");
        let clone_dir = tmp.join("clone");
        fs::create_dir_all(&remote).unwrap();
        git(&remote, &["init", "--quiet", "--bare", "-b", branch]);
        git(tmp, &["init", "--quiet", "-b", branch, "seed"]);
        let seed = tmp.join("seed");
        fs::write(seed.join("main.tex"), "hello\n").unwrap();
        git(&seed, &["add", "."]);
        git(&seed, &["commit", "--quiet", "-m", "init"]);
        git(
            &seed,
            &["push", "--quiet", remote.to_str().unwrap(), branch],
        );
        clone(tmp, remote.to_str().unwrap(), &clone_dir, None).unwrap();
        (remote, clone_dir)
    }

    fn commit(dir: &Path, file: &str, msg: &str) {
        fs::write(dir.join(file), msg).unwrap();
        git(dir, &["add", file]);
        git(dir, &["commit", "--quiet", "-m", msg]);
    }

    #[test]
    fn exclude_block_is_idempotent_and_preserves_user_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, dir) = remote_and_clone(tmp.path());
        let exclude = git_path(&dir, "info/exclude").unwrap();
        fs::write(&exclude, "# user\n*.bak").unwrap();

        assert!(ensure_exclude_block(&dir, &["*.aux", "*.log"]).unwrap());
        assert!(!ensure_exclude_block(&dir, &["*.aux", "*.log"]).unwrap());
        assert!(ensure_exclude_block(&dir, &["*.aux"]).unwrap());
        fs::write(&exclude, fs::read_to_string(&exclude).unwrap() + "*.tmp\n").unwrap();
        assert!(!ensure_exclude_block(&dir, &["*.aux"]).unwrap());

        let text = fs::read_to_string(&exclude).unwrap();
        assert_eq!(
            text,
            format!("# user\n*.bak\n{BLOCK_START}\n*.aux\n{BLOCK_END}\n*.tmp\n")
        );
        fs::write(dir.join("x.aux"), "").unwrap();
        assert_eq!(git(&dir, &["status", "--porcelain"]), "");
    }

    #[test]
    fn hook_install_is_idempotent_and_respects_foreign_hooks() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, dir) = remote_and_clone(tmp.path());
        assert_eq!(install_pre_push_hook(&dir).unwrap(), HookStatus::Installed);
        assert_eq!(install_pre_push_hook(&dir).unwrap(), HookStatus::Unchanged);

        let hook = git_path(&dir, "hooks").unwrap().join("pre-push");
        fs::write(&hook, "#!/bin/sh\nexit 0\n").unwrap();
        assert_eq!(
            install_pre_push_hook(&dir).unwrap(),
            HookStatus::Foreign(hook.clone())
        );
        assert_eq!(fs::read_to_string(&hook).unwrap(), "#!/bin/sh\nexit 0\n");
    }

    fn assert_hook_guards(branch: &str) {
        let tmp = tempfile::tempdir().unwrap();
        let (_, dir) = remote_and_clone_on(tmp.path(), branch);
        install_pre_push_hook(&dir).unwrap();

        commit(&dir, "a.tex", "a");
        git(&dir, &["push", "--quiet"]);

        let other = if branch == "main" { "master" } else { "main" };
        for target in ["HEAD:other".to_owned(), format!("HEAD:{other}")] {
            let err = run(&dir, &["push", "origin", &target]).unwrap_err();
            assert!(
                err.to_string()
                    .contains(&format!("only has refs/heads/{branch}")),
                "{err}"
            );
        }

        git(&dir, &["reset", "--quiet", "--hard", "HEAD~1"]);
        commit(&dir, "b.tex", "b");
        let err = run(&dir, &["push", "--force", "origin", branch]).unwrap_err();
        assert!(err.to_string().contains("non-fast-forward"), "{err}");

        let err = run(&dir, &["push", "origin", &format!(":{branch}")]).unwrap_err();
        assert!(err.to_string().contains("refusing to delete"), "{err}");
    }

    #[test]
    fn hook_guards_master_projects() {
        assert_hook_guards("master");
    }

    #[test]
    fn hook_guards_main_projects() {
        assert_hook_guards("main");
    }

    #[test]
    fn toplevel_and_origin() {
        let tmp = tempfile::tempdir().unwrap();
        let (remote, dir) = remote_and_clone(tmp.path());
        fs::create_dir_all(dir.join("sub")).unwrap();
        assert!(is_inside_work_tree(&dir.join("sub")));
        assert!(!is_inside_work_tree(tmp.path()));
        assert_eq!(
            toplevel(&dir.join("sub")).unwrap().canonicalize().unwrap(),
            dir.canonicalize().unwrap()
        );
        assert_eq!(origin_url(&dir).unwrap(), remote.to_str().unwrap());
    }

    #[test]
    fn classifies_real_bridge_auth_failure() {
        let stderr = include_str!("../tests/fixtures/git/clone-bad-token.txt");
        assert!(is_auth_failure(stderr));
    }

    #[test]
    fn classifies_auth_failures() {
        assert!(is_auth_failure(
            "remote: HTTP Basic: Access denied\nfatal: Authentication failed for 'https://git.overleaf.com/x/'"
        ));
        assert!(is_auth_failure(
            "fatal: could not read Username for 'https://git.overleaf.com': terminal prompts disabled"
        ));
        assert!(is_auth_failure(
            "fatal: unable to access 'https://git.overleaf.com/x/': The requested URL returned error: 403"
        ));
        assert!(!is_auth_failure("fatal: repository 'x' not found"));
    }

    #[test]
    fn store_credential_skips_non_http_remotes() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            store_credential(tmp.path(), "/some/local/path", "tok").unwrap(),
            CredentialStatus::NotHttp
        );
    }
}
