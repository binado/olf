#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Compile an olf workspace's paper locally with latexmk or tectonic.

Output goes to ``.olf/build/`` (never the checkout). The TeX log is not interpreted: success is
the engine's exit status, and each run's logs are kept under ``.olf/build/logs/`` to be read.

Exit codes: 0 built, 1 error, 2 not inside an olf workspace, 4 build failed, 6 engine missing.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tomllib
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path

KEEP_BUILDS = 20

COMPILER_MODES = {"pdflatex": "-pdf", "xelatex": "-xelatex", "lualatex": "-lualatex"}

INSTALL_HINTS = {
    "latexmk": (
        "install a TeX distribution with latexmk: TeX Live (https://tug.org/texlive/), "
        "MacTeX on macOS (`brew install --cask mactex-no-gui`), or `apt install latexmk`"
    ),
    "tectonic": "install tectonic: https://tectonic-typesetting.github.io",
}


class OlfError(Exception):
    """A failure with the process exit code to report it with."""

    def __init__(self, message: str, code: int = 1) -> None:
        super().__init__(message)
        self.code = code


@dataclass
class Workspace:
    """A directory holding ``.olf/config.toml`` plus the Overleaf checkout."""

    root: Path
    config: dict

    @property
    def checkout(self) -> Path:
        return Path(os.path.normpath(self.root / self.config.get("project_dir", "paper")))

    @property
    def build_dir(self) -> Path:
        return self.root / ".olf" / "build"


def discover(workspace: str | None = None) -> Workspace:
    """Find the workspace: ``--workspace``, else the cwd, else ``$OLF_PROJECT_DIR``.

    Parameters
    ----------
    workspace
        Directory to start from; when given, no other start is tried.

    Returns
    -------
    Workspace
        The nearest ancestor (inclusive) holding ``.olf/config.toml``.

    Raises
    ------
    OlfError
        With code 2 if no start directory is inside a workspace.
    """
    starts = [Path(workspace)] if workspace else [Path.cwd()]
    if not workspace and os.environ.get("OLF_PROJECT_DIR"):
        starts.append(Path(os.environ["OLF_PROJECT_DIR"]))
    for start in starts:
        start = Path(os.path.abspath(start))
        for directory in (start, *start.parents):
            config = directory / ".olf" / "config.toml"
            if config.is_file():
                try:
                    return Workspace(directory, tomllib.loads(config.read_text()))
                except tomllib.TOMLDecodeError as e:
                    raise OlfError(f"invalid {config}: {e}") from None
    raise OlfError(
        f"not inside an olf workspace (no .olf/config.toml in {starts[0]} or its parents)\n"
        "hint: run olf-setup's init.py --url <overleaf-url> to create one, "
        "or pass --workspace <dir>",
        2,
    )


def select_engine(choice: str) -> tuple[str, str]:
    """Pick an engine available on PATH; ``auto`` prefers latexmk, which Overleaf itself runs.

    Returns
    -------
    tuple[str, str]
        The engine name and the path of its executable.
    """
    candidates = ["latexmk", "tectonic"] if choice == "auto" else [choice]
    for name in candidates:
        if program := shutil.which(name):
            return name, program
    if choice == "auto":
        hints = f"{INSTALL_HINTS['latexmk']}\n      or {INSTALL_HINTS['tectonic']}"
        raise OlfError(f"latexmk or tectonic not found on PATH\nhint: {hints}", 6)
    raise OlfError(f"{choice} not found on PATH\nhint: {INSTALL_HINTS[choice]}", 6)


def engine_command(
    engine: str, program: str, main: Path, out_dir: Path, compiler: str
) -> list[str]:
    """The command line compiling ``main`` (run from the main file's directory)."""
    if engine == "latexmk":
        mode = COMPILER_MODES.get(compiler, "-pdf")
        return [
            program,
            mode,
            "-interaction=nonstopmode",
            "-file-line-error",
            "-halt-on-error",
            f"-outdir={out_dir}",
            main.name,
        ]
    return [program, "-X", "compile", main.name, "--outdir", str(out_dir), "--keep-logs"]


