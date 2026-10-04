//! `olf path`: print the checkout directory.

use crate::config::Workspace;
use crate::error::Result;

pub fn run() -> Result<()> {
    let ws = Workspace::discover(&std::env::current_dir()?)?;
    println!("{}", ws.checkout_dir().display());
    Ok(())
}
