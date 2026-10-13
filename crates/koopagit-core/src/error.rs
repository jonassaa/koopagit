use std::path::PathBuf;

/// Errors produced by the core library.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("'{0}' is not inside a git working tree")]
    NotAWorkingTree(PathBuf),
    #[error("failed to run git: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("git {args} failed (exit code {code:?}):\n{stderr}")]
    GitFailed {
        args: String,
        code: Option<i32>,
        stderr: String,
    },
    #[error("gix: {0}")]
    Gix(String),
    #[error("invalid command line: {0}")]
    InvalidCommandLine(String),
}

impl Error {
    pub(crate) fn gix(e: impl std::fmt::Display) -> Self {
        Error::Gix(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
