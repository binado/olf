# olf — Overleaf projects for agents

`olf` is a set of [Agent Skills](https://agentskills.io) that make a local git
checkout of an [Overleaf](https://www.overleaf.com) project safe and
convenient for coding agents (and pleasant for humans). Each skill bundles the
small helper script it needs; there is nothing to compile or put on `PATH`.

| Skill | What it teaches / ships |
|---|---|
| `olf-setup` | `scripts/init.py`: clone or adopt a project, rebase-on-pull, a pre-push hook guarding Overleaf's single branch, token handling |
| `olf-build` | `scripts/build.py`: compile locally with latexmk/tectonic into `.olf/build/`, keeping each run's timestamped logs; a build-fix loop |
| `olf-access` | `scripts/grant.py`: let an agent running in your *code* repo write the paper checkout |
| `olf-sync` | git discipline with co-authors editing in the browser |
| `olf-editing` | editing manners: small diffs, stable labels, no reformatting |

Day-to-day `git pull` / `commit` / `push` stay plain git. Existing hooks are
preserved, and Git hooks can be bypassed, so the guard is a convenience rather
than a guarantee.

## Install

```sh
npx skills add binado/olf -g   # install globally for your agents
```

The [skills CLI](https://github.com/vercel-labs/skills) lets you choose which
agents to install for; omit `-g` to install into the current project.

Requirements: [`uv`](https://docs.astral.sh/uv/) (the scripts are stdlib-only
Python >= 3.11, so `python3 script.py` works too), `git`, and for building
either `latexmk` (TeX Live / MacTeX, preferred because Overleaf uses it) or
[tectonic](https://tectonic-typesetting.github.io). Linux and macOS only.

## Quickstart

1. On overleaf.com, create a git token: Account Settings → Git Integration.
2. Create a workspace (outside any other git repo), or ask your agent to
   "set up this Overleaf project" with the `olf-setup` skill:

   ```sh
   export OVERLEAF_GIT_TOKEN=<token>   # --token can be used for init.py only
   uv run ~/.agents/skills/olf-setup/scripts/init.py \
     --url https://www.overleaf.com/project/<id> ~/papers/dark-matter
   ```

   (The skills CLI reports where it installed the skills; adjust the path.)
   The token is not saved. The checkout's Git credential helper reads
   `OVERLEAF_GIT_TOKEN` from the environment when you pull or push. For Claude
   Code sessions, add the variable to `.claude/settings.local.json` in the
   directory where you launch Claude:

   ```json
   { "env": { "OVERLEAF_GIT_TOKEN": "<your-token>" } }
   ```

3. Work as usual:

   ```sh
   cd ~/papers/dark-matter
   uv run <skills>/olf-build/scripts/build.py   # PDF in .olf/build/
   git -C paper pull && git -C paper push
   open https://www.overleaf.com/project/<id>   # the project on overleaf.com
   ```

Already have an Overleaf clone? Run `init.py` inside it to adopt it as the
workspace.

### Working on code and paper together

When the paper lives in its own workspace, an agent started in your code repo
can't write it (the sandbox only allows its working directory). Either:

```sh
cd ~/code/my-analysis
uv run <skills>/olf-access/scripts/grant.py --workspace ~/papers/dark-matter  # persistent
claude --add-dir "$OLF_PROJECT_DIR"   # or: one session only, with the checkout path
```

`grant.py` adds the checkout to `permissions.additionalDirectories` and sets
`env.OLF_PROJECT_DIR` in the repo's `.claude/settings.local.json`, and keeps
that file out of git via `.git/info/exclude`. Run it yourself: under the
sandbox an agent cannot edit its own `.claude` settings, which is intended.

## Workspace layout

```
~/papers/dark-matter/   # workspace
  .olf/
    config.toml         # project_id, project_dir, [build] main/compiler/engine
    build/              # build output, never inside the checkout
  paper/                # the Overleaf git checkout
```

The scripts find the workspace by walking up from `--workspace <dir>`, else the
current directory, else `$OLF_PROJECT_DIR`. Re-running `init.py` repairs the
setup and never overwrites values you edited in `.olf/config.toml`.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | success |
| 1 | generic error |
| 2 | not inside an olf workspace, or invalid command-line arguments |
| 3 | Overleaf rejected the git token (`init.py`) |
| 4 | build failed (`build.py`) |
| 6 | required tool (git, latexmk, tectonic) missing |
| 7 | `init.py` target is inside a non-Overleaf git repo |

## Migrating from the 0.3 CLI

| Before | Now |
|---|---|
| `olf init …` | `uv run <olf-setup>/scripts/init.py …` (`--force`, `--grant` removed) |
| `olf build [--json]` | `uv run <olf-build>/scripts/build.py [--json]` |
| `olf grant claude` | `uv run <olf-access>/scripts/grant.py` (also sets `OLF_PROJECT_DIR`) |
| `olf exec -- claude` | `claude --add-dir "$OLF_PROJECT_DIR"` |
| `olf path` | `$OLF_PROJECT_DIR` |
| `olf open` | `open https://www.overleaf.com/project/<id>` |
| `olf list`, `-p`, `olf edit` | removed (no project registry); use `--workspace <dir>` |

Existing workspaces keep working. The old `~/.olf/projects` symlinks are inert
and can be deleted.

## Development

```sh
just fmt        # ruff format
just fmt-check
just lint       # ruff check + ty
just test       # pytest (the real-TeX test skips without latexmk)
```

Install [just](https://github.com/casey/just) and [uv](https://docs.astral.sh/uv/).
Skills must stay self-contained (installers copy one skill directory), so
small helpers such as workspace discovery are intentionally duplicated per
script instead of shared.

## License

Licensed under the [MIT license](https://github.com/binado/olf/blob/main/LICENSE-MIT).
