# olf — Overleaf projects for agents

`olf` is a CLI for working on Overleaf projects from a local git checkout. It
targets coding agents first (simple, non-interactive commands with concise
output) while staying pleasant for humans.

Plain `git clone` already works against Overleaf's git bridge, and agents are
fluent in git, so `olf` does **not** wrap day-to-day git (pull, commit, push,
status). Its job is to:

1. **Configure** a checkout so the bridge's rules hold even when agents use
   plain git (`olf init`).
2. Provide what git can't: local **builds**, formatting, and project lookup.
3. Ship **agent skills** that encode the workflow an agent would not know on
   its own — the main differentiator.

## Background: the Overleaf git bridge

- Clone URL: `https://git.overleaf.com/<project-id>`; auth with username `git`
  and an Overleaf git token as password.
- Single branch (`main` on the project used for validation; the remote's
  `HEAD` is authoritative, so older `master` projects work too), linear
  history, no force pushes.
- Collaborators may be editing in the browser concurrently: pushes get rejected
  when the remote moved ahead, and large diffs cause painful conflicts.
- Everything pushed shows up as a file in the Overleaf project tree.
- Project settings (main document, compiler, TeX Live version) live in
  Overleaf, not in git.
- There is no public REST API. v1 stays git-only.

## Workspace layout

`olf` manages a **workspace**: a standalone directory holding olf's state plus
the Overleaf checkout.

```
~/papers/dark-matter/   # workspace (never inside another git repo)
  .olf/
    config.toml         # olf config
    build/              # build output, never inside the checkout
  paper/                # Overleaf git checkout (`project_dir`)
```

- The checkout is **visible** (not under `.olf/`): ripgrep and agent search
  tools skip dot-directories by default, and the paper is what gets edited.
- The checkout lives **inside** the workspace, so an agent started there can
  read, search and write it without extra permissions.
- `olf` finds the workspace by walking up from the cwd looking for
  `.olf/config.toml`, so commands work from the root or inside `paper/`.
  From anywhere else, use `-p <project>` (resolved through the index below).

### Configuration: `.olf/config.toml`

```toml
project_id = "64f0c0ffee..."
project_dir = "paper"      # relative to workspace

[build]
main = "main.tex"          # auto-detected by `olf init` (file with \documentclass)
compiler = "pdflatex"      # pdflatex | xelatex | lualatex — mirror Overleaf's setting
engine = "auto"            # auto | latexmk | tectonic

[fmt]
enabled = false            # opt-in per project, see `olf fmt`
wrap = false
```

The token is **never** stored in the config or the git remote URL.

### No workspaces inside git repos

`olf init` **refuses** to create a workspace inside an existing git work tree
(checked on the *target* directory with
`git -C <dir> rev-parse --is-inside-work-tree`, not on the cwd). The Overleaf
checkout must be its own repo, and nesting it in e.g. the paper's code repo
would require ignore-file tricks in a repo olf doesn't own. The error message
suggests a sibling location instead.

**Exception — adopting an existing Overleaf clone**: if the enclosing repo's
`origin` is `git.overleaf.com`, the clone itself becomes the workspace with
`project_dir = "."`. `.olf/` (and anything else olf writes there) is added to
that clone's `.git/info/exclude` so it never reaches Overleaf.

### Project index: `~/.olf/projects/`

`olf init` registers each workspace as a symlink, pointing **inward** at the
real workspace:

```
~/.olf/projects/
  64f0c0ffee -> ~/papers/dark-matter
  a1b2c3d4e5 -> ~/thesis
```

- Powers `olf list` and the global `-p <project>` flag (ID, unique ID prefix,
  or workspace directory name).
- Only humans and olf traverse these links; agents work on real paths.
- Dangling links (deleted workspaces) are reported by `olf list` and pruned
  with `olf list --prune`.
- v1: one workspace per project ID per machine; re-running `init` for the same
  ID elsewhere errors unless `--force` relinks.
- Location overridable with `OLF_HOME`.

### Working on code and paper together

When an agent runs in the paper's *code* repo and must also edit the paper, the
paper is outside its workspace and sandboxed writes are blocked. The user is
opting in, so the goal is to grant **exactly the paper directory, once**,
instead of repeated prompts or disabling the sandbox:

