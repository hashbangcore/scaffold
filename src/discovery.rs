//! Locate the template repository and enumerate its groups and templates.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Resolve the repository root from the `SCAFFOLD` environment value. No fallback exists.
pub fn source_root(value: Option<&OsStr>) -> Result<PathBuf> {
    let blank = match value {
        None => true,
        Some(raw) => raw.as_encoded_bytes().is_empty() || raw.to_string_lossy().trim().is_empty(),
    };
    match (value, blank) {
        (_, true) => Err(Error::MissingScaffold),
        (Some(raw), false) => Ok(PathBuf::from(raw)),
        (None, _) => Err(Error::MissingScaffold),
    }
}

/// Immediate subdirectories of `dir`, sorted by name; files and symlinks are skipped.
pub fn subdirectories(dir: &Path, context: &str) -> Result<Vec<(String, PathBuf)>> {
    let mut found = Vec::new();
    let read = std::fs::read_dir(dir).map_err(|source| Error::io(context, source))?;
    for entry in read {
        let entry = entry.map_err(|source| Error::io(context, source))?;
        let file_type = entry
            .file_type()
            .map_err(|source| Error::io(context, source))?;
        // Symlinks are skipped: v1 materializes plain directories and files only.
        if file_type.is_symlink() || !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        found.push((name, entry.path()));
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(found)
}

/// Group names (one per language) directly under the repository root.
pub fn groups(root: &Path) -> Result<Vec<String>> {
    let context = format!("read template repository `{}`", root.display());
    Ok(subdirectories(root, &context)?
        .into_iter()
        .map(|(name, _)| name)
        .collect())
}

/// Template names inside one group directory.
pub fn templates(root: &Path, group: &str) -> Result<Vec<String>> {
    let group_dir = root.join(group);
    if !group_dir.is_dir() {
        return Err(Error::GroupNotFound {
            group: group.to_owned(),
        });
    }
    let context = format!("read template group `{group}`");
    subdirectories(&group_dir, &context)
        .map(|found| found.into_iter().map(|(name, _)| name).collect())
}

/// A resolved template: its group, its directory, and how it was named.
#[derive(Debug)]
pub struct Selected {
    pub group: String,
    pub template: String,
    pub dir: PathBuf,
}

/// Resolve the template to use, preferring `explicit` over the default rule:
/// the only subdirectory, or the subdirectory named like the group.
pub fn select_template(root: &Path, group: &str, explicit: Option<&str>) -> Result<Selected> {
    let available = templates(root, group)?;
    let name = match explicit {
        Some(requested) => {
            if !available.iter().any(|candidate| candidate == requested) {
                return Err(Error::TemplateNotFound {
                    group: group.to_owned(),
                    requested: requested.to_owned(),
                    available,
                });
            }
            requested.to_owned()
        }
        None => default_template_name(group, &available)?,
    };
    Ok(Selected {
        group: group.to_owned(),
        template: name.clone(),
        dir: root.join(group).join(&name),
    })
}

fn default_template_name(group: &str, available: &[String]) -> Result<String> {
    if let [only] = available {
        return Ok(only.clone());
    }
    if let Some(same_name) = available.iter().find(|name| name == &group) {
        return Ok(same_name.clone());
    }
    Err(Error::DefaultTemplateAmbiguous {
        group: group.to_owned(),
        available: available.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::fs;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("scaffold-unit-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create fixture root");
        dir
    }

    #[test]
    fn subdirectories_lists_only_directories_sorted() {
        let root = temp_dir("scan");
        fs::create_dir_all(root.join("b_dir")).unwrap();
        fs::create_dir_all(root.join("a_dir")).unwrap();
        fs::write(root.join("loose.txt"), "x").unwrap();

        let found = subdirectories(&root, "scan test").unwrap();
        let names: Vec<&str> = found.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["a_dir", "b_dir"]);
    }

    #[test]
    fn subdirectories_skip_symlinks() {
        let root = temp_dir("scan-symlink");
        let target = temp_dir("scan-target");
        fs::create_dir_all(root.join("real")).unwrap();
        std::os::unix::fs::symlink(&target, root.join("linked")).unwrap();

        let found = subdirectories(&root, "scan test").unwrap();
        let names: Vec<&str> = found.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["real"]);
    }

    #[test]
    fn templates_reports_missing_group() {
        let root = temp_dir("templates-missing");
        match templates(&root, "ghost") {
            Err(Error::GroupNotFound { group }) => assert_eq!(group, "ghost"),
            other => panic!("expected GroupNotFound, got {other:?}"),
        }
    }

    #[test]
    fn source_root_accepts_a_non_empty_path() {
        let value = OsStr::new("./templates");
        assert!(source_root(Some(value)).is_ok());
    }

    #[test]
    fn source_root_rejects_missing_and_empty_values() {
        assert!(matches!(source_root(None), Err(Error::MissingScaffold)));
        assert!(matches!(
            source_root(Some(OsStr::new(""))),
            Err(Error::MissingScaffold)
        ));
        assert!(matches!(
            source_root(Some(OsStr::new("   "))),
            Err(Error::MissingScaffold)
        ));
    }

    #[test]
    fn default_rule_prefers_the_only_subdirectory() {
        let root = temp_dir("default-single");
        fs::create_dir_all(root.join("nix/unico_directorio")).unwrap();

        let selected = select_template(&root, "nix", None).unwrap();
        assert_eq!(selected.template, "unico_directorio");
        assert_eq!(selected.dir, root.join("nix").join("unico_directorio"));
    }

    #[test]
    fn default_rule_prefers_the_group_named_template() {
        let root = temp_dir("default-same-name");
        fs::create_dir_all(root.join("just/just")).unwrap();
        fs::create_dir_all(root.join("just/other")).unwrap();

        let selected = select_template(&root, "just", None).unwrap();
        assert_eq!(selected.template, "just");
    }

    #[test]
    fn default_rule_fails_when_ambiguous() {
        let root = temp_dir("default-ambiguous");
        fs::create_dir_all(root.join("rust/base")).unwrap();
        fs::create_dir_all(root.join("rust/min")).unwrap();

        match select_template(&root, "rust", None) {
            Err(Error::DefaultTemplateAmbiguous { group, available }) => {
                assert_eq!(group, "rust");
                assert_eq!(available, ["base", "min"]);
            }
            other => panic!("expected ambiguity error, got {other:?}"),
        }
    }

    #[test]
    fn explicit_selection_resolves_the_named_template() {
        let root = temp_dir("explicit");
        fs::create_dir_all(root.join("rust/min")).unwrap();
        fs::create_dir_all(root.join("rust/base")).unwrap();

        let selected = select_template(&root, "rust", Some("min")).unwrap();
        assert_eq!(selected.template, "min");
        assert_eq!(selected.group, "rust");
    }

    #[test]
    fn explicit_selection_fails_listing_availability() {
        let root = temp_dir("explicit-missing");
        fs::create_dir_all(root.join("rust/base")).unwrap();

        match select_template(&root, "rust", Some("ghost")) {
            Err(Error::TemplateNotFound {
                group,
                requested,
                available,
            }) => {
                assert_eq!(group, "rust");
                assert_eq!(requested, "ghost");
                assert_eq!(available, ["base"]);
            }
            other => panic!("expected TemplateNotFound, got {other:?}"),
        }
    }
}
