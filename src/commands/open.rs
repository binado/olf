//! `olf open`: open the project on overleaf.com.

use crate::cli::OpenArgs;
use crate::config::Workspace;
use crate::error::{OlfError, Result};
use crate::overleaf::{self, ProjectId};

pub fn run(args: &OpenArgs) -> Result<()> {
    let ws = Workspace::discover(&std::env::current_dir()?)?;
    let url = overleaf::web_url(&ProjectId::parse(&ws.config.project_id)?);
    if args.print {
        println!("{url}");
        return Ok(());
    }
    open::that(&url)
        .map_err(|e| OlfError::Error(format!("cannot open a browser: {e}\nhint: visit {url}")))
}
