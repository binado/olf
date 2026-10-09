#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Create or repair an olf workspace: clone an Overleaf project and make it safe to work in.

Exit codes: 0 ok, 1 error, 2 bad arguments, 3 Overleaf rejected the git token,
6 git missing, 7 the target is inside a non-Overleaf git repo.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path

DEFAULT_GIT_BASE = "https://git.overleaf.com"
BLOCK_START = "# >>> olf (managed by olf; edits inside are overwritten)"
BLOCK_END = "# <<< olf"
HOOK_MARKER = "# olf-managed"
HOOK_SOURCE = Path(__file__).resolve().parent.parent / "assets" / "pre-push"

# Credential helper answering with $OVERLEAF_GIT_TOKEN. The token itself never appears
# in argv or on disk.
CREDENTIAL_HELPER = (
    '!f() { test "$1" = get && test -n "$OVERLEAF_GIT_TOKEN" && echo username=git'
    ' && echo "password=$OVERLEAF_GIT_TOKEN"; }; f'
)

# LaTeX build artifacts kept out of the checkout's `git status`.
LATEX_ARTIFACTS = [
    "*.aux", "*.log", "*.out", "*.toc", "*.lof", "*.lot", "*.fls", "*.fdb_latexmk",
    "*.synctex.gz", "*.blg", "*.bcf", "*.run.xml", "*.xdv", "*.nav", "*.snm", "*.vrb",
]  # fmt: skip
# Paths olf writes inside an adopted clone, which must never reach Overleaf.
ADOPTED_EXTRAS = ["/.olf/", "/.claude/"]

DOCUMENTCLASS = re.compile(r"^[ \t]*\\documentclass\s*(?:\[[^\]]*\])?\s*\{([^}]*)\}", re.MULTILINE)
AUTH_FAILURES = (
    "authentication failed",
    "could not read username",
    "could not read password",
    "invalid username or password",
    "the requested url returned error: 401",
    "the requested url returned error: 403",
)

CONFIG_TEMPLATE = """\
project_id = "{project_id}"
project_dir = "{project_dir}"

[build]
main = {main}
compiler = "pdflatex"  # as in the Overleaf project settings: pdflatex, xelatex or lualatex
engine = "auto"        # auto, latexmk or tectonic
"""


class OlfError(Exception):
    def __init__(self, message: str, code: int = 1):
        super().__init__(message)
        self.code = code


# --- git --------------------------------------------------------------------------------


def git(cwd: Path, *args: str, check: bool = True, env: dict | None = None):
    full_env = {**os.environ, **(env or {})}
    if not sys.stdin.isatty():
        # Agents can't answer prompts; humans at a terminal still can.
        full_env["GIT_TERMINAL_PROMPT"] = "0"
    try:
        result = subprocess.run(
            ["git", *args], cwd=cwd, env=full_env, capture_output=True, text=True
        )
    except FileNotFoundError:
        raise OlfError(
            "git not found on PATH\nhint: install git from https://git-scm.com", 6
        ) from None
    if check and result.returncode != 0:
        stderr = result.stderr.strip()
        if is_auth_failure(stderr):
            raise auth_error(stderr)
        raise OlfError(f"`git {' '.join(args)}` failed:\n{stderr}")
    return result


def is_auth_failure(stderr: str) -> bool:
    lowered = stderr.lower()
    return any(needle in lowered for needle in AUTH_FAILURES)


def auth_error(stderr: str) -> OlfError:
    return OlfError(
        f"Overleaf rejected the git credentials\n{stderr}\n"
        "hint: create a git token under Account Settings → Git Integration on overleaf.com, "
        "then set OVERLEAF_GIT_TOKEN (for example in Claude's `.claude/settings.local.json` "
        "`env`) and retry, or pass `--token <token>` to init.py",
        3,
    )


def inside_work_tree(directory: Path) -> bool:
    result = git(directory, "rev-parse", "--is-inside-work-tree", check=False)
    return result.returncode == 0 and result.stdout.startswith("true")


def toplevel(directory: Path) -> Path:
    return Path(git(directory, "rev-parse", "--show-toplevel").stdout.strip())


def origin_url(directory: Path) -> str | None:
    result = git(directory, "remote", "get-url", "origin", check=False)
    return result.stdout.strip() or None if result.returncode == 0 else None


def git_path(directory: Path, name: str) -> Path:
    path = Path(git(directory, "rev-parse", "--git-path", name).stdout.strip())
    return path if path.is_absolute() else directory / path


