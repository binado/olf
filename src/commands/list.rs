//! `olf list`: show registered projects.

use crate::cli::ListArgs;
use crate::error::Result;
use crate::index::{self, Entry, Status};

pub fn run(args: &ListArgs, json: bool) -> Result<()> {
    let mut entries = index::entries()?;
    if args.prune {
        for entry in index::prune()? {
            println!("pruned {} -> {}", entry.id, entry.path.display());
        }
        entries.retain(|e| e.status == Status::Ok);
    }

    if json {
        let rows: Vec<_> = entries
            .iter()
            .map(|e| {
                serde_json::json!({
                    "id": e.id,
                    "path": e.path,
                    "status": e.status.as_str(),
                })
            })
            .collect();
        println!("{}", serde_json::Value::Array(rows));
        return Ok(());
    }

    if entries.is_empty() {
        eprintln!("no projects registered (run olf init)");
        return Ok(());
    }
    for entry in &entries {
        println!("{}", line(entry));
    }
    if entries.iter().any(|e| e.status != Status::Ok) {
        eprintln!("hint: run `olf list --prune` to remove stale entries");
    }
    Ok(())
}

fn line(entry: &Entry) -> String {
    let base = format!("{}\t{}", entry.id, entry.path.display());
    match &entry.status {
        Status::Ok => base,
        Status::Missing => format!("{base}\t(missing)"),
        Status::Mismatch(other) => format!("{base}\t(project mismatch: {other})"),
    }
}