- `olf grant <agent>`: persist access in the current repo's agent settings.
- `olf init --grant <agent>`: same, right after init (target is the new
  workspace, settings go to the cwd's repo).
- `olf exec -- <agent> [args]`: grant for a single session.
- `olf path`: raw path for anything else (`claude --add-dir "$(olf path)"`).

## Commands

Global flags: `-C <dir>`, `-p/--project <project>`, `--json` (supported by
`build` and `list`; other commands ignore it until a schema is needed).

### `olf init [--id <id> | --url <overleaf-url>] [--grant <agent>] [dir]`

One command to create or repair a workspace, cloning if needed:

- **With `--id`/`--url`**: create the workspace at `dir` (default: cwd) and
  clone into `dir/<project_dir>`. Accepts any Overleaf project URL.
- **Without**: if `.olf/config.toml` exists but the checkout is missing, clone
  it; if `dir` is an existing Overleaf clone, adopt it (see above).
- **Idempotent**: re-running repairs/updates the setup and never overwrites
  user-edited config values. If the checkout belongs to a *different* project,
  error.
- Refuses targets inside non-Overleaf git repos (see above).

Setup steps:

- Token from `--token` or `OVERLEAF_GIT_TOKEN`; stored via a git credential
  helper (Keychain on macOS), not embedded in `.git/config`.
- Write `.olf/config.toml`, auto-detecting the main file: prefer a root
  `main.tex`; else the single `.tex` file in any folder with `\documentclass`
  (ignoring `standalone` / `subfiles` classes), or the single such file named
  `main.tex`; otherwise prompt, or fail when non-interactive and ask for
  `--main`.
- Configure the checkout for the bridge, so plain git stays safe:
  `pull.rebase=true` plus `pull.ff=true` (linear history; the latter keeps a
  global `pull.ff=only` from refusing the rebase); pre-push hook that rejects force pushes
  and pushes to any branch other than the remote's `HEAD`. The hook is a
  safety net only (`--no-verify` bypasses it); the sync-discipline skill is
  the primary guardrail.
- Auth failures during clone exit with `AuthFailed` and explain how to create
  or refresh an Overleaf git token.
- Add stray LaTeX artifacts (`*.aux`, `*.log`, …) to the checkout's
  `.git/info/exclude`, in case someone builds manually inside it.
- Register the workspace in `~/.olf/projects/`.
- With `--grant <agent>`: run `olf grant <agent>` for the cwd.

### `olf build`

Compile locally for a fast edit → compile → fix loop.

- Engine selection with `engine = "auto"`: try **latexmk** first (Overleaf
  itself uses latexmk, so results match best), then **tectonic**.
- Warn when falling back to tectonic for a non-XeTeX project, since its engine
  differs.
- Output goes to `.olf/build/`, never the checkout.
- Run the engine from the main file's directory, as Overleaf does (a
  `notes/main.tex` resolves `\input{utils}` to `notes/utils.tex`); report
  paths relative to the checkout.
- No log parsing: success is the engine's exit status. Each run's TeX log and
  engine output are kept as timestamped files in `.olf/build/logs/` (last 20
  builds), their paths printed, and the build skill teaches the agent to
  search them (`file:line:` errors via `-file-line-error`, unwrapped lines).
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

### `olf list [--prune]`

List registered workspaces (ID, path, status of the link).

### `olf path`

Print the checkout path of the current (or `-p`) project.

### `olf edit`

Open the checkout in `$VISUAL` (falling back to `$EDITOR`). Works from
anywhere with `-p`.

### `olf open [--print]`

Open `https://www.overleaf.com/project/<id>` in the browser.

### `olf grant <agent> [--repo <dir>]`

Persistently allow an agent running in `--repo` (default: cwd) to write the
paper checkout. v1 supports `claude`:

- Merge the checkout path into `permissions.additionalDirectories` in
  `<repo>/.claude/settings.local.json` (never clobber existing settings).
- If that file isn't already git-ignored (`git check-ignore`), add it to the
  repo's `.git/info/exclude` — it's a machine-local path and must not be
  committed.
