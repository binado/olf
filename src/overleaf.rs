//! Overleaf project IDs and the URLs derived from them.

use crate::error::{OlfError, Result};
use std::fmt;

const DEFAULT_GIT_BASE: &str = "https://git.overleaf.com";
const WEB_BASE: &str = "https://www.overleaf.com";

/// Overleaf project ID: a MongoDB `ObjectId`, i.e. 24 lowercase hex chars.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectId(String);

impl ProjectId {
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();
        if s.len() == 24 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            Ok(Self(s.to_owned()))
        } else {
            Err(OlfError::Error(format!(
                "invalid Overleaf project ID `{s}` (expected 24 lowercase hex characters)"
            )))
        }
    }

    /// Extract the project ID from any overleaf.com project URL or bridge URL.
    pub fn from_url(url: &str) -> Result<Self> {
        let invalid = |why: &str| {
            OlfError::Error(format!(
                "cannot get a project ID from `{url}`: {why}\n\
                 hint: use the URL from your browser's address bar \
                 (https://www.overleaf.com/project/<id>) or the git clone URL"
            ))
        };
        let rest = url
            .trim()
            .strip_prefix("https://")
            .or_else(|| url.trim().strip_prefix("http://"))
            .ok_or_else(|| invalid("not an http(s) URL"))?;
        let rest = rest.split(['?', '#']).next().unwrap_or_default();
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        let host = authority.rsplit('@').next().unwrap_or(authority);
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        match (host, segments.as_slice()) {
            ("git.overleaf.com", [id, ..]) => Self::parse(id.trim_end_matches(".git")),
            ("www.overleaf.com" | "overleaf.com", ["project", id, ..]) => Self::parse(id),
            ("www.overleaf.com" | "overleaf.com", ["read" | "edit", ..]) => Err(invalid(
                "this is a share link, which doesn't contain the project ID; \
                 open it, join the project, and copy the URL from the address bar",
            )),
            ("www.overleaf.com" | "overleaf.com" | "git.overleaf.com", _) => {
                Err(invalid("no project ID in the path"))
            }
            _ => Err(invalid("not an overleaf.com URL")),
        }
    }
}

impl fmt::Display for ProjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Base URL of the git bridge. `OLF_GIT_BASE` (undocumented) lets tests point
/// it at a directory of local bare repositories named by project ID.
fn git_base() -> String {
    std::env::var("OLF_GIT_BASE")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_GIT_BASE.to_owned())
}

pub fn git_url(id: &ProjectId) -> String {
    format!("{}/{id}", git_base().trim_end_matches('/'))
}

pub fn web_url(id: &ProjectId) -> String {
    format!("{WEB_BASE}/project/{id}")
}

/// The project ID behind a git remote URL, if it points at the bridge.
pub fn id_from_remote(url: &str) -> Option<ProjectId> {
    let base = git_base();
    let base = base.trim_end_matches('/');
    if let Some(rest) = url.strip_prefix(base).and_then(|r| r.strip_prefix('/')) {
        let id = rest.trim_end_matches('/').trim_end_matches(".git");
        return ProjectId::parse(id).ok();
    }
    ProjectId::from_url(url)
        .ok()
        .filter(|_| url.contains("git.overleaf.com"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "64f0c0ffee0123456789abcd";

    fn id(url: &str) -> String {
        ProjectId::from_url(url).unwrap().to_string()
    }

    fn err(url: &str) -> String {
        ProjectId::from_url(url).unwrap_err().to_string()
    }

    #[test]
    fn parses_bare_ids() {
        assert_eq!(ProjectId::parse(ID).unwrap().to_string(), ID);
        assert!(ProjectId::parse("64F0C0FFEE0123456789ABCD").is_err());
        assert!(ProjectId::parse("64f0c0ffee").is_err());
        assert!(ProjectId::parse("zzzzzzzzzzzzzzzzzzzzzzzz").is_err());
    }

    #[test]
    fn parses_project_urls() {
        assert_eq!(id(&format!("https://www.overleaf.com/project/{ID}")), ID);
        assert_eq!(id(&format!("https://www.overleaf.com/project/{ID}/")), ID);
        assert_eq!(
            id(&format!(
                "https://www.overleaf.com/project/{ID}/detached?x=1#y"
            )),
            ID
        );
        assert_eq!(id(&format!("https://overleaf.com/project/{ID}?a=b")), ID);
        assert_eq!(id(&format!("http://www.overleaf.com/project/{ID}")), ID);
    }

    #[test]
    fn parses_git_urls() {
        assert_eq!(id(&format!("https://git.overleaf.com/{ID}")), ID);
        assert_eq!(id(&format!("https://git.overleaf.com/{ID}.git")), ID);
        assert_eq!(id(&format!("https://git@git.overleaf.com/{ID}")), ID);
    }

    #[test]
    fn rejects_share_links() {
        assert!(err("https://www.overleaf.com/read/abcdefghijkl#1a2b3c").contains("share link"));
        assert!(err("https://www.overleaf.com/1234567890abcdefgh").contains("no project ID"));
    }

    #[test]
    fn rejects_other_urls() {
        assert!(err("https://github.com/x/y").contains("not an overleaf.com URL"));
        assert!(err(ID).contains("not an http(s) URL"));
        assert!(err("https://www.overleaf.com/project").contains("no project ID"));
        assert!(
            err("https://www.overleaf.com/project/1234").contains("invalid Overleaf project ID")
        );
    }

    #[test]
    fn builds_urls() {
        let pid = ProjectId::parse(ID).unwrap();
        assert_eq!(
            web_url(&pid),
            format!("https://www.overleaf.com/project/{ID}")
        );
        // Tests don't set OLF_GIT_BASE in-process, so the default applies.
        assert_eq!(git_url(&pid), format!("https://git.overleaf.com/{ID}"));
    }

    #[test]
    fn recognises_bridge_remotes() {
        let url = format!("https://git.overleaf.com/{ID}");
        assert_eq!(id_from_remote(&url).unwrap().to_string(), ID);
        assert!(id_from_remote("git@github.com:x/y.git").is_none());
        assert!(id_from_remote(&format!("https://www.overleaf.com/project/{ID}")).is_none());
    }
}
