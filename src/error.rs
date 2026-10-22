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
    /// The requested template does not exist in the group.
    TemplateNotFound {
        group: String,
        requested: String,
        available: Vec<String>,
    },
    /// The group has several templates and none is named like the group.
    DefaultTemplateAmbiguous {
        group: String,
        available: Vec<String>,
    },
    /// A variable claim cannot be used: reserved name, bad range, or escape.
    SchemaInvalid { template: String, reason: String },
    /// A declared variable shadows a native flag.
    ReservedVariable { template: String, variable: String },
    /// An `include` entry escapes the template or points at a missing path.
    IncludeInvalid {
        template: String,
        path: String,
        reason: String,
    },
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
            Self::TemplateNotFound {
                group,
                requested,
                available,
            } => write!(
                f,
                "template '{requested}' does not exist in group '{group}'; available: {}",
                fmt_list(available)
            ),
            Self::DefaultTemplateAmbiguous { group, available } => write!(
                f,
                "group '{group}' has several templates and none is named like the group; choose one with --template; available: {}",
                fmt_list(available)
            ),
            Self::SchemaInvalid { template, reason } => {
                write!(f, "invalid schema.json for template '{template}': {reason}")
            }
            Self::ReservedVariable { template, variable } => write!(
                f,
                "variable '{variable}' in template '{template}' shadows a native flag; rename it in schema.json"
            ),
            Self::IncludeInvalid {
                template,
                path,
                reason,
            } => write!(f, "include '{path}' in template '{template}' {reason}"),
            Self::Internal { reason } => write!(f, "internal error: {reason}"),
        }
    }
}

/// Render non-empty names as `a, b, c`, or `(none)`.
#[must_use]
pub fn fmt_list(items: &[String]) -> String {
    if items.is_empty() {
        "(none)".to_owned()
    } else {
        items.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_list_renders_names_or_placeholder() {
        assert_eq!(fmt_list(&[]), "(none)");
        assert_eq!(fmt_list(&["a".to_owned(), "b".to_owned()]), "a, b");
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
