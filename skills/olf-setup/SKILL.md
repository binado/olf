---
name: olf-setup
description: Create or repair an olf workspace (a local git checkout of an Overleaf project) with `init.py`. Use when the user wants to start working on an Overleaf project, when a command says it is not inside an olf workspace, when `git pull` refuses to rebase, or when the Overleaf git token is rejected.
license: MIT
---

# Setting up an Overleaf workspace

`scripts/init.py` (next to this file) clones an Overleaf project, configures
it for safe collaboration, and writes `.olf/config.toml`. It needs `uv` (or
plain `python3` >= 3.11: the script is stdlib-only) and `git`.

```sh
uv run <this-skill-dir>/scripts/init.py --url https://www.overleaf.com/project/<id> ~/papers/dark-matter
```

Run it **outside any other git repo**: a workspace inside a code repo is
refused (exit 7). Flags:

| Flag | Meaning |
|---|---|
| `--url URL` / `--id ID` | the project: any `overleaf.com/project/<id>` or `git.overleaf.com/<id>` URL, or the 24-hex ID. Share links (`/read/…`, `/edit/…`) don't contain the ID and are rejected |
| `--token TOKEN` | git token for the clone; defaults to `$OVERLEAF_GIT_TOKEN` |
| `--main FILE` | main `.tex` file relative to the checkout (otherwise `main.tex`, else the only file with `\documentclass`) |
| `[path]` | workspace directory (default: cwd) |

## What it does

- Clones into `<workspace>/paper` (or adopts the current directory when it
  already is an Overleaf clone: then the workspace *is* the checkout and
  `/.olf/`, `/.claude/` are excluded from git).
- Sets `pull.rebase=true` and `pull.ff=true` (so a global `pull.ff=only`
  can't block rebasing onto browser edits), installs a pre-push hook that
  rejects force pushes, deletions and other branches, and hides LaTeX
  artifacts from `git status` via `.git/info/exclude`.
- Configures a credential helper that reads `$OVERLEAF_GIT_TOKEN` from the
  environment. The token is never written to disk or argv.
- Writes `.olf/config.toml` once (`project_id`, `project_dir`, `[build]`
  `main`/`compiler`/`engine`). It never overwrites existing values, so edit
  `compiler` there to match the Overleaf project settings.

It is idempotent. **Re-running it repairs a workspace** (missing checkout,
missing hook, lost git config) from anywhere inside the workspace.

## Token

Overleaf's git bridge needs a token the user creates under Account Settings →
Git Integration on overleaf.com. Never ask for it in chat. Have the user
export `OVERLEAF_GIT_TOKEN` (for Claude Code, in the `env` of
`.claude/settings.local.json` of the directory it is launched from) and run
`init.py` themselves. Exit code 3 means Overleaf rejected the token.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | workspace ready |
| 1 | error (message says what; e.g. `project mismatch`, ambiguous main file → pass `--main`) |
| 3 | Overleaf rejected the git token |
| 6 | `git` not found |
| 7 | the target is inside a non-Overleaf git repo |

## Finding the project later

Open it on overleaf.com: `open https://www.overleaf.com/project/<id>` (the ID
is `project_id` in `.olf/config.toml`). To let an agent in another repo write
the checkout, see the olf-access skill.
