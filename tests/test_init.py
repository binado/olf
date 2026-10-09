import os
import shutil
import subprocess
from pathlib import Path

import pytest
from conftest import ID, INIT, MAIN_TEX, OTHER_ID, git, load_script

init = load_script(INIT)


# --- parsing tables ---------------------------------------------------------------------


@pytest.mark.parametrize(
    "url",
    [
        f"https://www.overleaf.com/project/{ID}",
        f"https://www.overleaf.com/project/{ID}/",
        f"https://www.overleaf.com/project/{ID}/detached?x=1#y",
        f"https://overleaf.com/project/{ID}?a=b",
        f"http://www.overleaf.com/project/{ID}",
        f"https://git.overleaf.com/{ID}",
        f"https://git.overleaf.com/{ID}.git",
        f"https://git@git.overleaf.com/{ID}",
    ],
)
def test_url_parses(url):
    assert init.id_from_url(url) == ID


@pytest.mark.parametrize(
    ("url", "message"),
    [
        ("https://www.overleaf.com/read/abcdefghijkl#1a2b3c", "share link"),
        ("https://www.overleaf.com/edit/abcdefghijkl", "share link"),
        ("https://www.overleaf.com/1234567890abcdefgh", "no project ID"),
        ("https://www.overleaf.com/project", "no project ID"),
        ("https://github.com/x/y", "not an overleaf.com URL"),
        (ID, r"not an http\(s\) URL"),
        ("https://www.overleaf.com/project/1234", "invalid Overleaf project ID"),
    ],
)
def test_url_rejects(url, message):
    with pytest.raises(init.OlfError, match=message):
        init.id_from_url(url)


def test_project_id_validation():
    assert init.parse_id(ID) == ID
    for bad in ["64F0C0FFEE0123456789ABCD", "64f0c0ffee", "z" * 24]:
        with pytest.raises(init.OlfError):
            init.parse_id(bad)


def test_id_from_remote():
    assert init.id_from_remote(f"https://git.overleaf.com/{ID}") == ID
    assert init.id_from_remote("git@github.com:x/y.git") is None
    assert init.id_from_remote(f"https://www.overleaf.com/project/{ID}") is None


def make_tree(root: Path, files: dict[str, str]) -> Path:
    for name, content in files.items():
        (root / name).parent.mkdir(parents=True, exist_ok=True)
        (root / name).write_text(content)
    return root


def test_detect_main_in_subfolder(tmp_path):
    make_tree(
        tmp_path,
        {
            "notes/main.tex": "\\documentclass{book}",
            "notes/chapters/gr/index.tex": "\\section{GR}",
            ".git/x.tex": "\\documentclass{article}",
        },
    )
    assert init.detect_main(tmp_path) == "notes/main.tex"


def test_detect_main_prefers_single_main_tex_among_roots(tmp_path):
    make_tree(
        tmp_path,
        {
            "paper/main.tex": "\\documentclass{article}",
            "response/letter.tex": "\\documentclass{letter}",
        },
    )
    assert init.detect_main(tmp_path) == "paper/main.tex"


def test_detect_main_prefers_root_main_tex(tmp_path):
    make_tree(tmp_path, {"main.tex": "", "paper.tex": "\\documentclass{article}"})
    assert init.detect_main(tmp_path) == "main.tex"


def test_detect_main_single_documentclass_root(tmp_path):
    make_tree(
        tmp_path,
        {
            "paper.tex": "% comment\n\\documentclass[11pt,\n  a4paper]{revtex4-2}\n",
            "fig.tex": "\\documentclass{standalone}",
            "chapter.tex": "\\documentclass[paper.tex]{subfiles}",
            "notes.tex": "% \\documentclass{article}\n\\section{x}",
            "readme.md": "\\documentclass{article}",
        },
    )
    assert init.detect_main(tmp_path) == "paper.tex"


def test_detect_main_ambiguous_or_missing(tmp_path):
    make_tree(tmp_path, {"a.tex": "\\documentclass{article}", "b.tex": "\\documentclass{book}"})
    with pytest.raises(init.OlfError, match=r"a\.tex, b\.tex.*--main|--main"):
        init.detect_main(tmp_path)
    empty = make_tree(tmp_path / "other", {"x.tex": "\\section{x}"})
    with pytest.raises(init.OlfError, match="--main"):
        init.detect_main(empty)


def test_exclude_lines_cover_olf_paths_only_when_adopted():
    assert "/.olf/" in init.exclude_lines(True)
    assert "/.claude/" in init.exclude_lines(True)
    assert "/.olf/" not in init.exclude_lines(False)
    assert "*.aux" in init.exclude_lines(False)


@pytest.mark.parametrize(
    "stderr",
    [
        "fatal: Authentication failed for 'https://git.overleaf.com/x/'",
        "fatal: could not read Username for 'https://git.overleaf.com': terminal prompts disabled",
        "fatal: unable to access 'x': The requested URL returned error: 403",
    ],
)
def test_auth_failure_classification(stderr):
    assert init.is_auth_failure(stderr)


