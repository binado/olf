---
name: olf-build
description: Compile an Overleaf paper locally and fix LaTeX errors with `build.py`. Use after editing .tex/.bib files in an olf workspace (a directory with .olf/config.toml), when asked whether the paper compiles, or when fixing LaTeX errors or warnings.
license: MIT
---

# Build-fix loop with `build.py`

`scripts/build.py` (next to this file) compiles the paper locally (latexmk, or
tectonic as a fallback) into `.olf/build/`, never into the checkout:

```sh
uv run <this-skill-dir>/scripts/build.py [--json] [--engine auto|latexmk|tectonic] [--main FILE]
```

It works from anywhere inside the workspace. From elsewhere, pass
`--workspace <dir>` or rely on `$OLF_PROJECT_DIR` (see the olf-access skill).
It is stdlib-only Python (>= 3.11): `python3 build.py` works without `uv`.

| Exit code | Meaning |
|---|---|
| 0 | built; PDF path printed |
| 2 | not inside an olf workspace (create one with olf-setup's `init.py`) |
| 4 | the build failed; read the logs (below) |
| 6 | no latexmk/tectonic installed: tell the user, don't try to install TeX yourself |

`build.py` doesn't summarize errors. It prints where this run's logs are:

```
log: <workspace>/.olf/build/logs/2026-10-04T15-30-12Z-main.log
engine output: <workspace>/.olf/build/logs/2026-10-04T15-30-12Z-main.out
```

- `*.log` is the full TeX log of the run.
- `*.out` is the engine's console output (latexmk, bibtex/biber). Read it when
  there is no `.log` or the `.log` shows no error.
- Each build gets its own timestamped pair (UTC, sortable), and the last 20
  builds are kept. Compare the newest log with an earlier one to see what
  your change introduced.
- `build.py --json` prints `{ ok, engine, pdf, log, output }` instead.

## Reading a TeX log

Logs are long; search them, don't read them whole. Lines are not wrapped, and
errors use file:line form:

```sh
grep -n -A2 -E '^[^ ]+:[0-9]+: |^! ' <log>   # errors with their l.<n> line
```

- An error looks like `./sections/intro.tex:12: Undefined control sequence.`,
  followed by `l.12 <the offending source line>`.
- Some errors start with `! ` (e.g. `! LaTeX Error: File 'foo.sty' not
  found.`); the `l.<n>` line after them gives the line, and the most recent
  `(./file.tex` opened before it is the file.
- `Emergency stop` and `==> Fatal error occurred` only repeat an earlier error.
- **File paths are relative to the main file's directory** (`build.main` in
  `.olf/config.toml`), not to the checkout: with `main = "notes/main.tex"`,
  `./chapters/a.tex` is `notes/chapters/a.tex` in the checkout.
- Warnings: `grep -n -E 'Warning|Overfull' LOG`. Undefined references and
  citations end with `on input line <n>`. Package warnings continue on lines
  starting with `(<package>)`.

## Loop

1. Run `build.py`.
2. On exit 4, find the **first** error in the log and fix it; later errors are
   often consequences of it.
3. Rebuild and repeat.

## Stop instead of thrashing

- Give up after **3 attempts at the same error**, or **5 builds in a row**
  without progress, and report to the user: the error, what you tried, and
  your best hypothesis.
- Missing packages, fonts or `biber`/`bibtex` failures usually mean the local
  TeX installation differs from Overleaf's. Report them; don't rewrite the
  paper to work around the local setup.
- A local pass doesn't guarantee an Overleaf pass (Overleaf pins its own TeX
  Live version), and a local failure in a project that compiles on Overleaf
  points at the local setup, not the paper.

## Warnings

Before committing, check the newest log for warnings your change introduced
(undefined `\ref`/`\cite`, new overfull boxes) and fix those. Leave
pre-existing warnings alone unless the user asked; fixing them adds unrelated
diffs.