def save_logs(out_dir: Path, main: Path, output: bytes) -> tuple[Path | None, Path]:
    """Copy this run's TeX log and console output to ``logs/<timestamp>-<stem>.*``."""
    logs = out_dir / "logs"
    logs.mkdir(parents=True, exist_ok=True)
    stamp = datetime.now(UTC).strftime("%Y-%m-%dT%H-%M-%SZ")
    # Two builds within a second get distinct names; `.` sorts after `-`, so lexical order
    # stays chronological.
    base, n = f"{stamp}-{main.stem}", 1
    while (logs / f"{base}.out").exists():
        n += 1
        base = f"{stamp}.{n:03d}-{main.stem}"
    out_path = logs / f"{base}.out"
    out_path.write_bytes(output)
    tex_log = out_dir / f"{main.stem}.log"
    if not tex_log.is_file():
        return None, out_path
    saved = logs / f"{base}.log"
    shutil.copyfile(tex_log, saved)
    return saved, out_path


def prune_logs(logs: Path, keep: int) -> None:
    """Keep only the newest ``keep`` builds' logs (names sort chronologically)."""
    outs = sorted(logs.glob("*.out"))
    for out in outs[: max(len(outs) - keep, 0)]:
        out.unlink()
        out.with_suffix(".log").unlink(missing_ok=True)


def run(args: argparse.Namespace) -> None:
    ws = discover(args.workspace)
    checkout = ws.checkout
    build = ws.config.get("build", {})
    main_name = args.main or build.get("main")
    if not main_name:
        raise OlfError(
            "no main file configured\nhint: set build.main in .olf/config.toml or pass --main"
        )
    main = Path(main_name)
    if not (checkout / main).is_file():
        raise OlfError(f"main file {main} not found in {checkout}")

    engine, program = select_engine(args.engine or build.get("engine", "auto"))
    compiler = build.get("compiler", "pdflatex")
    if engine == "tectonic" and compiler != "xelatex":
        print(
            f"warning: building with tectonic (XeTeX-based) but the project uses {compiler}; "
            "results may differ from Overleaf",
            file=sys.stderr,
        )

    out_dir = ws.build_dir
    out_dir.mkdir(parents=True, exist_ok=True)
    env = {
        **os.environ,
        # Unwrapped log lines keep messages and file paths parseable.
        "max_print_line": "10000",
        "error_line": "254",
        "half_error_line": "238",
    }
    # Overleaf compiles from the main file's directory, so `\input` paths are relative to it.
    try:
        proc = subprocess.run(
            engine_command(engine, program, main, out_dir, compiler),
            cwd=checkout / main.parent,
            env=env,
            stdin=subprocess.DEVNULL,
            capture_output=True,
        )
    except OSError as e:
        raise OlfError(f"cannot run {program}: {e}") from None
    ok = proc.returncode == 0

    tex_log, out_log = save_logs(out_dir, main, proc.stdout + proc.stderr)
    prune_logs(out_dir / "logs", KEEP_BUILDS)
    pdf = out_dir / f"{main.stem}.pdf"
    # A failed run may leave the previous PDF behind; never report that one.
    pdf_path = pdf if ok and pdf.is_file() else None

    if args.json:
        print(
            json.dumps(
                {
                    "ok": ok,
                    "engine": engine,
                    "pdf": str(pdf_path) if pdf_path else None,
                    "log": str(tex_log) if tex_log else None,
                    "output": str(out_log),
                },
                indent=2,
            )
        )

    paths = (f"\nlog: {tex_log}" if tex_log else "") + f"\nengine output: {out_log}"
    if not ok:
        status = f"exit {proc.returncode}" if proc.returncode >= 0 else "killed"
        raise OlfError(f"build failed ({engine}, {status}){paths}", 4)
    if not args.json:
        print(f"built {pdf_path or 'no PDF produced'} with {engine}{paths}")


def main() -> None:
    parser = argparse.ArgumentParser(description="Compile the paper locally into .olf/build/.")
    parser.add_argument("--json", action="store_true", help="machine-readable output")
    parser.add_argument("--engine", choices=["auto", "latexmk", "tectonic"])
    parser.add_argument("--main", help="main .tex file relative to the checkout")
    parser.add_argument("--workspace", help="workspace directory (default: cwd, $OLF_PROJECT_DIR)")
    try:
        run(parser.parse_args())
    except OlfError as e:
        print(f"error: {e}", file=sys.stderr)
        sys.exit(e.code)


if __name__ == "__main__":
    main()
