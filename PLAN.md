# olf — Overleaf projects for agents

`olf` is a CLI for working on Overleaf projects from a local git checkout. It
targets coding agents first (simple, non-interactive commands with concise
output) while staying pleasant for humans.

Plain `git clone` already works against Overleaf's git bridge, so the CLI's job
is to add the conveniences and guardrails the bridge needs. The main
differentiator is a set of **agent skills** that encode the workflow an agent
would not know on its own.

## Background: the Overleaf git bridge

- Clone URL: `https://git.overleaf.com/<project-id>`; auth with username `git`
  and an Overleaf git token as password.
- Single branch (`master`), linear history, no force pushes.
- Collaborators may be editing in the browser concurrently: pushes get rejected
  when the remote moved ahead, and large diffs cause painful conflicts.
- Everything pushed shows up as a file in the Overleaf project tree.
- Project settings (main document, compiler, TeX Live version) live in
  Overleaf, not in git.
- There is no public REST API. v1 stays git-only.

## Workspace layout

`olf` manages a **workspace**: any directory holding olf's state plus the
Overleaf checkout. The workspace may itself be the user's own git repo (e.g.
the paper's analysis code), so olf must leave **no tracked footprint** there.

```
workspace/              # any dir; possibly the user's own git repo
  .olf/
    config.toml         # olf config
    build/              # build output, never inside the checkout
  paper/                # Overleaf git checkout (`project_dir`)
```

- The checkout is **visible** (not under `.olf/`): ripgrep and agent search
  tools skip dot-directories by default, and the paper is what gets edited.
- The checkout lives **inside** the workspace, not in a central `~/.olf/`:
  agent sandboxes only allow free writes in the working directory, cloud
  sessions don't persist `~`, search is rooted at the cwd, and a shared
  checkout would let concurrent sessions trample each other.
- `olf` finds the workspace by walking up from the cwd looking for
  `.olf/config.toml`, so commands work from the root or inside `paper/`.

### Configuration: `.olf/config.toml`

```toml
project_id = "64f0c0ffee..."
project_dir = "paper"      # relative to workspace; absolute path allowed (warns)

[build]
main = "main.tex"          # auto-detected by `olf init` (file with \documentclass)
compiler = "pdflatex"      # pdflatex | xelatex | lualatex — mirror Overleaf's setting
engine = "auto"            # auto | latexmk | tectonic

[fmt]
enabled = false            # opt-in per project, see `olf fmt`
wrap = false
```

The token is **never** stored in the config or the git remote URL.

### When the workspace is a git repo

The Overleaf checkout must remain its own repo (the bridge needs its own
linear `master`), and the user most likely doesn't want olf files committed to
their repo. `olf init` detects an enclosing repo and:

- Appends `/.olf/` and `/paper/` to that repo's **`.git/info/exclude`**
  (local-only, never committed; resolve the path with
  `git rev-parse --git-path info/exclude` so worktrees work). This also stops
  `git add .` from recording `paper/` as an embedded repository.
- Writes a workspace-root **`.ignore`** containing `!/paper/` so ripgrep-based
  tools still search the paper (`.ignore` takes precedence over git ignores and
  git itself does not read it). The `.ignore` file is itself added to
  `info/exclude`. If a tracked `.ignore` already exists, don't modify it — warn
  instead. **Needs verification** against Claude Code's Grep/Glob tools.
- Opt-in alternative for users who *do* want the outer repo to pin paper
  versions (e.g. tag code + paper at submission): `olf init --submodule`.
  Not v1.

### Adopting an existing clone

Running `olf init` inside an existing Overleaf clone makes the clone itself the
workspace with `project_dir = "."`. In that case `.olf/` sits inside the
Overleaf repo and is added to *its* `.git/info/exclude`, so it never reaches
Overleaf.

## Commands

### `olf init [--id <id> | --url <overleaf-url>] [dir]`

One command to create or repair a workspace, cloning if needed:

- **With `--id`/`--url`**: create the workspace at `dir` (default: cwd) and
  clone into `dir/<project_dir>`. Accepts any Overleaf project URL.
- **Without**: if `.olf/config.toml` exists but the checkout is missing, clone
  it; if `dir` is an existing Overleaf clone, adopt it (see above).
- **Idempotent**: re-running repairs/updates the setup and never overwrites
  user-edited config values. If the checkout belongs to a *different* project,
  error.

Setup steps:

