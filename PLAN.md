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

## Configuration: `.overleafrc`

Per-project TOML file at the checkout root, written by `olf checkout` /
`olf init`. `olf` finds it by walking up from the cwd (like git finds `.git`),
so no global registry is needed.

```toml
project_id = "64f0c0ffee..."

[build]
main = "main.tex"          # auto-detected at checkout (file with \documentclass)
compiler = "pdflatex"      # pdflatex | xelatex | lualatex — mirror Overleaf's setting
engine = "auto"            # auto | latexmk | tectonic
outdir = ".olf/build"

[fmt]
enabled = false            # opt-in per project, see `olf fmt`
wrap = false
```

The token is **never** stored in the rc file or the git remote URL.

## Commands

### `olf checkout (--id <id> | --url <overleaf-url>) [dir]`

Clone a project and set it up for safe use:

- Accept a project ID or any Overleaf project URL (extract the ID).
- Token from `--token` or `OVERLEAF_GIT_TOKEN`; stored via a git credential
  helper (Keychain on macOS), not embedded in `.git/config`.
- Write `.overleafrc` (auto-detect main file).
- Configure git for the bridge: `pull.rebase=true`; pre-push hook that rejects
  force pushes and non-`master` branches.
- Add `.olf/` and LaTeX build artifacts to `.git/info/exclude` (local only, so
  no `.gitignore` leaks into the Overleaf project).

### `olf init`

Same setup as `checkout` (rc file, git config, excludes) for an existing clone.
Project ID is read from `git remote get-url origin`.

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
- Output goes to `outdir` (`.olf/build`), never the working tree.
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
- Options come from `.overleafrc`; if the project has a `tex-fmt.toml`, defer
  to it instead.
- If `tex-fmt` is not on PATH, fail with install hints.

### `olf open`

Open `https://www.overleaf.com/project/<id>` in the browser.

### `olf skill install`

Install the bundled skills into the project (`.claude/skills/`).

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

- Implementation language (Python + uv vs. a single Go/Rust binary).
- Distribution of skills: `olf skill install` only, or also a Claude Code
  plugin?
- Confirm git integration availability on free Overleaf plans.
- Retry/backoff policy for `olf sync`.
