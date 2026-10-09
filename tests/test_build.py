import json
import shutil
from pathlib import Path

import pytest
from conftest import git

# Writes `$out/main.log` (from $LOG) and `$out/main.pdf`, then exits $CODE.
FAKE_LATEXMK = r"""
for a; do case "$a" in -outdir=*) out="${a#-outdir=}";; esac; done
printf '%s\n' "$LOG" > "$out/main.log"
[ "$CODE" = 0 ] && printf 'pdf' > "$out/main.pdf"
exit "$CODE"
"""

FAKE_TECTONIC = r"""
prev=; for a; do [ "$prev" = --outdir ] && out="$a"; prev="$a"; done
printf '(./main.tex)\n' > "$out/main.log"
printf 'pdf' > "$out/main.pdf"
"""


def saved_logs(ws):
    return sorted(p.name for p in (ws / ".olf/build/logs").iterdir())


def has_tex():
    return all(shutil.which(t) for t in ("latexmk", "pdflatex"))


def test_outside_workspace_is_not_a_project(env):
    result = env.build()
    assert result.returncode == 2
    assert "init.py" in result.stderr


def test_missing_engines_exit_with_install_hints(env):
    ws = env.workspace()
    result = env.build(cwd=ws, PATH=env.stub_path())
    assert result.returncode == 6
    assert "latexmk or tectonic" in result.stderr
    assert "tectonic-typesetting" in result.stderr


def test_auto_falls_back_to_tectonic_with_warning(env):
    ws = env.workspace()
    path = env.stub_path(tectonic=FAKE_TECTONIC)
    result = env.build(cwd=ws, PATH=path)
    assert result.returncode == 0, result.stderr
    assert "tectonic (XeTeX-based)" in result.stderr
    assert "with tectonic" in result.stdout
    assert (ws / ".olf/build/main.pdf").is_file()

    result = env.build("--engine", "latexmk", cwd=ws, PATH=path)
    assert result.returncode == 6


def test_auto_prefers_latexmk_and_keeps_timestamped_logs(env):
    ws = env.workspace()
    path = env.stub_path(latexmk=FAKE_LATEXMK, tectonic="exit 99")
    log = "./main.tex:3: Undefined control sequence."
    result = env.build(cwd=ws / "paper", PATH=path, LOG=log, CODE="12")
    assert result.returncode == 4
    assert result.stdout == ""
    assert "build failed (latexmk, exit 12)" in result.stderr
    assert ".olf/build/logs/" in result.stderr and "-main.log" in result.stderr

    names = saved_logs(ws)
    assert len(names) == 2, names
    base = names[0].removesuffix(".log")
    assert names[1] == f"{base}.out"
    assert base.endswith("Z-main")
    assert (ws / ".olf/build/logs" / names[0]).read_text().strip() == log

    result = env.build("--json", cwd=ws, PATH=path, LOG="fine", CODE="0")
    assert result.returncode == 0, result.stderr
    report = json.loads(result.stdout)
    assert set(report) == {"ok", "engine", "pdf", "log", "output"}
    assert report["ok"] is True and report["engine"] == "latexmk"
    assert report["pdf"].endswith(".olf/build/main.pdf")
    assert Path(report["log"]).read_text().strip() == "fine"
    assert report["output"].endswith("-main.out")
    assert len(saved_logs(ws)) == 4


def test_engine_output_is_kept_when_tex_never_ran(env):
    ws = env.workspace()
    path = env.stub_path(latexmk="echo 'biber: command not found' >&2; exit 12")
    result = env.build(cwd=ws, PATH=path)
    assert result.returncode == 4
    assert "engine output:" in result.stderr
    assert "log:" not in result.stderr.replace("engine output:", "")
    names = saved_logs(ws)
    assert len(names) == 1, names
    assert "biber: command not found" in (ws / ".olf/build/logs" / names[0]).read_text()


def test_keeps_only_the_newest_20_builds(env):
    ws = env.workspace()
    path = env.stub_path(latexmk=FAKE_LATEXMK)
    for _ in range(23):
        assert env.build(cwd=ws, PATH=path, LOG="x", CODE="0").returncode == 0
    names = saved_logs(ws)
    assert len(names) == 40
    # Same-second builds get a `.n` suffix, which still sorts chronologically.
    assert len([n for n in names if n.endswith(".out")]) == 20


def test_workspace_flag_and_env_discovery(env):
    ws = env.workspace()
    path = env.stub_path(latexmk=FAKE_LATEXMK)
    elsewhere = env.path / "elsewhere"
    elsewhere.mkdir()
    flag = env.build("--workspace", ws, cwd=elsewhere, PATH=path, LOG="x", CODE="0")
    assert flag.returncode == 0, flag.stderr
    via_env = env.build(
        cwd=elsewhere, PATH=path, LOG="x", CODE="0", OLF_PROJECT_DIR=str(ws / "paper")
    )
    assert via_env.returncode == 0, via_env.stderr


def test_compiler_selects_latexmk_mode_and_runs_in_main_dir(env):
    ws = env.workspace()
    config = ws / ".olf/config.toml"
    config.write_text(config.read_text().replace('"pdflatex"', '"xelatex"'))
    record = env.path / "argv"
    path = env.stub_path(
        latexmk=(
            f'pwd > "{record}"; echo "$@" >> "{record}"; '
            f'echo "$max_print_line" >> "{record}"; exit 1'
        )
    )
    env.build(cwd=ws, PATH=path)
    cwd, args, mpl = record.read_text().splitlines()
    assert cwd.endswith("ws/paper")
    assert args.startswith("-xelatex -interaction=nonstopmode -file-line-error -halt-on-error")
    assert args.endswith("main.tex")
    assert mpl == "10000"


@pytest.mark.skipif(not has_tex(), reason="latexmk/pdflatex not on PATH")
def test_builds_real_document_with_latexmk(env):
    ws = env.workspace()
    paper = ws / "paper"
    result = env.build(cwd=ws)
    assert result.returncode == 0, result.stderr
    assert "built" in result.stdout
    assert (ws / ".olf/build/main.pdf").is_file()
    assert git(paper, "status", "--porcelain") == ""

    (paper / "main.tex").write_text(
        "\\documentclass{article}\n\\begin{document}\nHello \\undefinedmacro.\n\\end{document}\n"
    )
    assert env.build(cwd=ws).returncode == 4
    newest = [n for n in saved_logs(ws) if n.endswith(".log")][-1]
    assert (
        "./main.tex:3: Undefined control sequence." in (ws / ".olf/build/logs" / newest).read_text()
    )
    assert git(paper, "status", "--porcelain") == " M main.tex"