- Token from `--token` or `OVERLEAF_GIT_TOKEN`; stored via a git credential
  helper (Keychain on macOS), not embedded in `.git/config`.
- Write `.olf/config.toml` (auto-detect main file).
- Configure the checkout for the bridge: `pull.rebase=true`; pre-push hook that
  rejects force pushes and non-`master` branches.
- Add stray LaTeX artifacts (`*.aux`, `*.log`, …) to the checkout's
  `.git/info/exclude`, in case someone builds manually inside it.
- Handle an enclosing git repo as described above.

### `olf status`

Fetch and report whether the remote moved ahead (someone edited online) and
whether there are local uncommitted/unpushed changes. Agents run this before
editing.

### `olf sync`

Pull (rebase), then push. On a rejected push, re-pull and retry once. On
conflicts, stop and print the conflicting files clearly instead of attempting
anything clever.

### `olf build`

Compile locally for a fast edit → compile → fix loop.

- Engine selection with `engine = "auto"`: try **latexmk** first (Overleaf
  itself uses latexmk, so results match best), then **tectonic**.
- Warn when falling back to tectonic for a non-XeTeX project, since its engine
  differs.
- Output goes to `.olf/build/`, never the checkout.
- Condense the log into `file:line: message` errors and warnings; full log
  path printed for drill-down.
- Caveat: Overleaf pins a TeX Live version, so a local pass does not guarantee
  an Overleaf pass.

### `olf fmt [--all] [--check]`

