---
name: olf-editing
description: Etiquette for editing a shared LaTeX paper on Overleaf. Use whenever you edit .tex or .bib files in an olf workspace (a directory with .olf/config.toml) or in an Overleaf git checkout, so your diffs stay small and co-authors' work and style stay intact.
license: MIT
---

# Editing a shared Overleaf paper

The paper is shared: co-authors read every diff in Overleaf's history, and
edit the same files in the browser. Large or noisy diffs cause conflicts and
make their review painful.

If the paper is outside your working directory and writes are blocked, see
the olf-access skill.

## Change only what you were asked to change

- **Don't reformat, re-wrap or re-indent** text you didn't otherwise touch.
  Keep the existing line breaks; if a paragraph uses one sentence per line,
  continue that, and if it uses long lines, keep them long.
- **Don't run formatters** over files unless the user asks; whole-file
  reformatting is exactly the noisy diff co-authors dislike.
- Don't "tidy" unrelated things: whitespace, comment blocks, commented-out
  text, package order, or spelling variants (British vs American) elsewhere.
- Keep each edit easy to review: a reader of the diff should see exactly what
  changed and why.

## Preserve the project's structure and conventions

- **Never rename or remove `\label`s** that may be referenced (search the
  project for `\ref`, `\cref`, `\eqref`, `\autoref` first), and keep citation
  keys stable.
- Use the project's own macros (e.g. `\vect{x}`, `\etal`, custom
  environments) instead of re-spelling them, and define new macros next to
  the existing ones in the preamble or macro file.
- Match the existing style: citation commands (`\citep` vs `\cite`),
  reference style, figure/table placement, capitalization of headings,
  notation.
- Add new sections or figures as files only when the project already splits
  content that way; follow its directory layout (`sections/`, `figures/`, …).
- Keep the main file and Overleaf's project settings (compiler, main
  document) as they are; olf mirrors them in `.olf/config.toml`.

## Before committing

- Run `olf build` and make sure you didn't introduce errors or new warnings
  (see the olf-build skill).
- Review `git diff`: if it shows changes you didn't intend (whitespace,
  re-wrapped lines), revert those hunks.
- Then sync following the olf-sync skill.
