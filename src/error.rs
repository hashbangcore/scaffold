//! Shared error type: plain messages, no external dependencies.

use std::error::Error as StdError;
use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

/// Errors surfaced by the CLI. `main` prints them with an `error:` prefix.
#[derive(Debug)]
pub enum Error {
    /// `SCAFFOLD` is unset, empty, or only whitespace; no fallback path exists.
    MissingScaffold,
    /// A filesystem interaction failed; `context` names the operation.
    Io {
        context: String,
        source: std::io::Error,
    },
    /// The group directory does not exist under the repository root.
    GroupNotFound { group: String },
    /// A code path that clap should have prevented was reached.
    Internal { reason: String },
}

impl Error {
    /// Wrap an I/O failure with the operation description.
    #[must_use]
    pub fn io(context: impl fmt::Display, source: std::io::Error) -> Self {
        Self::Io {
            context: context.to_string(),
            source,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingScaffold => write!(
                f,
                "environment variable SCAFFOLD must point to the template repository (unset or empty)"
            ),
            Self::Io { context, source } => write!(f, "{context}: {source}"),
            Self::GroupNotFound { group } => {
                write!(
                    f,
                    "group '{group}' does not exist under the template repository"
                )
            }
            Self::Internal { reason } => write!(f, "internal error: {reason}"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