- Other agents (Codex writable roots, Gemini include dirs, …) added later.

### `olf exec -- <agent> [args...]`

Launch an agent with access to the paper for one session:

- Known agents get their flag injected (`claude` → `--add-dir <checkout>`).
- Every agent gets `OLF_WORKSPACE` and `OLF_PROJECT_DIR` in its environment;
  unknown agents get only these, with a warning.

### `olf skill install [--project]`

Install the bundled skills. Default: user-level (`~/.claude/skills/`) — skills
are read-only, so living outside the workspace is fine. `--project`: install
into the workspace's `.claude/skills/` (added to `info/exclude` when the
workspace is an adopted Overleaf clone). `olf skill list` shows what is
bundled.

## API (Rust + clap)

Implemented in Rust with clap's derive API. Agent-oriented conventions:

- Global `-C <dir>` (like `git -C`) so agents never need to `cd`.
- Global `--json` for machine-readable output, initially only for `build` and
  `list` (each JSON schema is a compatibility promise).
- Distinct exit codes so agents can branch on outcomes without parsing text.
- Global `-p/--project` to act on any registered project from anywhere.
- No git wrappers: agents pull, commit and push with plain `git`;
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

    /// Act on a registered project (ID, ID prefix, or workspace name)
    #[arg(short, long, global = true)]
    pub project: Option<String>,

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
    /// Compile locally with latexmk or tectonic
    Build(BuildArgs),
    /// Format .tex files with tex-fmt (requires fmt.enabled)
    Fmt(FmtArgs),
    /// List registered workspaces
    List(ListArgs),
    /// Print the checkout path
    Path,
    /// Open the checkout in $VISUAL / $EDITOR
    Edit,
    /// Open the project on overleaf.com
    Open(OpenArgs),
    /// Persistently allow an agent in a repo to write the checkout
    Grant(GrantArgs),
    /// Launch an agent with access to the checkout for one session
    Exec(ExecArgs),
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
    /// Also grant these agents access from the cwd's repo
    #[arg(long, value_enum)]
    pub grant: Vec<Agent>,
    /// Relink the index if this project is registered elsewhere
    #[arg(long)]
    pub force: bool,
    /// Main .tex file (skips auto-detection)
    #[arg(long)]
    pub main: Option<PathBuf>,
    /// Workspace directory (default: cwd)
    pub path: Option<PathBuf>,
}

