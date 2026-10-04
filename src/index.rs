//! The project index: `~/.olf/projects/<id>` symlinks pointing at workspaces.
//!
//! Only humans and olf traverse these links (`olf list`, `olf -p`); agents
//! work on real paths. Core functions take the index directory so tests don't
//! depend on process-wide environment.

use crate::config::{self, Config, Workspace};
use crate::error::{OlfError, Result, bail};
use crate::overleaf::ProjectId;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Ok,
    /// The link's target is gone (or no longer a workspace).
    Missing,
    /// The target's config names another project ID.
    Mismatch(String),
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Missing => "missing",
            Self::Mismatch(_) => "mismatch",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub id: String,
    pub path: PathBuf,
    pub status: Status,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RegisterStatus {
    Created,
    Unchanged,
    Relinked,
}

/// `$OLF_HOME/projects`, else `~/.olf/projects`.
pub fn index_dir() -> Result<PathBuf> {
    if let Some(home) = std::env::var_os("OLF_HOME").filter(|h| !h.is_empty()) {
        return Ok(PathBuf::from(home).join("projects"));
    }
    Ok(config::home_dir()?.join(".olf").join("projects"))
}

fn status_of(id: &str, target: &Path) -> Status {
    match Config::load(&Workspace::config_path(target)) {
        Ok(config) if config.project_id == id => Status::Ok,
        Ok(config) => Status::Mismatch(config.project_id),
        Err(_) => Status::Missing,
    }
}

pub fn entries() -> Result<Vec<Entry>> {
    entries_in(&index_dir()?)
}

