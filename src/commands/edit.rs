//! `olf edit`: open the checkout in the user's editor.

use crate::config::Workspace;
use crate::error::{OlfError, Result, bail};
use std::process::Command;

pub fn run() -> Result<()> {
    let ws = Workspace::discover(&std::env::current_dir()?)?;
    let editor = ["VISUAL", "EDITOR"]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|v| !v.trim().is_empty()));
    let Some(editor) = editor else {
        bail!("no editor configured; set $VISUAL or $EDITOR");
    };
    let checkout = ws.checkout_dir();
    let status = command(&editor, &checkout)
        .status()
        .map_err(|e| OlfError::Error(format!("cannot run `{editor}`: {e}")))?;
    if !status.success() {
        bail!("`{editor}` exited with {status}");
    }
    Ok(())
}

/// Run the editor git-style, through the shell, so `code -w` works.
#[cfg(unix)]
fn command(editor: &str, checkout: &std::path::Path) -> Command {
    let mut cmd = Command::new("sh");
    cmd.arg("-c")
        .arg(format!("{editor} \"$@\""))
        .arg("olf-edit")
        .arg(checkout)
        .current_dir(checkout);
    cmd
}

#[cfg(windows)]
fn command(editor: &str, checkout: &std::path::Path) -> Command {
    let mut words = editor.split_whitespace();
    let mut cmd = Command::new(words.next().unwrap_or(editor));
    cmd.args(words).arg(checkout).current_dir(checkout);
    cmd
}