#[derive(Args)]
pub struct BuildArgs {
    /// Override engine from .olf/config.toml
    #[arg(long, value_enum)]
    pub engine: Option<Engine>,
    /// Override main file from .olf/config.toml
    #[arg(long)]
    pub main: Option<PathBuf>,
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
pub struct ListArgs {
    /// Remove links whose workspace no longer exists
    #[arg(long)]
    pub prune: bool,
}

#[derive(Args)]
pub struct OpenArgs {
    /// Print the URL instead of opening a browser
    #[arg(long)]
    pub print: bool,
}

#[derive(Args)]
pub struct GrantArgs {
    #[arg(value_enum)]
    pub agent: Agent,
    /// Repo whose agent settings to update (default: cwd)
    #[arg(long)]
    pub repo: Option<PathBuf>,
}

#[derive(Args)]
pub struct ExecArgs {
    /// Agent command and its arguments, after `--`
    #[arg(last = true, required = true)]
    pub command: Vec<String>,
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

#[derive(Clone, Copy, ValueEnum)]
pub enum Agent {
    Claude,
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
    Error = 1,          // generic / usage
    NotAProject = 2,    // no .olf/config.toml found
    AuthFailed = 3,     // Overleaf rejected the git token
    BuildFailed = 4,
    Unformatted = 5,    // fmt --check found diffs
    MissingTool = 6,    // latexmk / tectonic / tex-fmt not on PATH
    InsideGitRepo = 7,  // init target is inside a non-Overleaf repo
    UnknownProject = 8, // -p didn't match (or matched several) projects
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
  index.rs       // ~/.olf/projects symlink index, -p resolution
  overleaf.rs    // URL/ID parsing, project URL
  agents/        // per-agent grant/exec adapters (claude.rs first)
  latex/
    engine.rs    // detect + run latexmk / tectonic
  commands/      // one module per subcommand
skills/          // SKILL.md files, embedded via include_str!
```

Dependencies: `clap` (derive, env), `serde`, `toml`, `serde_json`, `anyhow`
or `thiserror`, `which`, `open`.

## Skills (the differentiator)

1. **Sync discipline** (core skill, plain git) — co-authors may be editing in
   the browser right now, so:
   - `git pull` before starting and before each push;
   - small, descriptive commits; push soon after committing;
   - if a push is rejected, `git pull` and push again;
   - on a rebase conflict, stop and report the files — don't resolve
     co-authors' text unasked;
   - never force push, create branches, or rewrite pushed history;
   - on auth failure, tell the user to refresh the Overleaf git token.
2. **Collaborative editing manners** — don't reformat or re-wrap text you did
   not change; preserve `\label`s, macros, and existing style; only run
   `olf fmt` if the project enabled it.
3. **Build-fix loop** — edit → `olf build` → search the saved log → fix; give
   up and report after N failed attempts instead of thrashing.
4. **Access etiquette** — if the paper is outside the workspace and writes are
   blocked, ask the user to run `olf grant` (or grant that one directory)
   rather than retrying or working around the sandbox.
5. *(Later)* **Writing tasks** — prose tightening, reference/citation checks,
   venue-specific formatting.

## Releases

| Release | Scope | Why |
|---|---|---|
| **v0.1** | `init`, `build`, `open`, `skill install` | The core loop: clone safely, edit with git, compile. Enough to dogfood on real papers. |
| **v0.2** | `fmt`, project index, `list`, `path`, `edit`, `-p` | Conveniences that matter once there are several projects. |
| **v0.3** | `grant`, `exec` | Depend on unverified agent-specific settings. |

## Validation spike (before writing code)

- Clone and push with a token on a free account; check how the credential
  helper stores the token for `git.overleaf.com`.
- Edit in the browser, then push locally: capture the exact rejection and
  conflict output and save it as fixtures for the skills and tests.
- Revoke the token and capture the auth-failure output (for `AuthFailed`).
- Run latexmk with a separate output directory on real projects; check
  packages known to struggle with it (`minted`, biber setups).

## Non-goals (v1)

- Wrapping day-to-day git (`status`, `sync`, `pull`, `push`): agents use git
  directly, guided by the sync-discipline skill.
- Listing projects from the Overleaf account, triggering Overleaf compiles,
  reading Overleaf comments, or anything else requiring cookie-based scraping.
- MCP server — the CLI is already agent-usable via the shell; revisit if a
  need appears.
- Shipping a TeX distribution.

## Rejected alternatives

- **Workspace inside the user's code repo** (outer `.git/info/exclude` plus a
  `.ignore` with `!/paper/` to keep it searchable): works, but needs tricks in
  a repo olf doesn't own and unverified search-tool behaviour. Replaced by
  standalone workspaces + `grant`/`exec`.
- **Git submodule / subtree in the code repo**: submodules add pointer-bump
  noise and auth on recursive clone; subtree splitting fights the bridge's
  linear history. Possible later as an opt-in for pinning paper versions.
- **Central checkout store (`~/.olf/<id>`)**: sandboxed agents can't write
  there without flags, cloud sessions don't persist `~`, search is rooted at
  the cwd, and concurrent sessions would share one checkout.
- **Outward symlink (`workspace/paper -> ~/.olf/<id>`)**: sandboxes resolve
  real paths (by design, to prevent escapes), ripgrep doesn't follow symlinks
  while walking, and the link itself still needs ignoring.

## Open questions

- Distribution of skills: `olf skill install` only, or also a Claude Code
  plugin?
- Confirm git integration availability on free Overleaf plans.
- Verify whether Claude Code's `additionalDirectories` / `--add-dir` also
  covers sandboxed Bash writes (needed for `git commit` in the checkout), or
  whether sandbox write paths must be granted separately.
- Exact settings keys for other agents (Codex, Gemini CLI) before adding them
  to `grant`/`exec`.
- Multiple workspaces for the same project (e.g. parallel agent sessions).
