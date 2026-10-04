# olf — Overleaf projects for agents

`olf` makes a local git checkout of an [Overleaf](https://www.overleaf.com)
project safe and convenient for coding agents (and pleasant for humans):

- `olf init` clones the project and configures it so plain `git` can't break
  Overleaf's rules (a single branch, linear history, no force pushes).
- `olf build` compiles locally into `.olf/build/`, keeping each run's logs
  (timestamped) in `.olf/build/logs/` for the agent to read.
- `olf list`, `olf path`, `olf edit` and the global `-p <project>` flag
  find your papers from anywhere; `olf fmt` runs
  [tex-fmt](https://github.com/WGUNDERWOOD/tex-fmt) on changed files (opt-in).
- `olf skill install` installs agent skills that teach the workflow:
  sync discipline with co-authors editing in the browser, editing manners,
  and a build-fix loop.

Day-to-day `git pull` / `commit` / `push` stay plain git. See
[PLAN.md](PLAN.md) for the full design.

## Install

```sh
cargo install --path .
```

Requirements: `git`, and for `olf build` either `latexmk` (TeX Live / MacTeX,
preferred because Overleaf uses it) or [tectonic](https://tectonic-typesetting.github.io).

## Quickstart

1. On overleaf.com, create a git token: Account Settings → Git Integration.
2. Create a workspace (outside any other git repo):

   ```sh
   export OVERLEAF_GIT_TOKEN=<token>   # or pass --token
   olf init --url https://www.overleaf.com/project/<id> ~/papers/dark-matter
   ```

   The token goes into your git credential helper (the Keychain on macOS),
   never into `.git/config` or olf's config.

3. Install the skills for Claude Code, then work as usual:

   ```sh
   olf skill install            # ~/.claude/skills/ (or --project)
   cd ~/papers/dark-matter
   olf build                    # PDF in .olf/build/
   git -C paper pull && git -C paper push
   olf open                     # the project on overleaf.com
   ```

Already have an Overleaf clone? `olf init` inside it adopts it as the
workspace.

## Commands

| Command | What it does |
|---|---|
| `olf init` | clone/adopt a project, configure it, register it in the index |
| `olf build` | compile locally into `.olf/build/` (`--json` for machine output) |
| `olf list [--prune]` | registered projects: ID, path, link status (`--json`); `--prune` removes links to deleted workspaces |
| `olf path` | print the checkout directory |
| `olf edit` | open the checkout in `$VISUAL`, else `$EDITOR` |
| `olf fmt [--all] [--check] [paths…]` | run `tex-fmt` on changed `.tex` files; refuses unless `fmt.enabled = true` in `.olf/config.toml` (whole-file reformatting bloats Overleaf history and conflicts with co-authors). A `tex-fmt.toml` in the checkout takes precedence over olf's defaults. Needs `tex-fmt` on PATH |
| `olf open [--print]` | the project on overleaf.com |
| `olf skill install` | install the bundled agent skills |

### Working on any project from anywhere

`olf init` registers each workspace as a symlink in `~/.olf/projects/<id>`
(set `OLF_HOME` to use `$OLF_HOME/projects` instead). The global `-p` flag
accepts a full project ID, a unique ID prefix, or the workspace directory
name, and runs the command in that workspace:

```sh
olf -p dark build
olf -p 64f0c0 path
```

One workspace per project ID per machine: initialising the same project
elsewhere fails unless you pass `olf init --force`, which relinks the index.
Use `-C <dir>` instead when you have a directory rather than a project.

**Upgrading from v0.1:** workspaces created by v0.1 aren't in the index yet;
re-run `olf init` in each one to register it.

## Workspace layout

```
~/papers/dark-matter/   # workspace
  .olf/
    config.toml         # project_id, project_dir, [build], [fmt]
    build/              # build output, never inside the checkout
  paper/                # the Overleaf git checkout
```

`olf` commands work from the workspace root or anywhere inside it; `-C <dir>`
runs as if started in `<dir>`. Re-running `olf init` repairs the setup and
never overwrites values you edited in `.olf/config.toml`.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | success |
| 1 | generic error / usage |
| 2 | not inside an olf workspace |
| 3 | Overleaf rejected the git token |
| 4 | build failed |
| 5 | `olf fmt --check` found unformatted files |
| 6 | required tool (git, latexmk, tectonic) missing |
| 7 | `init` target is inside a non-Overleaf git repo |
| 8 | `-p` matched no registered project, or several |

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings   # pedantic lints are on
cargo test                                  # the real-TeX test skips without latexmk
```
