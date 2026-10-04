---
name: olf-build
description: Compile an Overleaf paper locally and fix LaTeX errors with `olf build`. Use after editing .tex/.bib files in an olf workspace (a directory with .olf/config.toml), when asked whether the paper compiles, or when fixing LaTeX errors or warnings.
---

# Build-fix loop with `olf build`

`olf build` compiles the paper locally (latexmk, or tectonic as a fallback)
into `.olf/build/`, never into the checkout, and condenses the TeX log into
`file:line: message` lines. It works from anywhere inside the workspace, or
from elsewhere with `olf -C <workspace> build`.

| Exit code | Meaning |
|---|---|
| 0 | built; PDF path printed |
| 2 | not inside an olf workspace |
| 4 | LaTeX errors (listed on stdout, full log path on stderr) |
| 6 | no latexmk/tectonic installed: tell the user, don't try to install TeX yourself |

Useful flags: `--warnings` (undefined references/citations, overfull boxes),
`--json` (machine-readable `{ ok, engine, pdf, log, errors[], warnings[] }`),
`--main <file>` to build another root file.

## Loop

1. Run `olf build`.
2. Read the condensed errors. Fix the **first** error first; later ones are
   often consequences of it. Paths are relative to the checkout, and the
   `l.<n>` line echoes the offending source.
3. If the condensed output isn't enough, read the relevant part of the full
   log printed after `full log:` (search for the message; don't dump it all).
4. Rebuild and repeat.

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

Use `olf build --warnings` before committing. Fix warnings your change
introduced (undefined `\ref`/`\cite`, new overfull boxes). Leave pre-existing
warnings alone unless the user asked; fixing them adds unrelated diffs.