def replace_block(existing: str, lines: list[str]) -> str:
    block = "".join(f"{line}\n" for line in [BLOCK_START, *lines, BLOCK_END])
    start = existing.find(BLOCK_START)
    end = existing.find(BLOCK_END)
    if start != -1 and end != -1:
        end += len(BLOCK_END)
        if start < end:
            rest = existing[end:].removeprefix("\n")
            return existing[:start] + block + rest
    if not existing or existing.endswith("\n"):
        return existing + block
    return f"{existing}\n{block}"


def ensure_exclude_block(directory: Path, lines: list[str]) -> bool:
    """Make `lines` the content of olf's block in `info/exclude`. True if the file changed."""
    path = git_path(directory, "info/exclude")
    existing = path.read_text() if path.exists() else ""
    updated = replace_block(existing, lines)
    if updated == existing:
        return False
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(updated)
    return True


def exclude_lines(adopted: bool) -> list[str]:
    return [*(ADOPTED_EXTRAS if adopted else []), *LATEX_ARTIFACTS]


def install_pre_push_hook(directory: Path) -> str:
    """Install olf's hook: 'installed', 'unchanged', or 'foreign' (left alone)."""
    path = git_path(directory, "hooks/pre-push")
    wanted = HOOK_SOURCE.read_text()
    if path.exists():
        current = path.read_text()
        if current == wanted:
            return "unchanged"
        if HOOK_MARKER not in current:
            return "foreign"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(wanted)
    path.chmod(0o755)
    return "installed"


def clone(cwd: Path, url: str, dest: Path, token: str | None) -> None:
    args: list[str] = []
    env = None
    if token:
        # Use only the environment-backed helper for this clone. The token goes through the
        # child environment, never argv or persistent config.
        args += ["-c", "credential.helper=", "-c", f"credential.helper={CREDENTIAL_HELPER}"]
        env = {"OVERLEAF_GIT_TOKEN": token}
    git(cwd, *args, "clone", "--quiet", "--origin", "origin", url, str(dest), env=env)


def use_env_credential(checkout: Path) -> None:
    """Make future git operations read the token from the environment; nothing is persisted."""
    # An empty helper resets any inherited global helpers.
    git(checkout, "config", "--local", "--replace-all", "credential.helper", "")
    git(checkout, "config", "--add", "credential.helper", CREDENTIAL_HELPER)


# --- Overleaf ---------------------------------------------------------------------------


def parse_id(text: str) -> str:
    text = text.strip()
    if re.fullmatch(r"[0-9a-f]{24}", text):
        return text
    raise OlfError(f"invalid Overleaf project ID `{text}` (expected 24 lowercase hex characters)")


def id_from_url(url: str) -> str:
    def invalid(why: str) -> OlfError:
        return OlfError(
            f"cannot get a project ID from `{url}`: {why}\n"
            "hint: use the URL from your browser's address bar "
            "(https://www.overleaf.com/project/<id>) or the git clone URL"
        )

    stripped = url.strip()
    for scheme in ("https://", "http://"):
        if stripped.startswith(scheme):
            rest = stripped[len(scheme) :]
            break
    else:
        raise invalid("not an http(s) URL")
    rest = re.split(r"[?#]", rest, maxsplit=1)[0]
    authority, _, path = rest.partition("/")
    host = authority.rsplit("@", 1)[-1]
    segments = [s for s in path.split("/") if s]

    web = host in ("www.overleaf.com", "overleaf.com")
    if host == "git.overleaf.com" and segments:
        return parse_id(segments[0].removesuffix(".git"))
    if web and len(segments) >= 2 and segments[0] == "project":
        return parse_id(segments[1])
    if web and segments and segments[0] in ("read", "edit"):
        raise invalid(
            "this is a share link, which doesn't contain the project ID; "
            "open it, join the project, and copy the URL from the address bar"
        )
    if web or host == "git.overleaf.com":
        raise invalid("no project ID in the path")
    raise invalid("not an overleaf.com URL")


def git_base() -> str:
    """Base URL of the git bridge. `OLF_GIT_BASE` (undocumented) lets tests point it at a
    directory of local bare repositories named by project ID."""
    return os.environ.get("OLF_GIT_BASE") or DEFAULT_GIT_BASE


def git_url(project_id: str) -> str:
    return f"{git_base().rstrip('/')}/{project_id}"


