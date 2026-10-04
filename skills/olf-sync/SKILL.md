---
name: olf-sync
description: Git sync discipline for Overleaf projects. Use whenever you pull, commit, or push in an olf workspace (a directory with .olf/config.toml) or any git checkout whose origin is git.overleaf.com — co-authors may be editing the same paper in the browser right now.
---

# Syncing an Overleaf project with git

An olf workspace holds `.olf/config.toml` and the Overleaf git checkout
(`project_dir` in the config, usually `paper/`; `.` when the clone itself is
the workspace). Run git **inside the checkout** (`git -C paper ...`).

Overleaf's git bridge is not GitHub:

- There is exactly one branch (whatever `origin/HEAD` points at, usually
  `main`; older projects may use `master`), with linear history.
- Co-authors edit the same files in the browser, often at the same moment.
- Every file you push appears in the Overleaf project tree for everyone.

`olf init` already set `pull.rebase=true` (plus `pull.ff=true`, so a global
`pull.ff=only` can't block the rebase) and a pre-push hook that rejects
force pushes, deletions and pushes to any other branch. The hook is a safety net,
not permission to try those things.

## Rules

1. **Pull before you start** and again right before each push:
   `git -C paper pull`. Commit (or `git stash`) your work first; a rebase
   needs a clean tree.
2. **Small, descriptive commits** (e.g. `Fix typo in related work`,
   `Add ablation table`), each limited to what the user asked for.
3. **Push soon after committing.** The longer local commits sit, the more
   likely they conflict with browser edits.
4. **Push rejected** (`rejected`, `fetch first`, `non-fast-forward`): someone
   pushed or edited in the browser. Run `git pull`, then push again. Never
   reach for `--force` or `--no-verify`. If `git pull` says `Not possible to
   fast-forward`, the checkout's config is incomplete: run `olf init` (it
   repairs the setup) and pull again; don't merge instead.
5. **Rebase conflict during pull: stop.** Don't resolve co-authors' text on
   your own. Do this:
   - list the files: `git -C paper diff --name-only --diff-filter=U`
   - restore your state: `git -C paper rebase --abort`
     (your commits stay local and unpushed)
   - tell the user which files conflict and what you were changing, then wait.
   Resolve only when the user explicitly asks, and then keep both sides'
   intent.
6. **Never** force push, create or push other branches or tags, amend or
   rebase commits that are already pushed, or rewrite history.
7. **Only commit paper files.** No build outputs (`*.aux`, `*.log`, PDFs from
   `olf build` live in `.olf/build/` anyway), scratch notes, or agent files.
   Check `git status` before committing and stage files by name.
8. **Authentication failure** (`Authentication failed`, HTTP 401/403, or
   `could not read Username`): the Overleaf git token is missing, expired or
   revoked. Don't retry in a loop, and never ask for the token in chat. Ask
   the user to create a token on overleaf.com (Account Settings → Git
   Integration) and run `olf init --token <token>` themselves.

## A typical round

```sh
git -C paper pull
# ... edit, then `olf build` to check it compiles ...
git -C paper status
git -C paper add sections/results.tex
git -C paper commit -m "Tighten results discussion"
git -C paper pull      # pick up browser edits made meanwhile
git -C paper push
```
