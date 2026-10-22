//! Materialize a template: content selection, destination semantics, atomic precheck, copy.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::discovery::Selected;
use crate::error::{Error, Result};
use crate::replace::{Replacements, substitute_bytes};
use crate::schema::{Content, SCHEMA_FILE, Schema, content};

/// One file waiting to be written: relative layout plus its source path.
#[derive(Debug)]
pub struct PlanItem {
    /// Path relative to the template root; preserved under the destination.
    pub rel: PathBuf,
    /// Absolute source path inside the template.
    pub abs: PathBuf,
}

/// Inputs gathered by the CLI for one `init` run.
#[derive(Debug, Default)]
pub struct InitOptions {
    /// `--name`: destination fallback and the `name` variable override.
    pub name: Option<String>,
    /// Trailing positional destination.
    pub dest: Option<String>,
    /// `-o/--output` destination; wins over the positional.
    pub output: Option<String>,
    /// Overwrite existing targets.
    pub force: bool,
    /// Per-variable flag values; unknown keys are ignored.
    pub overrides: BTreeMap<String, String>,
}

/// Where materialization lands.
#[derive(Debug, PartialEq)]
pub enum Destination {
    /// A directory receiving the template's relative hierarchy.
    Project(PathBuf),
    /// One exact file path; requires a single exported file.
    File(PathBuf),
}

/// Plan the files one run writes, sorted for a deterministic order.
pub fn plan(
    template_dir: &Path,
    selection: &Content,
    template_label: &str,
) -> Result<Vec<PlanItem>> {
    let mut items = Vec::new();
    match selection {
        Content::All | Content::AllButSchema => {
            walk_dir(template_dir, PathBuf::new(), &mut items)?;
        }
        Content::Whitelist(paths) => {
            for entry in paths {
                let as_dir = entry.ends_with('/');
                let rel = PathBuf::from(entry.trim_end_matches('/'));
                let full = template_dir.join(&rel);
                if full.is_dir() {
                    walk_dir(&full, rel, &mut items)?;
                } else if full.is_file() {
                    if as_dir {
                        return Err(Error::IncludeInvalid {
                            template: template_label.to_owned(),
                            path: entry.clone(),
                            reason: "ends with '/' but is a file".to_owned(),
                        });
                    }
                    items.push(PlanItem { rel, abs: full });
                } else {
                    return Err(Error::IncludeInvalid {
                        template: template_label.to_owned(),
                        path: entry.clone(),
                        reason: "does not exist in the template".to_owned(),
                    });
                }
            }
        }
    }
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
            walk_dir(&path, rel, items)?;
        } else {
            items.push(PlanItem { rel, abs: path });
        }
    }
    Ok(())
}

/// Decide the destination: `-o` wins, then the positional, then `--name`.
///
/// The auto destination is always a project: `./<name>/`. An explicit path is a
/// project when it ends with `/` or exists as a directory, and a file otherwise.
pub fn resolve_destination(
    dest: Option<&str>,
    output: Option<&str>,
    name: Option<&str>,
) -> Result<Destination> {
    let explicit = output.or(dest);
    let Some(path_str) = explicit else {
        return match name {
            Some(name) if !name.trim().is_empty() => {
                Ok(Destination::Project(PathBuf::from(format!("./{name}"))))
            }
            _ => Err(Error::DestinationUnresolved),
        };
    };
    let path = PathBuf::from(path_str);
    let is_dir = path.is_dir() || path_str.ends_with('/');
    Ok(if is_dir {
        Destination::Project(path)
    } else {
        Destination::File(path)
    })
}

