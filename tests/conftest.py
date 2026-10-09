"""Shared fixtures: a sandboxed HOME, local bare "Overleaf" remotes reached through
`OLF_GIT_BASE`, fake TeX engines on PATH, and git isolated from the developer's config."""

from __future__ import annotations

import importlib.util
import os
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
SKILLS = ROOT / "skills"
INIT = SKILLS / "olf-setup" / "scripts" / "init.py"
BUILD = SKILLS / "olf-build" / "scripts" / "build.py"
GRANT = SKILLS / "olf-access" / "scripts" / "grant.py"
HOOK = SKILLS / "olf-setup" / "assets" / "pre-push"

ID = "64f0c0ffee0123456789abcd"
OTHER_ID = "0123456789abcdef01234567"
MAIN_TEX = "\\documentclass{article}\n\\begin{document}\nHello\n\\end{document}\n"

GIT_ENV = {
    "GIT_CONFIG_GLOBAL": "/dev/null",
    "GIT_CONFIG_NOSYSTEM": "1",
    "GIT_AUTHOR_NAME": "olf test",
    "GIT_AUTHOR_EMAIL": "test@example.com",
    "GIT_COMMITTER_NAME": "olf test",
    "GIT_COMMITTER_EMAIL": "test@example.com",
}


def git(cwd: Path, *args: str, check: bool = True) -> str:
    result = subprocess.run(
        ["git", *args],
        cwd=cwd,
        env={**os.environ, **GIT_ENV},
        capture_output=True,
        text=True,
        stdin=subprocess.DEVNULL,
    )
    if check:
        assert result.returncode == 0, f"git {args} failed: {result.stderr}"
    return result.stdout.rstrip("\n")


class Env:
    def __init__(self, tmp: Path):
        # Canonical, so paths compare equal to what git reports (/private/var on macOS).
        self.path = tmp.resolve()
        self.home.mkdir()
        self.remotes.mkdir()

    @property
    def home(self) -> Path:
        return self.path / "home"

    @property
    def remotes(self) -> Path:
        return self.path / "remotes"

    @property
    def bin(self) -> Path:
        self._bin = self.path / "bin"
        self._bin.mkdir(exist_ok=True)
        return self._bin

    def fake_tool(self, name: str, script: str, directory: Path | None = None) -> Path:
        path = (directory or self.bin) / name
        path.write_text(f"#!/bin/sh\n{script}\n")
        path.chmod(0o755)
        return path

    def stub_path(self, **stubs: str) -> str:
        """A PATH holding only the given stub scripts."""
        directory = self.path / "stub-bin"
        directory.mkdir(exist_ok=True)
        for name, body in stubs.items():
            self.fake_tool(name, body, directory)
        return str(directory)

    def environ(self, **extra: str | None) -> dict[str, str]:
        env = {k: v for k, v in os.environ.items() if not k.startswith(("OLF_", "OVERLEAF_"))}
        env.pop("CLAUDE_CONFIG_DIR", None)
        env.update(
            HOME=str(self.home),
            # Keep the developer's global git ignore out of `check-ignore`.
            XDG_CONFIG_HOME=str(self.home / ".config"),
            PATH=f"{self.bin}:{os.environ['PATH']}",
            OLF_GIT_BASE=str(self.remotes),
            **GIT_ENV,
        )
        for key, value in extra.items():
            if value is None:
                env.pop(key, None)
            else:
                env[key] = value
        return env

    def run(
        self, script: Path, *args: str | Path, cwd: Path | None = None, **extra: str | None
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(script), *map(str, args)],
            cwd=cwd or self.path,
            env=self.environ(**extra),
            capture_output=True,
            text=True,
            stdin=subprocess.DEVNULL,
        )

    def init(self, *args: str | Path, **kw) -> subprocess.CompletedProcess[str]:
        return self.run(INIT, *args, **kw)

    def build(self, *args: str | Path, **kw) -> subprocess.CompletedProcess[str]:
        return self.run(BUILD, *args, **kw)

    def grant(self, *args: str | Path, **kw) -> subprocess.CompletedProcess[str]:
        return self.run(GRANT, *args, **kw)

    def remote(self, project_id: str, files: dict[str, str], branch: str = "master") -> Path:
        """A bare remote for `project_id` whose `branch` holds `files`."""
        remote = self.remotes / project_id
        remote.mkdir(parents=True)
        git(remote, "init", "--quiet", "--bare", "-b", branch)
        seed = self.path / f"seed-{project_id}"
        seed.mkdir()
        git(seed, "init", "--quiet", "-b", branch)
        for name, content in files.items():
            (seed / name).parent.mkdir(parents=True, exist_ok=True)
            (seed / name).write_text(content)
        git(seed, "add", ".")
        git(seed, "commit", "--quiet", "-m", "init")
        git(seed, "push", "--quiet", str(remote), branch)
        shutil.rmtree(seed)
        return remote

    def workspace(self) -> Path:
        """`init.py --id ID ws` against a remote holding a minimal document."""
        self.remote(ID, {"main.tex": MAIN_TEX})
        result = self.init("--id", ID, "ws")
        assert result.returncode == 0, result.stderr
        return self.path / "ws"

    def code_repo(self) -> Path:
        """A plain git repo standing in for the paper's code repo."""
        code = self.path / "code"
        code.mkdir()
        git(code, "init", "--quiet", "-b", "main")
        return code

    def clone_remote(self, project_id: str, name: str) -> Path:
        git(self.path, "clone", "--quiet", str(self.remotes / project_id), name)
        return self.path / name


@pytest.fixture
def env(tmp_path: Path) -> Env:
    return Env(tmp_path)


def load_script(path: Path):
    """Import a skill script as a module (they are single files, not packages)."""
    spec = importlib.util.spec_from_file_location(path.stem, path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module
