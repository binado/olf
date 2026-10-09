---
name: olf-access
description: Use when editing an Overleaf paper (an olf workspace with .olf/config.toml, or $OLF_PROJECT_DIR) that lives outside your working directory, and writes to it are blocked or you need to find it.
license: MIT
---

# Reaching a paper outside your working directory

The Overleaf checkout often lives in a separate olf workspace, not in the
code repo you were started in. The sandbox only lets you write your working
directory plus directories the user has explicitly allowed.

## Find the paper

- `$OLF_PROJECT_DIR` is the checkout directory. It is set when the user ran
  `grant.py` for your repo (below). Work there using absolute paths.
- Otherwise ask the user where the workspace is (the directory holding
  `.olf/config.toml`). The scripts take `--workspace <dir>` when you aren't
  inside it, e.g. `build.py --workspace <dir>` in the olf-build skill.

## If a write is blocked

Stop and ask the user to do one of:

- run `uv run <this-skill-dir>/scripts/grant.py --workspace <workspace>` from
  this repo (persistent: adds the checkout to
  `permissions.additionalDirectories` and sets `env.OLF_PROJECT_DIR` in this
  repo's `.claude/settings.local.json`, kept out of git via
  `.git/info/exclude`; restart the session afterwards), or
- restart you with `claude --add-dir "<checkout>"` (this session only).

Don't retry the write, copy files into the repo and back, or ask the user to
disable the sandbox. You can't grant yourself access: your own `.claude`
settings are write-protected on purpose, so the human has to run it.

`grant.py` flags: `--workspace DIR` (default: cwd, else `$OLF_PROJECT_DIR`),
`--repo DIR` (default: the git repo containing the cwd). It's a no-op inside
the workspace, refuses to touch invalid JSON, and exits 2 outside a workspace.

Once access works, follow the olf-editing, olf-build and olf-sync skills.
