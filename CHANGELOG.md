# Changelog

## 0.4.0

- **Breaking:** the `olf` CLI is removed. The repo now ships only Agent Skills,
  each bundling the small stdlib-only Python scripts it needs (run with
  `uv run`, or `python3`): `olf-setup/scripts/init.py`,
  `olf-build/scripts/build.py`, `olf-access/scripts/grant.py`.
- Add the `olf-setup` skill (clone/adopt/repair a workspace).
- Remove the project registry (`olf list`, `-p`, `~/.olf/projects`),
  `olf path`, `olf edit`, `olf open` and `olf exec`. Use `--workspace <dir>`,
  `$OLF_PROJECT_DIR` (set by `grant.py`), `open https://www.overleaf.com/project/<id>`
  and `claude --add-dir "$OLF_PROJECT_DIR"` instead.
- Existing 0.3 workspaces keep working: same `.olf/config.toml` and pre-push hook.
- Drop the unused `[fmt]` config section.

## 0.3.0

- Add `olf grant claude` to grant persistent checkout access from a code repo.
- Add `olf exec` to launch an agent with checkout access for one session.
- Add `olf init --grant` to configure checkout access during setup.
- Read Overleaf Git tokens from `OVERLEAF_GIT_TOKEN` without saving credentials
  in the checkout.
- Prepare the crates.io package with license files, package metadata, and an
  explicit file allowlist.

## 0.2.0

- Register workspaces in the project index and select projects with `-p`.
- Add `olf list`, `olf path`, and `olf edit`.

## 0.1.0

- Clone or adopt Overleaf projects with `olf init`.
- Build locally with `latexmk` or `tectonic` using `olf build`.
- Open projects in the browser with `olf open`.
- Bundle and install agent skills for syncing, editing, and building papers.