Format `.tex` files with [tex-fmt](https://github.com/WGUNDERWOOD/tex-fmt).

- Refuses to run unless `fmt.enabled = true` — whole-file reformatting in a
  shared project bloats Overleaf history and conflicts with co-authors.
- Default: only `.tex` files changed in the working tree. `--all`: whole
  project (for the one-time "format everything" commit).
- `--check`: report without writing (usable before push / in the build loop).
- Options come from `.olf/config.toml`; if the project has a `tex-fmt.toml`,
  defer to it instead.
- If `tex-fmt` is not on PATH, fail with install hints.

### `olf edit`

Open the checkout in `$VISUAL` (falling back to `$EDITOR`).

### `olf open`

Open `https://www.overleaf.com/project/<id>` in the browser.

### `olf skill install [--project]`

Install the bundled skills. Default: user-level (`~/.claude/skills/`) — skills
are read-only, so living outside the workspace is fine and keeps the user's
repo clean. `--project`: install into the workspace's `.claude/skills/` and add
those paths to `info/exclude`. `olf skill list` shows what is bundled.

## API (Rust + clap)

Implemented in Rust with clap's derive API. Agent-oriented conventions:

- Global `-C <dir>` (like `git -C`) so agents never need to `cd`.
- Global `--json` for machine-readable output on every command.
- Distinct exit codes so agents can branch on outcomes without parsing text.
- `sync` never commits: agents commit with plain `git` and their own messages;
  `olf` complements git rather than hiding it.

```rust
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// Work on Overleaf projects from a local git checkout.
#[derive(Parser)]
#[command(name = "olf", version, about)]
pub struct Cli {
    /// Run as if olf was started in <dir> (like `git -C`)
    #[arg(short = 'C', global = true, value_name = "DIR")]
    pub dir: Option<PathBuf>,

    /// Emit machine-readable JSON instead of human text
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Clone and/or set up an Overleaf project (idempotent)
    Init(InitArgs),
    /// Show local changes and whether Overleaf moved ahead
    Status(StatusArgs),
    /// Pull (rebase) and push committed work
    Sync(SyncArgs),
    /// Compile locally with latexmk or tectonic
    Build(BuildArgs),
    /// Format .tex files with tex-fmt (requires fmt.enabled)
    Fmt(FmtArgs),
    /// Open the checkout in $VISUAL / $EDITOR
    Edit,
    /// Open the project on overleaf.com
    Open(OpenArgs),
    /// Manage bundled agent skills
    #[command(subcommand)]
    Skill(SkillCommand),
}

#[derive(Args)]
pub struct InitArgs {
    /// Overleaf project ID
    #[arg(long, conflicts_with = "url")]
    pub id: Option<String>,
    /// Any overleaf.com project URL
    #[arg(long)]
    pub url: Option<String>,
    /// Git token (stored in the credential helper, never in config)
    #[arg(long, env = "OVERLEAF_GIT_TOKEN", hide_env_values = true)]
    pub token: Option<String>,
    /// Target directory (default: cwd, or project ID when cloning)
    pub path: Option<PathBuf>,
}

#[derive(Args)]
pub struct StatusArgs {
    /// Skip `git fetch` (offline / fast)
    #[arg(long)]
    pub no_fetch: bool,
}

#[derive(Args)]
pub struct SyncArgs {
    /// Show what would happen without pushing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct BuildArgs {
    /// Override engine from .olf/config.toml
    #[arg(long, value_enum)]
    pub engine: Option<Engine>,
    /// Override main file from .olf/config.toml
    #[arg(long)]
    pub main: Option<PathBuf>,
    /// Also print warnings (overfull boxes, undefined refs, ...)
    #[arg(long)]
    pub warnings: bool,
}

#[derive(Args)]
pub struct FmtArgs {
    /// Format all .tex files, not only changed ones
    #[arg(long, conflicts_with = "paths")]
    pub all: bool,
    /// Report unformatted files without writing; non-zero exit if any
    #[arg(long)]
    pub check: bool,
    /// Explicit files to format
    pub paths: Vec<PathBuf>,
}

#[derive(Args)]
pub struct OpenArgs {
    /// Print the URL instead of opening a browser
    #[arg(long)]
    pub print: bool,
}

#[derive(Subcommand)]
pub enum SkillCommand {
    /// Install skills (default: ~/.claude/skills/)
    Install {
        /// Install into the workspace's .claude/skills/ instead
        #[arg(long)]
        project: bool,
        /// Overwrite existing skill files
        #[arg(long)]
        force: bool,
    },
    /// List bundled skills
    List,
}

#[derive(Clone, Copy, ValueEnum, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    Auto,
    Latexmk,
    Tectonic,
}
```

### Exit codes

```rust
#[repr(u8)]
pub enum Exit {
    Ok = 0,
    Error = 1,       // generic / usage
    NotAProject = 2, // no .olf/config.toml found
    Conflict = 3,    // sync hit merge conflicts
    Rejected = 4,    // push rejected after retry
    BuildFailed = 5,
    Unformatted = 6, // fmt --check found diffs
    MissingTool = 7, // latexmk / tectonic / tex-fmt not on PATH
}
```

### Implementation notes

- Shell out to the `git` binary rather than using `git2`: libgit2 does not
  run git's credential helpers or hooks the same way, and both the Keychain
  token storage and the pre-push guard depend on them.
- Skills are embedded in the binary with `include_str!`.

### Crate layout

```
src/
  main.rs        // parse, dispatch, map errors → Exit
  cli.rs         // clap structs above
  config.rs      // .olf/config.toml (serde + toml), workspace discovery
  git.rs         // thin wrapper over the `git` subprocess
  overleaf.rs    // URL/ID parsing, project URL
  latex/
    engine.rs    // detect + run latexmk / tectonic
    log.rs       // condense .log → file:line: message
  commands/      // one module per subcommand
skills/          // SKILL.md files, embedded via include_str!
```

Dependencies: `clap` (derive, env), `serde`, `toml`, `serde_json`, `anyhow`
or `thiserror`, `which`, `open`.

## Skills (the differentiator)

1. **Sync discipline** — `olf status` before editing; small, descriptive
   commits; `olf sync` instead of raw push; never force push or branch; re-pull
   after rejection.
2. **Collaborative editing manners** — don't reformat or re-wrap text you did
   not change; preserve `\label`s, macros, and existing style; only run
   `olf fmt` if the project enabled it.
3. **Build-fix loop** — edit → `olf build` → read condensed errors → fix; give
   up and report after N failed attempts instead of thrashing.
4. *(Later)* **Writing tasks** — prose tightening, reference/citation checks,
   venue-specific formatting.

## Non-goals (v1)

- Listing projects, triggering Overleaf compiles, reading Overleaf comments, or
  anything else requiring cookie-based scraping.
- MCP server — the CLI is already agent-usable via the shell; revisit if a
  need appears.
- Shipping a TeX distribution.

## Open questions

- Distribution of skills: `olf skill install` only, or also a Claude Code
  plugin?
- Confirm git integration availability on free Overleaf plans.
- Verify the `.ignore` re-inclusion trick with Claude Code's Grep/Glob and
  other agents' search tools; fallback if it doesn't work.
- `--submodule` mode for outer repos that want to pin paper versions.
- Retry/backoff policy for `olf sync`.
