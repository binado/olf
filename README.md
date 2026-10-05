# olf — Overleaf projects for agents

`olf` makes a local git checkout of an [Overleaf](https://www.overleaf.com)
project safe and convenient for coding agents (and pleasant for humans):

- `olf init` clones the project, configures pulls to rebase, and installs a
  pre-push hook to guard Overleaf's single branch and reject non-fast-forward
  pushes and branch deletion.
- `olf build` compiles locally into `.olf/build/`, keeping each run's logs
  (timestamped) in `.olf/build/logs/` for the agent to read.
- `olf list`, `olf path`, `olf edit` and the global `-p <project>` flag
  find your papers from anywhere.
- Agent skills in `skills/` teach the workflow:
  sync discipline with co-authors editing in the browser, editing manners,
  a build-fix loop, and what to do when the paper is outside the sandbox.
- `olf grant` / `olf exec` let an agent running in your *code* repo write
  the paper checkout, which lives in a separate workspace.

Day-to-day `git pull` / `commit` / `push` stay plain git. Existing hooks are
preserved, and Git hooks can be bypassed, so the guard is a convenience rather
than a guarantee.

## Install

```sh
cargo install olf --locked
```

Requirements: `git`, and for `olf build` either `latexmk` (TeX Live / MacTeX,
preferred because Overleaf uses it) or [tectonic](https://tectonic-typesetting.github.io).

Supported platforms: Linux and macOS. Windows support is experimental and
has no CI coverage. Building from source requires Rust 1.85 or newer.

## Quickstart

1. On overleaf.com, create a git token: Account Settings → Git Integration.
2. Create a workspace (outside any other git repo):

   ```sh
   export OVERLEAF_GIT_TOKEN=<token>   # --token can be used for init only
   olf init --url https://www.overleaf.com/project/<id> ~/papers/dark-matter
   ```

   `olf` does not save the token. The checkout's Git credential helper reads
   `OVERLEAF_GIT_TOKEN` from the environment when you pull or push. For Claude
   Code sessions, add the variable to `.claude/settings.local.json` in the
   directory where you launch Claude:

   ```json
   {
     "env": {
       "OVERLEAF_GIT_TOKEN": "<your-token>"
     }
   }
   ```

3. Install the skills, then work as usual:

   ```sh
   npx skills add binado/olf -g  # install globally for your agents
   cd ~/papers/dark-matter
   olf build                    # PDF in .olf/build/
   git -C paper pull && git -C paper push
   olf open                     # the project on overleaf.com
   ```

   The skills are plain [Agent Skills](https://agentskills.io)
   (`skills/<name>/SKILL.md` in this repo). The [skills CLI](https://github.com/vercel-labs/skills)
   lets you choose which agents to install them for; omit `-g` to install
   into the current project instead.

Already have an Overleaf clone? `olf init` inside it adopts it as the
workspace.

## Commands

| Command | What it does |
|---|---|
| `olf init` | clone/adopt a project, configure it, register it in the index |
| `olf build [--json]` | compile locally into `.olf/build/` (`--json` for machine output) |
| `olf list [--json] [--prune]` | registered projects: ID, path, link status (`--json`); `--prune` removes links to deleted workspaces |
| `olf path` | print the checkout directory |
| `olf edit` | open the checkout in `$VISUAL`, else `$EDITOR` |
| `olf grant <agent> [--repo <dir>]` | persistently allow an agent in a repo to write the checkout (`claude`) |
| `olf exec -- <agent> [args...]` | launch an agent with checkout access for one session |
| `olf open [--print]` | the project on overleaf.com |

### Working on any project from anywhere

`olf init` registers each workspace as a symlink in `~/.olf/projects/<id>`
(set `OLF_HOME` to use `$OLF_HOME/projects` instead). The global `-p` flag
accepts a full project ID, a unique ID prefix, or the workspace directory
name, and runs the command in that workspace:

```sh
olf -p dark build
olf -p 64f0c0 path
```

### Working on code and paper together

When the paper lives in its own workspace, an agent started in your code
repo can't write it (the sandbox only allows its working directory). Either:

```sh
cd ~/code/my-analysis
olf -p dark grant claude        # persistent: edits .claude/settings.local.json
olf -p dark exec -- claude      # one session: passes --add-dir <checkout>
olf init --url <url> ../paper-ws --grant claude   # set up and grant at once
```

`grant` adds the checkout to `permissions.additionalDirectories` in the
repo's `.claude/settings.local.json` and keeps that file out of git via
`.git/info/exclude`. `exec` also sets `OLF_WORKSPACE` and `OLF_PROJECT_DIR`,
and runs the agent in your current directory. Run `grant` yourself: under the
sandbox an agent cannot edit its own `.claude` settings, which is intended.

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
| 1 | generic error |
| 2 | not inside an olf workspace, or invalid command-line arguments |
| 3 | Overleaf rejected the git token |
| 4 | build failed |
| 6 | required tool (git, latexmk, tectonic, or an agent) missing |
| 7 | `init` target is inside a non-Overleaf git repo |
| 8 | `-p` matched no registered project, or several |

Argument parsing uses Clap's standard exit codes: help and version requests
return 0, and invalid arguments return 2. `olf exec` returns the launched
agent's exit status.

## Development

Install from a local checkout:

```sh
cargo install --path . --locked
```

Install [just](https://github.com/casey/just), then run:

```sh
just fmt
just fmt-check
just lint
just test
```

Pedantic Clippy lints are enabled. The real-TeX test skips without `latexmk`.

## Releasing

[release-plz](https://release-plz.dev/docs/config) manages the version,
`Cargo.lock`, changelog, crates.io publication, and GitHub releases using
`release-plz.toml`. Tags and GitHub releases use `v<version>`.

From a clean checkout of `main`, open a release PR:

```sh
release-plz release-pr --git-token "$GITHUB_TOKEN"
```

Review the generated version and changelog, and run these checks on the
release PR branch:

```sh
just release-check
```

This runs formatting, linting, and tests, lists the packaged files, and runs
`cargo publish --dry-run`. Review the file list and resolve any warnings
before merging the release PR. Then, from an updated, clean checkout of
`main`, publish the release:

```sh
release-plz release --git-token "$GITHUB_TOKEN"
```

Provide a GitHub token in `GITHUB_TOKEN` and a crates.io token in
`CARGO_REGISTRY_TOKEN`. The configuration sets `release_always = false`, so
publication happens only after merging a release PR. These commands are
run manually; CI currently validates packages without publishing them.

## License

Licensed under the [MIT license](https://github.com/binado/olf/blob/main/LICENSE-MIT).
