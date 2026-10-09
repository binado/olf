import json

SETTINGS = ".claude/settings.local.json"


def dirs(repo):
    return json.loads((repo / SETTINGS).read_text())["permissions"]["additionalDirectories"]


def test_grants_code_repo_and_excludes_settings(env):
    ws = env.workspace()
    code = env.code_repo()

    result = env.grant("--workspace", ws, cwd=code)
    assert result.returncode == 0, result.stderr
    assert "granted" in result.stdout
    assert dirs(code) == [str(ws / "paper")]
    settings = json.loads((code / SETTINGS).read_text())
    assert settings["env"]["OLF_PROJECT_DIR"] == str(ws / "paper")
    assert (code / SETTINGS).read_text().endswith("}\n")
    assert not (ws / SETTINGS).exists(), "must not write into the workspace"
    assert "/.claude/settings.local.json" in (code / ".git/info/exclude").read_text()

    again = env.grant("--workspace", ws, cwd=code)
    assert again.returncode == 0
    assert "already has access" in again.stdout
    assert len(dirs(code)) == 1


def test_preserves_existing_settings_and_key_order(env):
    ws = env.workspace()
    code = env.code_repo()
    (code / ".claude").mkdir()
    (code / SETTINGS).write_text(
        '{"zeta":1,"permissions":{"allow":["Bash(ls)"],"additionalDirectories":["/other"]},'
        '"env":{"A":"b"},"alpha":2}'
    )
    assert env.grant("--workspace", ws, cwd=code).returncode == 0
    text = (code / SETTINGS).read_text()
    data = json.loads(text)
    assert data["permissions"]["additionalDirectories"] == ["/other", str(ws / "paper")]
    assert data["permissions"]["allow"] == ["Bash(ls)"]
    assert data["env"]["A"] == "b"
    assert text.index("zeta") < text.index("permissions") < text.index("alpha")
    assert text.startswith('{\n  "zeta"')


def test_skips_exclude_when_already_ignored(env):
    ws = env.workspace()
    code = env.code_repo()
    (code / ".gitignore").write_text(".claude/settings.local.json\n")
    assert env.grant("--workspace", ws, cwd=code).returncode == 0
    exclude = code / ".git/info/exclude"
    assert "settings.local.json" not in (exclude.read_text() if exclude.exists() else "")


def test_subdir_snaps_to_repo_root(env):
    ws = env.workspace()
    code = env.code_repo()
    (code / "src/deep").mkdir(parents=True)
    assert env.grant("--workspace", ws, cwd=code / "src/deep").returncode == 0
    assert (code / SETTINGS).is_file()
    assert not (code / "src/deep/.claude").exists()


def test_explicit_repo_flag(env):
    ws = env.workspace()
    code = env.code_repo()
    assert env.grant("--workspace", ws, "--repo", code, cwd=env.home).returncode == 0
    assert (code / SETTINGS).is_file()


def test_non_git_dir_works_without_exclude(env):
    ws = env.workspace()
    plain = env.path / "plain"
    plain.mkdir()
    result = env.grant("--workspace", ws, cwd=plain)
    assert result.returncode == 0
    assert "excluded" not in result.stdout
    assert (plain / SETTINGS).is_file()


def test_invalid_or_mistyped_json_is_untouched(env):
    ws = env.workspace()
    code = env.code_repo()
    (code / ".claude").mkdir()
    for bad in ("{ nope", '{"permissions":[]}', '{"permissions":{"additionalDirectories":"x"}}'):
        (code / SETTINGS).write_text(bad)
        result = env.grant("--workspace", ws, cwd=code)
        assert result.returncode == 1, bad
        assert (code / SETTINGS).read_text() == bad


def test_inside_workspace_is_a_noop(env):
    ws = env.workspace()
    result = env.grant(cwd=ws)
    assert result.returncode == 0
    assert "nothing to grant" in result.stdout
    assert not (ws / SETTINGS).exists()


def test_outside_any_workspace_fails(env):
    code = env.code_repo()
    assert env.grant(cwd=code).returncode == 2


def test_workspace_found_through_env(env):
    ws = env.workspace()
    code = env.code_repo()
    result = env.grant(cwd=code, OLF_PROJECT_DIR=str(ws / "paper"))
    assert result.returncode == 0, result.stderr
    assert dirs(code) == [str(ws / "paper")]