/// Check every write target before touching the disk; refuse atomically.
pub fn precheck(items: &[PlanItem], destination: &Destination, force: bool) -> Result<()> {
    let mut conflicts = Vec::new();
    match destination {
        Destination::File(target) => {
            if target.exists() && !force {
                conflicts.push(display_rel(target, target));
            }
        }
        Destination::Project(root) => {
            if root.exists() && !root.is_dir() {
                conflicts.push(display_rel(root, root));
            }
            for item in items {
                let target = root.join(&item.rel);
                if target.exists() && !force {
                    conflicts.push(display_rel(&target, root));
                }
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

/// Write everything: one file exactly, or a full tree with relative layout.
pub fn write(
    items: &[PlanItem],
    destination: &Destination,
    replacements: &Replacements,
) -> Result<()> {
    match destination {
        Destination::File(target) => {
            let [item] = items else {
                unreachable!("file mode is gated to a single item by the caller");
            };
            write_one(item, target, replacements)?;
        }
        Destination::Project(root) => {
            std::fs::create_dir_all(root)
                .map_err(|source| Error::io(format!("create `{}`", root.display()), source))?;
            for item in items {
                write_one(item, &root.join(&item.rel), replacements)?;
            }
        }
    }
    Ok(())
}

fn write_one(item: &PlanItem, target: &Path, replacements: &Replacements) -> Result<()> {
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
    let label = format!("{}/{}", selected.group, selected.template);
    let selection = content(schema);
    let items = plan(&selected.dir, &selection, &label)?;
    let destination = resolve_destination(
        options.dest.as_deref(),
        options.output.as_deref(),
        options.name.as_deref(),
    )?;
    if matches!(destination, Destination::File(_)) && items.len() != 1 {
        return Err(Error::FileModeMultipleFiles {
            template: label,
            exported: items.len(),
        });
    }
    precheck(&items, &destination, options.force)?;
    let replacements = schema.map_or_else(Replacements::default, |schema| {
        Replacements::bind(&schema.variables, &options.overrides)
    });
    write(&items, &destination, &replacements)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::parse;
    use std::fs;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new(tag: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("scaffold-mat-{tag}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).expect("create fixture");
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write(&self, rel: &str, content: &str) {
            let file = self.0.join(rel);
            fs::create_dir_all(file.parent().expect("fixture parent")).expect("fixture dirs");
            fs::write(file, content).expect("fixture file");
        }

        fn write_bytes(&self, rel: &str, content: &[u8]) {
            let file = self.0.join(rel);
            fs::create_dir_all(file.parent().expect("fixture parent")).expect("fixture dirs");
            fs::write(file, content).expect("fixture binary file");
        }

        fn selected(&self, group: &str, template: &str) -> Selected {
            Selected {
                group: group.to_owned(),
                template: template.to_owned(),
                dir: self.0.join(group).join(template),
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    // 6.1 — content selection per level.

    #[test]
    fn plan_copies_whole_tree_verbatim() {
        let template = Fixture::new("all");
        template.write("t/Cargo.toml", "x");
        template.write("t/src/main.rs", "y");
        template.write("t/.hidden", "z");
        let content = content(None);

        let items = plan(&template.path().join("t"), &content, "t").unwrap();
        let rels: Vec<String> = items.iter().map(|i| i.rel.display().to_string()).collect();
        assert_eq!(rels, [".hidden", "Cargo.toml", "src/main.rs"]);
    }

    #[test]
    fn plan_writes_everything_but_schema_json() {
        let schema = Fixture::new("allbut");
        schema.write("t/Cargo.toml", "x");
        schema.write("t/schema.json", "{}");
        let parsed = parse(r#"{"variables": {}}"#, "t").unwrap();
        let content = content(Some(&parsed));

        let items = plan(&schema.path().join("t"), &content, "t").unwrap();
        let rels: Vec<String> = items.iter().map(|i| i.rel.display().to_string()).collect();
        assert_eq!(rels, ["Cargo.toml"]);
    }

    #[test]
    fn plan_whitelist_recurses_directories_and_keeps_order() {
        let template = Fixture::new("whitelist");
        template.write("rust/base/Cargo.toml", "x");
        template.write("rust/base/schema.json", "{}");
        template.write("rust/base/src/lib.rs", "y");
        template.write("rust/base/src/deep/mod.rs", "z");
        template.write("rust/base/README.md", "r");
        let parsed = parse(r#"{"include": ["Cargo.toml", "src/"]}"#, "rust/base").unwrap();
        let content = content(Some(&parsed));

        let items = plan(&template.path().join("rust/base"), &content, "rust/base").unwrap();
        let rels: Vec<String> = items.iter().map(|i| i.rel.display().to_string()).collect();
        assert_eq!(rels, ["Cargo.toml", "src/deep/mod.rs", "src/lib.rs"]);
    }

    #[test]
    fn plan_whitelist_reports_missing_and_mismatched_entries() {
        let template = Fixture::new("whitelist-bad");
        template.write("t/file.txt", "x");

        let parsed = parse(r#"{"include": ["ghost.txt"]}"#, "t").unwrap();
        match plan(&template.path().join("t"), &content(Some(&parsed)), "t") {
            Err(Error::IncludeInvalid { path, .. }) => assert_eq!(path, "ghost.txt"),
            other => panic!("expected IncludeInvalid, got {other:?}"),
        }

        let parsed = parse(r#"{"include": ["file.txt/"]}"#, "t").unwrap();
        assert!(plan(&template.path().join("t"), &content(Some(&parsed)), "t").is_err());
    }

    // 6.2 — destination resolution.

    #[test]
    fn output_flag_wins_over_the_positional() {
        let resolved =
            resolve_destination(Some("./pos/"), Some("./out.nix"), Some("demo")).unwrap();
        assert_eq!(resolved, Destination::File(PathBuf::from("./out.nix")));
    }

    #[test]
    fn name_alone_yields_a_project_destination() {
        let resolved = resolve_destination(None, None, Some("demo")).unwrap();
        assert_eq!(resolved, Destination::Project(PathBuf::from("./demo")));
    }

    #[test]
    fn missing_destination_and_name_fails() {
        assert!(matches!(
            resolve_destination(None, None, None),
            Err(Error::DestinationUnresolved)
        ));
    }

    #[test]
    fn explicit_project_forms() {
        assert_eq!(
            resolve_destination(Some("./fresh/"), None, None).unwrap(),
            Destination::Project(PathBuf::from("./fresh"))
        );
        let exists = Fixture::new("exists-dir");
        let already = exists.path().join("already");
        fs::create_dir_all(&already).unwrap();
        assert_eq!(
            resolve_destination(Some(already.to_string_lossy().as_ref()), None, None).unwrap(),
            Destination::Project(already.clone())
        );
    }

    #[test]
    fn plain_non_existing_and_existing_file_destinations_are_files() {
        let resolved = resolve_destination(Some("out.nix"), None, None).unwrap();
        assert_eq!(resolved, Destination::File(PathBuf::from("out.nix")));

        let exists = Fixture::new("exists-file");
        let there = exists.path().join("there.nix");
        fs::write(&there, "old").unwrap();
        assert_eq!(
            resolve_destination(Some(there.to_string_lossy().as_ref()), None, None).unwrap(),
            Destination::File(there.clone())
        );
    }

    // 6.3 — file mode gate.

    #[test]
    fn file_mode_refuses_multi_file_export() {
        let template = Fixture::new("filemode-multi");
        template.write("r/t/a", "1");
        template.write("r/t/b", "2");
        let selected = template.selected("r", "t");

        match run(
            &selected,
            None,
            &InitOptions {
                dest: Some("single.out".to_owned()),
                ..InitOptions::default()
            },
        ) {
            Err(Error::FileModeMultipleFiles { template, exported }) => {
                assert_eq!(template, "r/t");
                assert_eq!(exported, 2);
            }
            other => panic!("expected FileModeMultipleFiles, got {other:?}"),
        }
    }

    // 6.4 — atomic refusal and force.

    #[test]
    fn conflict_refuses_without_writing_anything() {
        let template = Fixture::new("conflict");
        template.write("r/t/a", "new");
        template.write("r/t/b", "new");
        let selected = template.selected("r", "t");
        let dest = Fixture::new("conflict-dest");
        fs::write(dest.path().join("a"), "old").unwrap();

        let result = run(
            &selected,
            None,
            &InitOptions {
                dest: Some(dest.path().to_string_lossy().into_owned()),
                ..InitOptions::default()
            },
        );
        assert!(matches!(result, Err(Error::Conflict { .. })));
        assert_eq!(fs::read_to_string(dest.path().join("a")).unwrap(), "old");
        assert!(!dest.path().join("b").exists(), "nothing was written");
    }

    #[test]
    fn force_overwrites_existing_targets() {
        let template = Fixture::new("force");
        template.write("r/t/a", "new");
        let selected = template.selected("r", "t");
        let dest = Fixture::new("force-dest");
        fs::write(dest.path().join("a"), "old").unwrap();

        run(
            &selected,
            None,
            &InitOptions {
                dest: Some(dest.path().to_string_lossy().into_owned()),
                force: true,
                ..InitOptions::default()
            },
        )
        .expect("force write");
        assert_eq!(fs::read_to_string(dest.path().join("a")).unwrap(), "new");
    }

    // Composition: substitutions and binary passthrough through run().

    #[test]
    fn run_substitutes_and_never_ships_schema_json() {
        let template = Fixture::new("compose");
        template.write("r/t/Cargo.toml", "name = \"__name__\"\nwatch = __watch__\n");
        template.write("r/t/README.md", "stays");
        template.write(
            "r/t/schema.json",
            r#"{"include": ["Cargo.toml"], "variables": {"name": {"__name__": "default_name"}, "watch": {"__watch__": "true"}}}"#,
        );
        let schema = crate::schema::load(&template.path().join("r/t"), "r/t").unwrap();
        let selected = template.selected("r", "t");
        let dest = template.path().join("demo");

        run(
            &selected,
            schema.as_ref(),
            &InitOptions {
                // Trailing slash: this test targets a project directory.
                dest: Some(format!("{}/", dest.display())),
                overrides: BTreeMap::from([("watch".to_owned(), "false".to_owned())]),
                ..InitOptions::default()
            },
        )
        .expect("compose run");

        let cargo = fs::read_to_string(dest.join("Cargo.toml")).unwrap();
        assert!(
            cargo.contains("name = \"default_name\""),
            "default bound: {cargo}"
        );
        assert!(cargo.contains("watch = false"), "override bound: {cargo}");
        assert!(!dest.join("schema.json").exists(), "schema never ships");
        assert!(!dest.join("README.md").exists(), "whitelist held");
    }

    #[test]
    fn run_passes_binary_files_through_verbatim() {
        let template = Fixture::new("binary");
        template.write_bytes("r/t/logo.bin", &[0xFF, 0xFE, 0x00, b'a']);
        let selected = template.selected("r", "t");
        let dest = Fixture::new("binary-dest");
        let target = dest.path().join("copy.bin");

        run(
            &selected,
            None,
            &InitOptions {
                dest: Some(target.to_string_lossy().into_owned()),
                ..InitOptions::default()
            },
        )
        .expect("binary run");
        let copied = fs::read(&target).unwrap();
        assert_eq!(copied, [0xFF, 0xFE, 0x00, b'a']);
    }
}
