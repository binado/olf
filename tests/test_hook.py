import os
import shutil
import subprocess

import pytest
from conftest import HOOK, ID, git


def setup_clone(env, branch):
    env.remote(ID, {"main.tex": "hello\n"}, branch=branch)
    clone = env.clone_remote(ID, "clone")
    hook = clone / ".git/hooks/pre-push"
    shutil.copy(HOOK, hook)
    hook.chmod(0o755)
    return clone


def push(clone, *args):
    result = subprocess.run(
        ["git", "push", *args],
        cwd=clone,
        env={**os.environ, "GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_NOSYSTEM": "1"},
        capture_output=True,
        text=True,
    )
    return result


def commit(clone, name):
    (clone / name).write_text(name)
    git(clone, "add", name)
    git(clone, "commit", "--quiet", "-m", name)


def test_hook_is_marked():
    assert "# olf-managed" in HOOK.read_text()


@pytest.mark.parametrize("branch", ["master", "main"])
def test_hook_guards(env, branch):
    clone = setup_clone(env, branch)
    commit(clone, "a.tex")
    assert push(clone, "--quiet").returncode == 0

    other = "main" if branch == "master" else "master"
    for target in ("HEAD:other", f"HEAD:{other}"):
        result = push(clone, "origin", target)
        assert result.returncode != 0
        assert f"only has refs/heads/{branch}" in result.stderr

    git(clone, "reset", "--quiet", "--hard", "HEAD~1")
    commit(clone, "b.tex")
    result = push(clone, "--force", "origin", branch)
    assert result.returncode != 0 and "non-fast-forward" in result.stderr

    result = push(clone, "origin", f":{branch}")
    assert result.returncode != 0 and "refusing to delete" in result.stderr
