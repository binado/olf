//! `olf grant`: persistently let an agent running in a code repo write the checkout.

use crate::agents::{self, GrantStatus};
use crate::cli::{Agent, GrantArgs};
use crate::config::Workspace;
use crate::error::Result;
use crate::git;
use std::path::Path;

pub fn run(args: &GrantArgs, invocation_dir: &Path) -> Result<()> {
    let ws = Workspace::discover(&std::env::current_dir()?)?;
    let start = match &args.repo {
        Some(repo) => std::path::absolute(invocation_dir.join(repo))?,
        None => invocation_dir.to_path_buf(),
    };
    grant_to(args.agent, &start, &ws)
}

/// Grant `agent` in the repo containing `start` write access to `ws`'s checkout.
pub fn grant_to(agent: Agent, start: &Path, ws: &Workspace) -> Result<()> {
    let checkout = ws.checkout_dir();
    let is_git = start.is_dir() && git::is_inside_work_tree(start);
    // Claude Code's project settings live at the repo root.
    let repo = if is_git {
        git::toplevel(start)?
    } else {
        start.to_path_buf()
    };

    let canonical = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    if canonical(&repo).starts_with(canonical(&ws.root)) {
        println!(
            "{} is inside the workspace; nothing to grant",
            repo.display()
        );
        return Ok(());
    }

    let name = format!("{agent:?}").to_lowercase();
    match agents::grant(agent, &repo, &checkout)? {
        GrantStatus::Added => println!(
            "granted {name} in {} write access to {}",
            repo.display(),
            checkout.display()
        ),
        GrantStatus::Unchanged => println!(
            "{name} in {} already has access to {}",
            repo.display(),
            checkout.display()
        ),
    }

    if is_git {
        let file = agents::settings_file(agent);
        if !git::is_ignored(&repo, Path::new(file))? {
            git::ensure_exclude_block(&repo, &[&format!("/{file}")])?;
            println!(
                "excluded {file} in {} via .git/info/exclude",
                repo.display()
            );
        }
    }
    Ok(())
}
