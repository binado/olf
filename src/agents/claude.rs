//! Claude Code: sandboxed Bash and file tools may write the working
//! directory and anything in `permissions.additionalDirectories` / `--add-dir`.

use super::GrantStatus;
use crate::error::{Result, bail};
use serde_json::{Map, Value, json};
use std::ffi::OsString;
use std::fs;
use std::path::Path;

pub const SETTINGS: &str = ".claude/settings.local.json";

pub fn exec_flags(checkout: &Path) -> Vec<OsString> {
    vec!["--add-dir".into(), checkout.into()]
}

/// Add `checkout` to `permissions.additionalDirectories` in the repo's local
/// settings, leaving everything else (including key order) alone.
pub fn grant(repo: &Path, checkout: &Path) -> Result<GrantStatus> {
    let path = repo.join(SETTINGS);
    let mut root = match fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(map)) => map,
            Ok(_) => bail!("{} isn't a JSON object; left it alone", path.display()),
            Err(e) => bail!("{} isn't valid JSON ({e}); left it alone", path.display()),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Map::new(),
        Err(e) => return Err(e.into()),
    };

    let permissions = root
        .entry("permissions")
        .or_insert_with(|| json!({}))
        .as_object_mut();
    let Some(permissions) = permissions else {
        bail!(
            "`permissions` in {} isn't an object; left it alone",
            path.display()
        );
    };
    let dirs = permissions
        .entry("additionalDirectories")
        .or_insert_with(|| json!([]))
        .as_array_mut();
    let Some(dirs) = dirs else {
        bail!(
            "`permissions.additionalDirectories` in {} isn't an array; left it alone",
            path.display()
        );
    };

    let wanted = canonical(checkout);
    if dirs
        .iter()
        .filter_map(Value::as_str)
        .any(|d| canonical(Path::new(d)) == wanted)
    {
        return Ok(GrantStatus::Unchanged);
    }
    // Canonical, so `../ws` style paths from the command line are stored clean.
    dirs.push(Value::String(wanted.to_string_lossy().into_owned()));

    fs::create_dir_all(repo.join(".claude"))?;
    let mut text = serde_json::to_string_pretty(&Value::Object(root))
        .map_err(|e| crate::error::OlfError::Error(e.to_string()))?;
    text.push('\n');
    fs::write(&path, text)?;
    Ok(GrantStatus::Added)
}

fn canonical(path: &Path) -> std::path::PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(repo: &Path) -> String {
        fs::read_to_string(repo.join(SETTINGS)).unwrap()
    }

    fn write(repo: &Path, text: &str) {
        fs::create_dir_all(repo.join(".claude")).unwrap();
        fs::write(repo.join(SETTINGS), text).unwrap();
    }

    #[test]
    fn creates_settings_from_scratch() {
        let repo = tempfile::tempdir().unwrap();
        let status = grant(repo.path(), Path::new("/ws/paper")).unwrap();
        assert_eq!(status, GrantStatus::Added);
        let json: Value = serde_json::from_str(&read(repo.path())).unwrap();
        assert_eq!(
            json["permissions"]["additionalDirectories"],
            json!(["/ws/paper"])
        );
        assert!(read(repo.path()).ends_with('\n'));
    }

    #[test]
    fn merges_preserving_keys_and_order() {
        let repo = tempfile::tempdir().unwrap();
        write(
            repo.path(),
            r#"{"zeta":1,"permissions":{"allow":["Bash(ls)"],"additionalDirectories":["/other"]},"alpha":2}"#,
        );
        grant(repo.path(), Path::new("/ws/paper")).unwrap();
        let text = read(repo.path());
        let json: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            json["permissions"]["additionalDirectories"],
            json!(["/other", "/ws/paper"])
        );
        assert_eq!(json["permissions"]["allow"], json!(["Bash(ls)"]));
        let (zeta, perms, alpha) = (
            text.find("zeta").unwrap(),
            text.find("permissions").unwrap(),
            text.find("alpha").unwrap(),
        );
        assert!(zeta < perms && perms < alpha, "{text}");
    }

    #[test]
    fn is_idempotent() {
        let repo = tempfile::tempdir().unwrap();
        grant(repo.path(), Path::new("/ws/paper")).unwrap();
        let before = read(repo.path());
        let status = grant(repo.path(), Path::new("/ws/paper")).unwrap();
        assert_eq!(status, GrantStatus::Unchanged);
        assert_eq!(read(repo.path()), before);
    }

    #[test]
    fn invalid_json_is_left_untouched() {
        let repo = tempfile::tempdir().unwrap();
        write(repo.path(), "{ not json");
        assert!(grant(repo.path(), Path::new("/ws/paper")).is_err());
        assert_eq!(read(repo.path()), "{ not json");
    }

    #[test]
    fn wrongly_typed_permissions_is_an_error() {
        let repo = tempfile::tempdir().unwrap();
        write(repo.path(), r#"{"permissions":[]}"#);
        assert!(grant(repo.path(), Path::new("/ws/paper")).is_err());
        assert_eq!(read(repo.path()), r#"{"permissions":[]}"#);

        write(
            repo.path(),
            r#"{"permissions":{"additionalDirectories":"x"}}"#,
        );
        assert!(grant(repo.path(), Path::new("/ws/paper")).is_err());
    }
}