def test_non_auth_failure():
    assert not init.is_auth_failure("fatal: repository 'x' not found")


# --- exclude block and hook -------------------------------------------------------------


def clone_of(env, branch="master"):
    env.remote(ID, {"main.tex": "hello\n"}, branch=branch)
    return env.clone_remote(ID, "clone")


def test_exclude_block_is_idempotent_and_preserves_user_lines(env):
    clone = clone_of(env)
    exclude = clone / ".git/info/exclude"
    exclude.write_text("# user\n*.bak")

    assert init.ensure_exclude_block(clone, ["*.aux", "*.log"])
    assert not init.ensure_exclude_block(clone, ["*.aux", "*.log"])
    assert init.ensure_exclude_block(clone, ["*.aux"])
    exclude.write_text(exclude.read_text() + "*.tmp\n")
    assert not init.ensure_exclude_block(clone, ["*.aux"])

    assert exclude.read_text() == (
        f"# user\n*.bak\n{init.BLOCK_START}\n*.aux\n{init.BLOCK_END}\n*.tmp\n"
    )
    (clone / "x.aux").write_text("")
    assert git(clone, "status", "--porcelain") == ""


def test_hook_install_is_idempotent_and_respects_foreign_hooks(env):
    clone = clone_of(env)
    assert init.install_pre_push_hook(clone) == "installed"
    assert init.install_pre_push_hook(clone) == "unchanged"
    hook = clone / ".git/hooks/pre-push"
    assert os.access(hook, os.X_OK)
    assert "# olf-managed" in hook.read_text()

    hook.write_text("#!/bin/sh\nexit 0\n")
    assert init.install_pre_push_hook(clone) == "foreign"
    assert hook.read_text() == "#!/bin/sh\nexit 0\n"


# --- end to end -------------------------------------------------------------------------


def test_fresh_clone_sets_up_workspace(env):
    env.remote(ID, {"main.tex": MAIN_TEX})
    result = env.init("--url", f"https://www.overleaf.com/project/{ID}", "ws")
    assert result.returncode == 0, result.stderr
    assert "cloned" in result.stdout and "workspace ready" in result.stdout

    ws = env.path / "ws"
    paper = ws / "paper"
    config = (ws / ".olf/config.toml").read_text()
    assert f'project_id = "{ID}"' in config
    assert 'project_dir = "paper"' in config
    assert 'main = "main.tex"' in config
    assert 'engine = "auto"' in config
    assert "[fmt]" not in config

    assert (paper / "main.tex").is_file()
    assert git(paper, "config", "pull.rebase") == "true"
    assert "# olf-managed" in (paper / ".git/hooks/pre-push").read_text()

    (paper / "main.aux").write_text("")
    (paper / "main.synctex.gz").write_text("")
    assert git(paper, "status", "--porcelain") == ""


def test_pull_rebases_despite_global_ff_only(env):
    ws = env.workspace()
    paper = ws / "paper"

    browser = env.clone_remote(ID, "browser")
    (browser / "other.tex").write_text("browser edit\n")
    git(browser, "add", ".")
    git(browser, "commit", "--quiet", "-m", "browser")
    git(browser, "push", "--quiet")

    (paper / "local.tex").write_text("local edit\n")
    git(paper, "add", ".")
    git(paper, "commit", "--quiet", "-m", "local")

    global_config = env.path / "gitconfig"
    global_config.write_text("[pull]\n\tff = only\n")
    pull = subprocess.run(
        ["git", "pull", "--quiet"],
        cwd=paper,
        env={**env.environ(), "GIT_CONFIG_GLOBAL": str(global_config)},
        capture_output=True,
        text=True,
    )
    assert pull.returncode == 0, pull.stderr
    assert git(paper, "log", "--format=%s", "-3") == "local\nbrowser\ninit"


def test_rerun_is_idempotent_and_keeps_user_config(env):
    ws = env.workspace()
    config_path = ws / ".olf/config.toml"
    edited = config_path.read_text().replace(
        'compiler = "pdflatex"', 'compiler = "xelatex" # as on Overleaf'
    )
    assert edited != config_path.read_text()
    config_path.write_text(edited)

    result = env.init(cwd=ws)
    assert result.returncode == 0, result.stderr
    assert "cloned" not in result.stdout and "wrote" not in result.stdout
    # From inside the checkout.
    assert env.init(cwd=ws / "paper").returncode == 0
    assert config_path.read_text() == edited


def test_rerun_reclones_missing_checkout(env):
    ws = env.workspace()
    shutil.rmtree(ws / "paper")
    result = env.init(cwd=ws)
    assert result.returncode == 0, result.stderr
    assert "cloned" in result.stdout
    assert (ws / "paper/main.tex").is_file()