def id_from_remote(url: str) -> str | None:
    """The project ID behind a git remote URL, if it points at the bridge."""
    base = git_base().rstrip("/") + "/"
    try:
        if url.startswith(base):
            return parse_id(url[len(base) :].rstrip("/").removesuffix(".git"))
        if "git.overleaf.com" in url:
            return id_from_url(url)
    except OlfError:
        pass
    return None


# --- workspace --------------------------------------------------------------------------


def find_root(start: Path) -> Path | None:
    for directory in (start, *start.parents):
        if (directory / ".olf" / "config.toml").is_file():
            return directory
    return None


def load_config(path: Path) -> dict:
    try:
        with path.open("rb") as f:
            return tomllib.load(f)
    except tomllib.TOMLDecodeError as e:
        raise OlfError(f"invalid {path}: {e}") from None


def set_main(path: Path, main: str) -> None:
    """Set `[build] main`, touching nothing else in the file."""
    line = f"main = {json.dumps(main)}"
    lines = path.read_text().splitlines()
    section = None
    for i, current in enumerate(lines):
        header = re.match(r"\s*\[([^\]]*)\]", current)
        if header:
            section = header.group(1).strip()
        elif section == "build" and re.match(r"\s*main\s*=", current):
            lines[i] = line
            break
    else:
        for i, current in enumerate(lines):
            if re.match(r"\s*\[build\]", current):
                lines.insert(i + 1, line)
                break
        else:
            lines += ["", "[build]", line]
    path.write_text("\n".join(lines) + "\n")


def same_path(a: Path, b: Path) -> bool:
    return a.resolve() == b.resolve()


def check_enclosing_repo(target: Path) -> str | None:
    """Refuse targets inside non-Overleaf git repos. Returns the project ID when `target` is
    itself the root of an Overleaf clone (adopt mode)."""
    probe = next((p for p in (target, *target.parents) if p.is_dir()), Path("/"))
    if not inside_work_tree(probe):
        return None
    top = toplevel(probe)
    url = origin_url(top)
    project_id = id_from_remote(url) if url else None
    if project_id and same_path(top, target):
        return project_id
    if project_id:
        raise OlfError(
            f"{target} is inside the Overleaf clone at {top}\n"
            f"hint: to use that clone as the workspace, run init.py on {top}"
        )
    suggestion = top.parent / f"{top.name or 'paper'}-paper"
    raise OlfError(
        f"{target} is inside the git repository at {top}\n"
        f"hint: create the workspace outside it, e.g. `init.py {suggestion}`",
        7,
    )


def resolve_id(args, existing: dict | None, adopted: str | None, checkout: Path) -> str:
    """The project ID, checked for agreement across every place that names one."""
    sources: list[tuple[str, str]] = []
    if args.id:
        sources.append(("--id", parse_id(args.id)))
    if args.url:
        sources.append(("--url", id_from_url(args.url)))
    if existing:
        sources.append((".olf/config.toml", parse_id(str(existing.get("project_id", "")))))
    if adopted:
        sources.append(("the clone's origin", adopted))
    elif checkout.is_dir() and inside_work_tree(checkout):
        origin = origin_url(checkout) or ""
        project_id = id_from_remote(origin)
        if not project_id:
            raise OlfError(
                f"{checkout} is a git checkout whose origin ({origin}) isn't an Overleaf project"
            )
        sources.append((f"the origin of {checkout}", project_id))

    if not sources:
        raise OlfError(
            "no Overleaf project given\n"
            "hint: pass --url <overleaf-project-url> or --id <project-id>"
        )
    first_source, project_id = sources[0]
    for source, other in sources:
        if other != project_id:
            raise OlfError(
                f"project mismatch: {first_source} says {project_id} but {source} says {other}"
            )
    return project_id


def needs_clone(checkout: Path) -> bool:
    if not checkout.exists() or not any(checkout.iterdir()):
        return True
    if not (checkout / ".git").exists():
        raise OlfError(
            f"{checkout} exists but isn't a git checkout\nhint: move it away and re-run init.py"
        )
    return False


