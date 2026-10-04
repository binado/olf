use std::path::PathBuf;

/// Process exit codes; agents branch on these instead of parsing text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Exit {
    Error = 1,
    NotAProject = 2,
    AuthFailed = 3,
    BuildFailed = 4,
    MissingTool = 6,
    InsideGitRepo = 7,
    UnknownProject = 8,
}

#[derive(Debug, thiserror::Error)]
pub enum OlfError {
    #[error(
        "not inside an olf workspace (no .olf/config.toml in {} or its parents)\n\
         hint: run `olf init --url <overleaf-url>` to create one",
        .0.display()
    )]
    NotAProject(PathBuf),

    #[error(
        "Overleaf rejected the git credentials\n{0}\n\
         hint: create a git token under Account Settings → Git Integration on overleaf.com, \
         then run `olf init --token <token>` (or set OVERLEAF_GIT_TOKEN)"
    )]
    AuthFailed(String),

    #[error("{0}")]
    BuildFailed(String),

    #[error("{0}")]
    UnknownProject(String),

    #[error("{tool} not found on PATH\nhint: {hint}")]
    MissingTool { tool: String, hint: String },

    #[error(
        "{} is inside the git repository at {}\n\
         hint: create the workspace outside it, e.g. `olf init {}`",
        target.display(), toplevel.display(), suggestion.display()
    )]
    InsideGitRepo {
        target: PathBuf,
        toplevel: PathBuf,
        suggestion: PathBuf,
    },

    #[error("{0}")]
    Error(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl OlfError {
    pub fn exit(&self) -> Exit {
        match self {
            Self::NotAProject(_) => Exit::NotAProject,
            Self::AuthFailed(_) => Exit::AuthFailed,
            Self::BuildFailed(_) => Exit::BuildFailed,
            Self::UnknownProject(_) => Exit::UnknownProject,
            Self::MissingTool { .. } => Exit::MissingTool,
            Self::InsideGitRepo { .. } => Exit::InsideGitRepo,
            Self::Error(_) | Self::Io(_) => Exit::Error,
        }
    }
}

/// Shorthand for a generic error with a formatted message.
macro_rules! bail {
    ($($arg:tt)*) => {
        return Err($crate::error::OlfError::Error(format!($($arg)*)))
    };
}
pub(crate) use bail;

pub type Result<T, E = OlfError> = std::result::Result<T, E>;