def test_project_mismatch_is_an_error(env):
    env.workspace()
    result = env.init("--id", OTHER_ID, "ws")
    assert result.returncode == 1
    assert "project mismatch" in result.stderr


def test_refuses_inside_plain_git_repo(env):
    env.remote(ID, {"main.tex": MAIN_TEX})
    code = env.code_repo()
    result = env.init("--id", ID, "code/paper-ws")
    assert result.returncode == 7
    assert "code-paper" in result.stderr
    assert not (code / "paper-ws").exists()


def test_adopts_existing_overleaf_clone(env):
    env.remote(ID, {"main.tex": MAIN_TEX})
    clone = env.clone_remote(ID, "clone")

    result = env.init("clone")
    assert result.returncode == 0, result.stderr
    assert "adopting" in result.stdout
    config = (clone / ".olf/config.toml").read_text()
    assert 'project_dir = "."' in config and ID in config
    (clone / ".claude/skills").mkdir(parents=True)
    (clone / ".claude/skills/x").write_text("")
    assert git(clone, "status", "--porcelain") == ""

    # Subdirectories of the clone aren't workspaces of their own.
    (clone / "sub").mkdir()
    assert env.init("clone/sub").returncode == 0
    assert not (clone / "sub/.olf").exists()


def test_refuses_subdir_of_unadopted_overleaf_clone(env):
    env.remote(ID, {"main.tex": MAIN_TEX})
    env.clone_remote(ID, "clone")
    result = env.init("clone/ws")
    assert result.returncode == 1
    assert "inside the Overleaf clone" in result.stderr


def test_ambiguous_main_file_requires_flag(env):
    env.remote(ID, {"a.tex": MAIN_TEX, "b.tex": MAIN_TEX})
    result = env.init("--id", ID, "ws")
    assert result.returncode == 1
    assert "a.tex, b.tex" in result.stderr and "--main" in result.stderr

    assert env.init("--id", ID, "--main", "b.tex", "ws").returncode == 0
    config = (env.path / "ws/.olf/config.toml").read_text()
    assert 'main = "b.tex"' in config

    # An explicit --main replaces the configured one.
    assert env.init("--main", "a.tex", "ws").returncode == 0
    assert 'main = "a.tex"' in (env.path / "ws/.olf/config.toml").read_text()

    missing = env.init("--main", "nope.tex", "ws")
    assert missing.returncode == 1 and "nope.tex" in missing.stderr


def test_missing_project_is_an_error(env):
    result = env.init("ws")
    assert result.returncode == 1
    assert "--url" in result.stderr


def test_token_clone_uses_env_credential_helper(env):
    """The token reaches git only through the child env and a correctly keyed helper."""
    env.remote(ID, {"main.tex": MAIN_TEX})
    real_git = subprocess.run(
        ["which", "git"], capture_output=True, text=True, env=env.environ()
    ).stdout.strip()
    log = env.path / "git-calls.log"
    env.fake_tool(
        "git",
        f'printf \'%s|%s\\n\' "$OVERLEAF_GIT_TOKEN" "$*" >> "{log}"\nexec "{real_git}" "$@"',
    )

    result = env.init("--id", ID, "--token", "s3cret", "ws")
    assert result.returncode == 0, result.stderr
    assert "s3cret" not in result.stdout + result.stderr

    clone_call = next(line for line in log.read_text().splitlines() if " clone " in line)
    token, argv = clone_call.split("|", 1)
    assert token == "s3cret"
    assert "s3cret" not in argv
    assert "-c credential.helper= -c credential.helper=!f()" in argv

    paper = env.path / "ws/paper"
    helpers = git(paper, "config", "--local", "--get-all", "credential.helper").split("\n")
    assert helpers[0] == "" and helpers[1] == init.CREDENTIAL_HELPER
    assert "s3cret" not in (paper / ".git/config").read_text()

    # The persisted helper answers git with the token from the environment alone.
    fill = subprocess.run(
        ["git", "credential", "fill"],
        cwd=paper,
        input="protocol=https\nhost=git.overleaf.com\n\n",
        env={**env.environ(OVERLEAF_GIT_TOKEN="s3cret")},
        capture_output=True,
        text=True,
    )
    assert "username=git" in fill.stdout and "password=s3cret" in fill.stdout


def test_token_from_environment(env):
    env.remote(ID, {"main.tex": MAIN_TEX})
    result = env.init("--id", ID, "ws", OVERLEAF_GIT_TOKEN="from-env")
    assert result.returncode == 0, result.stderr


def test_auth_failure_exits_3(env):
    env.fake_tool("git", 'echo "fatal: Authentication failed for x" >&2; exit 128')
    result = env.init("--id", ID, "ws")
    assert result.returncode == 3
    assert "OVERLEAF_GIT_TOKEN" in result.stderr