pub fn entries_in(dir: &Path) -> Result<Vec<Entry>> {
    let read = match fs::read_dir(dir) {
        Ok(read) => read,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut entries = Vec::new();
    for entry in read {
        let entry = entry?;
        let Ok(path) = fs::read_link(entry.path()) else {
            continue; // not one of our links
        };
        let id = entry.file_name().to_string_lossy().into_owned();
        let status = status_of(&id, &path);
        entries.push(Entry { id, path, status });
    }
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(entries)
}

fn link_dir(target: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    return std::os::unix::fs::symlink(target, link);
    #[cfg(windows)]
    return std::os::windows::fs::symlink_dir(target, link);
}

fn canonical(root: &Path) -> Result<PathBuf> {
    Ok(root.canonicalize()?)
}

/// What `register` would do, failing on a conflict without touching anything.
pub fn check(id: &ProjectId, root: &Path, force: bool) -> Result<RegisterStatus> {
    check_in(&index_dir()?, id, root, force)
}

pub fn check_in(dir: &Path, id: &ProjectId, root: &Path, force: bool) -> Result<RegisterStatus> {
    let link = dir.join(id.to_string());
    let Ok(existing) = fs::read_link(&link) else {
        return Ok(RegisterStatus::Created);
    };
    let root = std::path::absolute(root)?;
    if same_dir(&existing, &root) {
        return Ok(RegisterStatus::Unchanged);
    }
    if force || status_of(&id.to_string(), &existing) != Status::Ok {
        return Ok(RegisterStatus::Relinked);
    }
    bail!(
        "project {id} is already registered at {}\n\
         cannot also register {}\n\
         hint: pass --force to point the index at the new workspace",
        existing.display(),
        root.display()
    )
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

pub fn register(id: &ProjectId, root: &Path, force: bool) -> Result<RegisterStatus> {
    register_in(&index_dir()?, id, root, force)
}

pub fn register_in(dir: &Path, id: &ProjectId, root: &Path, force: bool) -> Result<RegisterStatus> {
    let status = check_in(dir, id, root, force)?;
    if status == RegisterStatus::Unchanged {
        return Ok(status);
    }
    fs::create_dir_all(dir)?;
    let link = dir.join(id.to_string());
    if status == RegisterStatus::Relinked {
        fs::remove_file(&link)?;
    }
    link_dir(&canonical(root)?, &link)?;
    Ok(status)
}

/// The workspace root matching `query`: an exact ID, else a unique ID prefix,
/// else a unique workspace directory name. Only healthy links are considered.
pub fn resolve(query: &str) -> Result<PathBuf> {
    resolve_in(&index_dir()?, query)
}

pub fn resolve_in(dir: &Path, query: &str) -> Result<PathBuf> {
    let live: Vec<Entry> = entries_in(dir)?
        .into_iter()
        .filter(|e| e.status == Status::Ok)
        .collect();
    let by_name = |e: &Entry| e.path.file_name().is_some_and(|n| n == query);
    let stages: [Vec<&Entry>; 3] = [
        live.iter().filter(|e| e.id == query).collect(),
        live.iter().filter(|e| e.id.starts_with(query)).collect(),
        live.iter().filter(|e| by_name(e)).collect(),
    ];
    for matches in stages {
        match matches.as_slice() {
            [] => {}
            [one] => return Ok(one.path.clone()),
            many => {
                let candidates: Vec<String> = many
                    .iter()
                    .map(|e| format!("  {}  {}", e.id, e.path.display()))
                    .collect();
                return Err(OlfError::UnknownProject(format!(
                    "`{query}` matches several projects:\n{}\n\
                     hint: use a longer ID prefix (see `olf list`)",
                    candidates.join("\n")
                )));
            }
        }
    }
    Err(OlfError::UnknownProject(format!(
        "no registered project matches `{query}`\n\
         hint: see `olf list`, or register a workspace with `olf init`"
    )))
}

/// Remove links that aren't healthy; returns what was removed.
pub fn prune() -> Result<Vec<Entry>> {
    prune_in(&index_dir()?)
}

pub fn prune_in(dir: &Path) -> Result<Vec<Entry>> {
    let bad: Vec<Entry> = entries_in(dir)?
        .into_iter()
        .filter(|e| e.status != Status::Ok)
        .collect();
    for entry in &bad {
        fs::remove_file(dir.join(&entry.id))?;
    }
    Ok(bad)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "64f0c0ffee0123456789abcd";
    const B: &str = "64f0c0ffee9999999999abcd";
    const C: &str = "0123456789abcdef01234567";

    fn id(s: &str) -> ProjectId {
        ProjectId::parse(s).unwrap()
    }

    fn workspace(base: &Path, name: &str, project: &str) -> PathBuf {
        let root = base.join(name);
        Config::new(project.into(), "paper".into())
            .write_merged(&Workspace::config_path(&root))
            .unwrap();
        root.canonicalize().unwrap()
    }

    struct Fixture {
        tmp: tempfile::TempDir,
        index: PathBuf,
    }

    fn fixture() -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let index = tmp.path().join("index");
        Fixture { tmp, index }
    }

    #[test]
    fn creates_then_is_unchanged() {
        let f = fixture();
        let ws = workspace(f.tmp.path(), "one", A);
        assert_eq!(
            register_in(&f.index, &id(A), &ws, false).unwrap(),
            RegisterStatus::Created
        );
        assert_eq!(
            register_in(&f.index, &id(A), &ws, false).unwrap(),
            RegisterStatus::Unchanged
        );
        let entries = entries_in(&f.index).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, ws);
        assert_eq!(entries[0].status, Status::Ok);
    }

    #[test]
    fn missing_index_dir_is_empty() {
        let f = fixture();
        assert!(entries_in(&f.index).unwrap().is_empty());
    }

    #[test]
    fn relinks_dangling_and_mismatched_links() {
        let f = fixture();
        let old = workspace(f.tmp.path(), "old", A);
        register_in(&f.index, &id(A), &old, false).unwrap();
        fs::remove_dir_all(&old).unwrap();
        assert_eq!(entries_in(&f.index).unwrap()[0].status, Status::Missing);

        let new = workspace(f.tmp.path(), "new", A);
        assert_eq!(
            register_in(&f.index, &id(A), &new, false).unwrap(),
            RegisterStatus::Relinked
        );
        assert_eq!(entries_in(&f.index).unwrap()[0].path, new);

        // The workspace now claims a different project: also replaceable.
        fs::write(
            Workspace::config_path(&new),
            format!("project_id = \"{B}\"\n"),
        )
        .unwrap();
        assert_eq!(
            entries_in(&f.index).unwrap()[0].status,
            Status::Mismatch(B.into())
        );
        let third = workspace(f.tmp.path(), "third", A);
        assert_eq!(
            register_in(&f.index, &id(A), &third, false).unwrap(),
            RegisterStatus::Relinked
        );
    }

    #[test]
    fn conflicts_with_live_workspace_unless_forced() {
        let f = fixture();
        let one = workspace(f.tmp.path(), "one", A);
        let two = workspace(f.tmp.path(), "two", A);
        register_in(&f.index, &id(A), &one, false).unwrap();

        let err = register_in(&f.index, &id(A), &two, false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("one") && err.contains("two") && err.contains("--force"));
        assert!(check_in(&f.index, &id(A), &two, false).is_err());
        assert_eq!(entries_in(&f.index).unwrap()[0].path, one, "left untouched");

        assert_eq!(
            register_in(&f.index, &id(A), &two, true).unwrap(),
            RegisterStatus::Relinked
        );
        assert_eq!(entries_in(&f.index).unwrap()[0].path, two);
    }

    #[test]
    fn resolves_by_id_prefix_and_name() {
        let f = fixture();
        let a = workspace(f.tmp.path(), "dark-matter", A);
        let b = workspace(f.tmp.path(), "thesis", B);
        let c = workspace(f.tmp.path(), "other", C);
        for (project, ws) in [(A, &a), (B, &b), (C, &c)] {
            register_in(&f.index, &id(project), ws, false).unwrap();
        }

        assert_eq!(resolve_in(&f.index, A).unwrap(), a);
        assert_eq!(resolve_in(&f.index, "0123").unwrap(), c);
        assert_eq!(resolve_in(&f.index, "thesis").unwrap(), b);

        let ambiguous = resolve_in(&f.index, "64f0").unwrap_err();
        assert!(matches!(ambiguous, OlfError::UnknownProject(_)));
        let text = ambiguous.to_string();
        assert!(text.contains(A) && text.contains(B), "{text}");

        let unknown = resolve_in(&f.index, "nope").unwrap_err();
        assert!(matches!(unknown, OlfError::UnknownProject(_)));
        assert!(unknown.to_string().contains("olf list"));
    }

    #[test]
    fn resolve_ignores_unhealthy_links() {
        let f = fixture();
        let a = workspace(f.tmp.path(), "gone", A);
        register_in(&f.index, &id(A), &a, false).unwrap();
        fs::remove_dir_all(&a).unwrap();
        assert!(resolve_in(&f.index, A).is_err());
        assert!(resolve_in(&f.index, "gone").is_err());
    }

    #[test]
    fn prune_removes_only_bad_links() {
        let f = fixture();
        let a = workspace(f.tmp.path(), "a", A);
        let b = workspace(f.tmp.path(), "b", B);
        register_in(&f.index, &id(A), &a, false).unwrap();
        register_in(&f.index, &id(B), &b, false).unwrap();
        fs::remove_dir_all(&b).unwrap();

        let pruned = prune_in(&f.index).unwrap();
        assert_eq!(pruned.len(), 1);
        assert_eq!(pruned[0].id, B);
        let left = entries_in(&f.index).unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].id, A);
    }
}
