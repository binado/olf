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

- `$OLF_PROJECT_DIR` is set when you were launched with `olf exec`.
- Otherwise run `olf -p <project> path` (`olf list` shows registered
  projects). Work in that directory using absolute paths.

## If a write is blocked

Stop and ask the user to do one of:

- `olf grant claude` (persistent; run from this repo), or
- restart you with `olf exec -- claude` (this session only).

Don't retry the write, copy files into the repo and back, or ask the user to
disable the sandbox. You can't grant yourself access: your own `.claude`
settings are write-protected on purpose, so the human has to run it.

Once access works, follow the olf-editing, olf-build and olf-sync skills.
