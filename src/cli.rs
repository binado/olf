use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Work on Overleaf projects from a local git checkout.
#[derive(Parser)]
#[command(name = "olf", version, about)]
pub struct Cli {
    /// Run as if olf was started in <dir> (like `git -C`)
    #[arg(short = 'C', global = true, value_name = "DIR")]
    pub dir: Option<PathBuf>,

    /// Run against a registered project: full ID, unique ID prefix, or workspace
    /// directory name (see `olf list`)
    #[arg(
        id = "project_query",
        short = 'p',
        global = true,
        value_name = "PROJECT"
    )]
    pub project: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Clone and/or set up an Overleaf project (idempotent)
    Init(InitArgs),
    /// Compile locally with latexmk or tectonic
    Build(BuildArgs),
    /// List registered projects
    List(ListArgs),
    /// Print the checkout path of the project
    Path,
    /// Open the checkout in $VISUAL / $EDITOR
    Edit,
    /// Open the project on overleaf.com
    Open(OpenArgs),
    /// Persistently allow an agent in a repo to write the checkout
    ///
    /// Run by the human, not the agent: under the sandbox an agent cannot
    /// edit its own `.claude` settings, which is intended.
    Grant(GrantArgs),
    /// Launch an agent with access to the checkout for one session
    Exec(ExecArgs),
}

#[derive(Args)]
pub struct InitArgs {
    /// Overleaf project ID
    #[arg(long, conflicts_with = "url")]
    pub id: Option<String>,
    /// Any overleaf.com project URL
    #[arg(long)]
    pub url: Option<String>,
    /// Git token (used for this setup; future Git operations read `OVERLEAF_GIT_TOKEN`)
    #[arg(long, env = "OVERLEAF_GIT_TOKEN", hide_env_values = true)]
    pub token: Option<String>,
    /// Main .tex file, relative to the checkout (skips auto-detection)
    #[arg(long)]
    pub main: Option<PathBuf>,
    /// Relink the index if this project is registered elsewhere
    #[arg(long)]
    pub force: bool,
    /// Also let an agent in the current repo write the checkout (repeatable)
    #[arg(long, value_enum, value_name = "AGENT")]
    pub grant: Vec<Agent>,
    /// Workspace directory (default: cwd)
    pub path: Option<PathBuf>,
}

#[derive(Args)]
pub struct GrantArgs {
    /// Agent to grant access to
    #[arg(value_enum)]
    pub agent: Agent,
    /// Repo the agent runs in (default: the git repo containing the cwd)
    #[arg(long)]
    pub repo: Option<PathBuf>,
}

#[derive(Args)]
pub struct ExecArgs {
    /// Agent command line, after `--` (e.g. `olf exec -- claude --resume`)
    #[arg(last = true, required = true)]
    pub command: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Agent {
    Claude,
}

#[derive(Args)]
pub struct BuildArgs {
    /// Emit machine-readable JSON instead of human text
    #[arg(long)]
    pub json: bool,
    /// Override engine from .olf/config.toml
    #[arg(long, value_enum)]
    pub engine: Option<Engine>,
    /// Override main file from .olf/config.toml
    #[arg(long)]
    pub main: Option<PathBuf>,
}

#[derive(Args)]
pub struct ListArgs {
    /// Emit machine-readable JSON instead of human text
    #[arg(long)]
    pub json: bool,
    /// Remove links whose workspace no longer exists
    #[arg(long)]
    pub prune: bool,
}

#[derive(Args)]
pub struct OpenArgs {
    /// Print the URL instead of opening a browser
    #[arg(long)]
    pub print: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    #[default]
    Auto,
    Latexmk,
    Tectonic,
}
