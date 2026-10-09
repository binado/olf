#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Persistently let Claude Code in a code repo write the paper checkout.

Run this yourself, not through the agent: under the sandbox an agent cannot edit its own
``.claude`` settings, which is intended.

It adds the checkout to ``permissions.additionalDirectories`` and sets ``env.OLF_PROJECT_DIR``
in the repo's ``.claude/settings.local.json``, and keeps that file out of git via
``.git/info/exclude``.

Exit codes: 0 ok, 1 error, 2 not inside an olf workspace.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tomllib
from pathlib import Path

SETTINGS = ".claude/settings.local.json"
BLOCK_START = "# >>> olf (managed by olf; edits inside are overwritten)"
BLOCK_END = "# <<< olf"


class OlfError(Exception):
    """A failure with the process exit code to report it with."""

    def __init__(self, message: str, code: int = 1) -> None:
        super().__init__(message)
        self.code = code


def git(cwd: Path, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    """Run git in ``cwd``; with ``check``, a failure raises ``OlfError``."""
    try:
        result = subprocess.run(
            ["git", *args], cwd=cwd, capture_output=True, text=True, stdin=subprocess.DEVNULL
        )
    except FileNotFoundError:
        raise OlfError(
            "git not found on PATH\nhint: install git from https://git-scm.com", 6
        ) from None
    if check and result.returncode != 0:
        raise OlfError(f"`git {' '.join(args)}` failed:\n{result.stderr.strip()}")
    return result


def discover(workspace: str | None = None) -> tuple[Path, Path]:
    """Find the workspace: ``--workspace``, else the cwd, else ``$OLF_PROJECT_DIR``.

    Returns
    -------
    tuple[Path, Path]
        The workspace root and its checkout directory.

    Raises
    ------
    OlfError
        With code 2 if no start directory is inside a workspace.
    """
    starts = [Path(workspace)] if workspace else [Path.cwd()]
    if not workspace and os.environ.get("OLF_PROJECT_DIR"):
        starts.append(Path(os.environ["OLF_PROJECT_DIR"]))
    for start in starts:
        start = Path(os.path.abspath(start))
        for directory in (start, *start.parents):
            config = directory / ".olf" / "config.toml"
            if config.is_file():
                try:
                    project_dir = tomllib.loads(config.read_text()).get("project_dir", "paper")
                except tomllib.TOMLDecodeError as e:
                    raise OlfError(f"invalid {config}: {e}") from None
                return directory, Path(os.path.normpath(directory / project_dir))
    raise OlfError(
        f"not inside an olf workspace (no .olf/config.toml in {starts[0]} or its parents)\n"
        "hint: pass --workspace <dir> or set OLF_PROJECT_DIR",
        2,
    )


def ensure_exclude_block(repo: Path, lines: list[str]) -> None:
    """Make ``lines`` olf's block in the repo's ``info/exclude``, leaving other patterns alone."""
    out = git(repo, "rev-parse", "--git-path", "info/exclude").stdout.strip()
    path = Path(out) if os.path.isabs(out) else repo / out
    existing = path.read_text() if path.exists() else ""
    block = "".join(f"{line}\n" for line in [BLOCK_START, *lines, BLOCK_END])
    start, end = existing.find(BLOCK_START), existing.find(BLOCK_END)
    if start != -1 and start < end:
        end += len(BLOCK_END)
        updated = existing[:start] + block + existing[end:].removeprefix("\n")
    elif not existing or existing.endswith("\n"):
        updated = existing + block
    else:
        updated = f"{existing}\n{block}"
    if updated != existing:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(updated)


def canonical(path: str | Path) -> Path:
    return Path(path).resolve()


def update_settings(repo: Path, checkout: Path) -> bool:
    """Add ``checkout`` to the repo's local Claude settings, preserving everything else.

    Returns
    -------
    bool
        Whether the file changed.

    Raises
    ------
    OlfError
        If the file is not a JSON object of the expected shape; it is left untouched.
    """
    path = repo / SETTINGS
    try:
        data = json.loads(path.read_text())
    except FileNotFoundError:
        data = {}
    except json.JSONDecodeError as e:
        raise OlfError(f"{path} isn't valid JSON ({e}); left it alone") from None
    if not isinstance(data, dict):
        raise OlfError(f"{path} isn't a JSON object; left it alone")

    permissions = data.setdefault("permissions", {})
    if not isinstance(permissions, dict):
        raise OlfError(f"`permissions` in {path} isn't an object; left it alone")
    dirs = permissions.setdefault("additionalDirectories", [])
    if not isinstance(dirs, list):
        raise OlfError(
            f"`permissions.additionalDirectories` in {path} isn't an array; left it alone"
        )
    env = data.setdefault("env", {})
    if not isinstance(env, dict):
        raise OlfError(f"`env` in {path} isn't an object; left it alone")

    wanted = canonical(checkout)
    changed = False
    if not any(isinstance(d, str) and canonical(d) == wanted for d in dirs):
        # Canonical, so `../ws` style paths from the command line are stored clean.
        dirs.append(str(wanted))
        changed = True
    if env.get("OLF_PROJECT_DIR") != str(wanted):
        env["OLF_PROJECT_DIR"] = str(wanted)
        changed = True
    if changed:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n")
    return changed


def run(args: argparse.Namespace) -> None:
    root, checkout = discover(args.workspace)
    start = Path(os.path.abspath(args.repo or Path.cwd()))
    is_git = start.is_dir() and git(
        start, "rev-parse", "--is-inside-work-tree", check=False
    ).stdout.startswith("true")
    # Claude Code's project settings live at the repo root.
    repo = Path(git(start, "rev-parse", "--show-toplevel").stdout.strip()) if is_git else start

    if canonical(repo).is_relative_to(canonical(root)):
        print(f"{repo} is inside the workspace; nothing to grant")
        return

    if update_settings(repo, checkout):
        print(f"granted Claude in {repo} write access to {checkout}")
    else:
        print(f"Claude in {repo} already has access to {checkout}")

    if is_git:
        ignored = git(repo, "check-ignore", "-q", "--", SETTINGS, check=False)
        if ignored.returncode not in (0, 1):
            raise OlfError(f"`git check-ignore` failed:\n{ignored.stderr.strip()}")
        if ignored.returncode == 1:
            ensure_exclude_block(repo, [f"/{SETTINGS}"])
            print(f"excluded {SETTINGS} in {repo} via .git/info/exclude")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Persistently let Claude Code in a repo write the Overleaf checkout."
    )
    parser.add_argument("--repo", help="repo the agent runs in (default: the cwd's git repo)")
    parser.add_argument("--workspace", help="workspace directory (default: cwd, $OLF_PROJECT_DIR)")
    try:
        run(parser.parse_args())
    except OlfError as e:
        print(f"error: {e}", file=sys.stderr)
        sys.exit(e.code)


if __name__ == "__main__":
    main()
