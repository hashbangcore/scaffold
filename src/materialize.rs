//! Materialize a template tree after checking destination conflicts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::discovery::Selected;
use crate::error::{Error, Result};
use crate::replace::{Replacements, substitute_bytes};
use crate::schema::{SCHEMA_FILE, Schema};

/// One file or directory waiting to be copied.
#[derive(Debug)]
pub struct PlanItem {
    /// Path relative to the template root.
    pub rel: PathBuf,
    /// Source path inside the template.
    pub abs: PathBuf,
    /// Whether the item is a directory.
    pub is_dir: bool,
}

/// Inputs gathered by the CLI for one `init` run.
#[derive(Debug, Default)]
pub struct InitOptions {
    /// Destination directory, or `.` when omitted.
    pub destination: PathBuf,
    /// The current directory may receive files without creating a new root.
    pub destination_is_implicit: bool,
    /// Per-variable flag values; unknown keys are ignored.
    pub overrides: BTreeMap<String, String>,
}

/// Plan the files one run writes, sorted for a deterministic order.
pub fn plan(template_dir: &Path) -> Result<Vec<PlanItem>> {
    let mut items = Vec::new();
    walk_dir(template_dir, PathBuf::new(), &mut items)?;
    items.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(items)
}

/// Collect files under `dir`, skipping symlinks and the root `schema.json`.
fn walk_dir(dir: &Path, rel_root: PathBuf, items: &mut Vec<PlanItem>) -> Result<()> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|source| Error::io(format!("read `{}`", dir.display()), source))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    entries.sort();
    for path in entries {
        let file_type = std::fs::symlink_metadata(&path)
            .map_err(|source| Error::io(format!("stat `{}`", path.display()), source))?;
        if file_type.is_symlink() {
            eprintln!("warning: skipping symlink `{}`", path.display());
            continue;
        }
        let name = path
            .file_name()
            .expect("read_dir names are present")
            .to_owned();
        let rel = rel_root.join(name);
        if rel == Path::new(SCHEMA_FILE) {
            // `schema.json` never ships, at any content level.
            continue;
        }
        if file_type.is_dir() {
            items.push(PlanItem {
                rel: rel.clone(),
                abs: path.clone(),
                is_dir: true,
            });
            walk_dir(&path, rel, items)?;
        } else {
            items.push(PlanItem {
                rel,
                abs: path,
                is_dir: false,
            });
        }
    }
    Ok(())
}

/// Check every write target before touching the disk; refuse atomically.
pub fn precheck(items: &[PlanItem], options: &InitOptions) -> Result<()> {
    if !options.destination_is_implicit && options.destination.exists() {
        return Err(Error::DestinationExists {
            path: options.destination.display().to_string(),
        });
    }
    let mut conflicts = Vec::new();
    if options.destination_is_implicit {
        for item in items {
            let target = options.destination.join(&item.rel);
            if target.exists() {
                conflicts.push(display_rel(&target, &options.destination));
            }
        }
    }
    if conflicts.is_empty() {
        Ok(())
    } else {
        Err(Error::Conflict { paths: conflicts })
    }
}

/// Render a target path relative to the destination root when possible.
fn display_rel(target: &Path, root: &Path) -> String {
    target.strip_prefix(root).map_or_else(
        |_| target.display().to_string(),
        |rel| rel.display().to_string(),
    )
}

/// Write the full template tree under the destination directory.
pub fn write(items: &[PlanItem], options: &InitOptions, replacements: &Replacements) -> Result<()> {
    if !options.destination_is_implicit {
        if let Some(parent) = options.destination.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|source| Error::io(format!("create `{}`", parent.display()), source))?;
        }
        std::fs::create_dir(&options.destination).map_err(|source| {
            Error::io(
                format!("create `{}`", options.destination.display()),
                source,
            )
        })?;
    }
    for item in items {
        write_one(item, &options.destination.join(&item.rel), replacements)?;
    }
    Ok(())
}

fn write_one(item: &PlanItem, target: &Path, replacements: &Replacements) -> Result<()> {
    if item.is_dir {
        return std::fs::create_dir_all(target)
            .map_err(|source| Error::io(format!("create `{}`", target.display()), source));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|source| Error::io(format!("create `{}`", parent.display()), source))?;
    }
    let raw = std::fs::read(&item.abs)
        .map_err(|source| Error::io(format!("read `{}`", item.abs.display()), source))?;
    let bytes = substitute_bytes(&raw, replacements);
    std::fs::write(target, bytes.as_ref())
        .map_err(|source| Error::io(format!("write `{}`", target.display()), source))
}

/// Run one init end to end: plan, resolve, precheck, write. Quiet on success.
pub fn run(selected: &Selected, schema: Option<&Schema>, options: &InitOptions) -> Result<()> {
    let items = plan(&selected.dir)?;
    precheck(&items, options)?;
    let replacements = schema.map_or_else(Replacements::default, |schema| {
        Replacements::bind(&schema.variables, &options.overrides)
    });
    write(&items, options, &replacements)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture(tag: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("scaffold-materialize-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create fixture");
        path
    }

    fn selected(root: &Path) -> Selected {
        Selected {
            group: "just".to_owned(),
            template: "publish".to_owned(),
            dir: root.join("scaffold"),
        }
    }

    #[test]
    fn copies_all_files_except_schema_and_substitutes_variables() {
        let root = fixture("copy");
        let source = root.join("scaffold");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(
            source.join("schema.json"),
            r#"{"variables":{"name":{"__name__":"default"}}}"#,
        )
        .unwrap();
        fs::write(source.join("nested/file.txt"), "__name__").unwrap();
        fs::create_dir(source.join("empty")).unwrap();
        fs::write(source.join("other.txt"), "all content").unwrap();
        let destination = root.join("new-project");
        let schema = crate::schema::load(&source, "just/publish").unwrap();
        let options = InitOptions {
            destination: destination.clone(),
            destination_is_implicit: false,
            overrides: BTreeMap::from([("name".to_owned(), "demo".to_owned())]),
        };

        run(&selected(&root), schema.as_ref(), &options).expect("materialize template");

        assert_eq!(
            fs::read_to_string(destination.join("nested/file.txt")).unwrap(),
            "demo"
        );
        assert_eq!(
            fs::read_to_string(destination.join("other.txt")).unwrap(),
            "all content"
        );
        assert!(!destination.join("schema.json").exists());
        assert!(destination.join("empty").is_dir());
    }

    #[test]
    fn explicit_existing_destination_is_rejected_without_changes() {
        let root = fixture("existing");
        let source = root.join("scaffold");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("file"), "new").unwrap();
        let destination = fixture("existing-destination");
        let options = InitOptions {
            destination: destination.clone(),
            destination_is_implicit: false,
            ..InitOptions::default()
        };

        let error = run(&selected(&root), None, &options).expect_err("existing destination fails");

        assert!(matches!(error, Error::DestinationExists { .. }));
        assert!(fs::read_dir(destination).unwrap().next().is_none());
    }

    #[test]
    fn implicit_destination_rejects_colliding_files_before_copying() {
        let root = fixture("collision");
        let source = root.join("scaffold");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("same"), "new").unwrap();
        let destination = fixture("collision-destination");
        fs::write(destination.join("same"), "old").unwrap();
        let options = InitOptions {
            destination: destination.clone(),
            destination_is_implicit: true,
            ..InitOptions::default()
        };

        let error = run(&selected(&root), None, &options).expect_err("collision fails");

        assert!(matches!(error, Error::Conflict { .. }));
        assert_eq!(fs::read_to_string(destination.join("same")).unwrap(), "old");
    }
}