def detect_main(checkout: Path) -> str:
    """A root `main.tex`, else the single `.tex` file (in any folder, as Overleaf allows) with
    a `\\documentclass` that isn't `standalone`/`subfiles`, else the single one named main.tex."""
    if (checkout / "main.tex").is_file():
        return "main.tex"
    candidates: list[Path] = []
    for dirpath, dirnames, filenames in os.walk(checkout):
        dirnames[:] = [d for d in dirnames if not d.startswith(".")]  # .git, .olf, .claude, ...
        for name in filenames:
            if name.startswith(".") or not name.endswith(".tex"):
                continue
            path = Path(dirpath, name)
            text = path.read_text(errors="replace")
            if any(m.group(1).strip() not in ("standalone", "subfiles")
                   for m in DOCUMENTCLASS.finditer(text)):  # fmt: skip
                candidates.append(path.relative_to(checkout))
    candidates.sort(key=lambda p: (len(p.parts), p.as_posix()))
    named_main = [p for p in candidates if p.name == "main.tex"]
    if len(candidates) == 1:
        return candidates[0].as_posix()
    if len(named_main) == 1:
        return named_main[0].as_posix()
    if not candidates:
        raise OlfError(
            f"no main .tex file found in {checkout} (no file with \\documentclass)\n"
            "hint: pass --main <file>"
        )
    names = ", ".join(p.as_posix() for p in candidates)
    raise OlfError(
        f"several possible main files: {names}\n"
        "hint: pass --main <file> (Overleaf: Menu → Main document)"
    )


# --- command ----------------------------------------------------------------------------


def run(args) -> None:
    target = Path(os.path.abspath(Path.cwd() / (args.path or ".")))
    # Re-running from inside a workspace (e.g. its checkout) repairs that workspace.
    target = find_root(target) or target

    config_path = target / ".olf" / "config.toml"
    existing = load_config(config_path) if config_path.is_file() else None

    adopted_id = check_enclosing_repo(target)
    adopted = adopted_id is not None
    project_dir = (existing or {}).get("project_dir") or ("." if adopted else "paper")
    checkout = Path(os.path.normpath(target / project_dir))

    project_id = resolve_id(args, existing, adopted_id, checkout)
    url = git_url(project_id)
    token = args.token or os.environ.get("OVERLEAF_GIT_TOKEN") or None

    if needs_clone(checkout):
        target.mkdir(parents=True, exist_ok=True)
        clone(target, url, checkout, token)
        print(f"cloned {url} into {checkout}")
    elif adopted:
        print(f"adopting existing Overleaf clone at {checkout}")

    use_env_credential(checkout)

    configured_main = ((existing or {}).get("build") or {}).get("main")
    if args.main:
        if not (checkout / args.main).is_file():
            raise OlfError(f"--main {args.main} not found in {checkout}")
        main = args.main
    else:
        main = configured_main or detect_main(checkout)

    if existing is None:
        config_path.parent.mkdir(parents=True, exist_ok=True)
        config_path.write_text(
            CONFIG_TEMPLATE.format(
                project_id=project_id, project_dir=project_dir, main=json.dumps(main)
            )
        )
        print(f"wrote {config_path}")
    elif main != configured_main:
        # An explicit flag is a request to change the value; a missing key gets the detected one.
        set_main(config_path, main)

    git(checkout, "config", "--local", "pull.rebase", "true")
    # A global `pull.ff=only` would otherwise make `git pull` refuse to rebase as soon as a
    # co-author edits in the browser.
    git(checkout, "config", "--local", "pull.ff", "true")
    status = install_pre_push_hook(checkout)
    if status == "installed":
        print("installed pre-push hook")
    elif status == "foreign":
        hook = git_path(checkout, "hooks/pre-push")
        print(
            f"warning: {hook} exists and wasn't written by olf; left it alone, "
            "so force pushes and pushes to other branches aren't guarded",
            file=sys.stderr,
        )
    ensure_exclude_block(checkout, exclude_lines(adopted))

    print(f"workspace ready: {target} (project {project_id}, main {main})")


def main() -> None:
    parser = argparse.ArgumentParser(description="Clone and/or set up an Overleaf project.")
    source = parser.add_mutually_exclusive_group()
    source.add_argument("--id", help="Overleaf project ID")
    source.add_argument("--url", help="any overleaf.com project URL")
    parser.add_argument(
        "--token",
        help="git token for this setup; later git operations read $OVERLEAF_GIT_TOKEN "
        "(also the default for this flag)",
    )
    parser.add_argument("--main", help="main .tex file, relative to the checkout")
    parser.add_argument("path", nargs="?", help="workspace directory (default: cwd)")
    try:
        run(parser.parse_args())
    except OlfError as e:
        print(f"error: {e}", file=sys.stderr)
        sys.exit(e.code)


if __name__ == "__main__":
    main()
