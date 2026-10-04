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
    ///
    /// Short-only: `skill install --project` already owns the long name.
    #[arg(
        id = "project_query",
        short = 'p',
        global = true,
        value_name = "PROJECT"
    )]
    pub project: Option<String>,

    /// Emit machine-readable JSON instead of human text (supported by `build` and `list`)
    #[arg(long, global = true)]
    pub json: bool,

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
    /// Manage bundled agent skills
    #[command(subcommand)]
    Skill(SkillCommand),
}

#[derive(Args)]
pub struct InitArgs {
    /// Overleaf project ID
    #[arg(long, conflicts_with = "url")]
    pub id: Option<String>,
    /// Any overleaf.com project URL
    #[arg(long)]
    pub url: Option<String>,
    /// Git token (stored in the credential helper, never in config)
    #[arg(long, env = "OVERLEAF_GIT_TOKEN", hide_env_values = true)]
    pub token: Option<String>,
    /// Main .tex file, relative to the checkout (skips auto-detection)
    #[arg(long)]
    pub main: Option<PathBuf>,
    /// Relink the index if this project is registered elsewhere
    #[arg(long)]
    pub force: bool,
    /// Workspace directory (default: cwd)
    pub path: Option<PathBuf>,
}

#[derive(Args)]
pub struct BuildArgs {
    /// Override engine from .olf/config.toml
    #[arg(long, value_enum)]
    pub engine: Option<Engine>,
    /// Override main file from .olf/config.toml
    #[arg(long)]
    pub main: Option<PathBuf>,
}

#[derive(Args)]
pub struct ListArgs {
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

#[derive(Subcommand)]
pub enum SkillCommand {
    /// Install skills (default: ~/.claude/skills/)
    Install {
        /// Install into the workspace's .claude/skills/ instead
        #[arg(long)]
        project: bool,
        /// Overwrite existing skill files
        #[arg(long)]
        force: bool,
    },
    /// List bundled skills
    List,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    #[default]
    Auto,
    Latexmk,
    Tectonic,
}
