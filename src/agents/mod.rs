//! Per-agent knowledge: how to persistently grant write access to the
//! checkout, and which flags grant it for a single session.

mod claude;

use crate::cli::Agent;
use crate::error::Result;
use std::ffi::OsString;
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub enum GrantStatus {
    Added,
    Unchanged,
}

/// Persistently allow `agent`, running in `repo`, to write `checkout`.
pub fn grant(agent: Agent, repo: &Path, checkout: &Path) -> Result<GrantStatus> {
    match agent {
        Agent::Claude => claude::grant(repo, checkout),
    }
}

/// Path of the per-repo file `grant` writes, relative to the repo root.
pub fn settings_file(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => claude::SETTINGS,
    }
}

/// Flags that give `program` one-session access to `checkout`, or `None` if
/// olf doesn't know the program. Matches on the file name, so `/opt/bin/claude` works.
pub fn exec_args(program: &str, checkout: &Path) -> Option<Vec<OsString>> {
    let name = Path::new(program).file_name()?.to_str()?;
    match name {
        "claude" => Some(claude::exec_flags(checkout)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_agents_by_file_name() {
        let checkout = Path::new("/ws/paper");
        assert!(exec_args("claude", checkout).is_some());
        assert!(exec_args("/opt/bin/claude", checkout).is_some());
        assert!(exec_args("sh", checkout).is_none());
    }
}
