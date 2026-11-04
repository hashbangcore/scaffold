//! Unit tests for init argument parsing.

use std::ffi::OsString;
use std::path::PathBuf;

use super::*;

#[test]
fn init_accepts_selection_destination_and_schema_variables() {
    let variables = vec!["name".to_owned(), "watch".to_owned()];
    let matches = init_root(&variables)
        .try_get_matches_from([
            "scaffold",
            "init",
            "just/publish",
            "demo",
            "--name",
            "chosen",
            "--watch=false",
        ])
        .expect("declared variable flags parse");
    let (_, init) = matches.subcommand().expect("init matched");
    let schema = crate::schema::parse(
        r#"{"variables":{"name":"default","watch":"true"}}"#,
        "just/publish",
    )
    .expect("schema parses");
    let options = init_options(init, Some(&schema));

    assert_eq!(options.destination, PathBuf::from("demo"));
    assert!(!options.destination_is_implicit);
    assert_eq!(options.overrides["name"], "chosen");
    assert_eq!(options.overrides["watch"], "false");
}

#[test]
fn init_defaults_destination_to_current_directory() {
    let matches = init_root(&[])
        .try_get_matches_from(["scaffold", "init", "just/publish"])
        .expect("selection without destination parses");
    let (_, init) = matches.subcommand().expect("init matched");
    let options = init_options(init, None);

    assert_eq!(options.destination, PathBuf::from("."));
    assert!(options.destination_is_implicit);
}

#[test]
fn dot_destination_targets_the_current_directory() {
    let matches = init_root(&[])
        .try_get_matches_from(["scaffold", "init", "just/publish", "./"])
        .expect("dot destination parses");
    let (_, init) = matches.subcommand().expect("init matched");
    let options = init_options(init, None);

    assert_eq!(options.destination, PathBuf::from("./"));
    assert!(options.destination_is_implicit);
}

#[test]
fn undeclared_flags_are_rejected_with_variable_hints() {
    let selected = discovery::Selected {
        group: "just".to_owned(),
        template: "publish".to_owned(),
        dir: PathBuf::from("unused"),
    };
    let variables = vec!["watch".to_owned()];
    let args: Vec<OsString> = ["--wtch", "true"].map(OsString::from).into();

    match reject_unknown_flags(&args, &variables, &selected) {
        Err(Error::UnknownFlag { suggestion, .. }) => {
            assert_eq!(suggestion.as_deref(), Some("watch"));
        }
        other => panic!("expected UnknownFlag, got {other:?}"),
    }
}

#[test]
fn edit_distance_is_the_classic_metric() {
    assert_eq!(edit_distance("watch", "wtch"), 1);
    assert_eq!(edit_distance("same", "same"), 0);
    assert_eq!(edit_distance("abc", "xyz"), 3);
}

#[test]
fn scaffold_completion_returns_matching_group_template_paths() {
    let root = std::env::temp_dir().join(format!("scaffold-completion-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("rust/min")).expect("create rust template");
    std::fs::create_dir_all(root.join("just/publish")).expect("create just template");
    let groups = discovery::groups(&root).expect("read template groups");

    let candidates = complete_scaffold_candidates(&root, "ru", groups);

    assert_eq!(candidates, ["rust/min"]);
    std::fs::remove_dir_all(root).expect("remove completion fixture");
}

#[test]
fn group_completion_returns_only_matching_group_names() {
    let groups = ["css", "envrc", "flake", "just", "license"].map(str::to_owned);

    assert_eq!(complete_group_candidates(&groups, ""), groups);
    assert_eq!(complete_group_candidates(&groups, "fl"), ["flake"]);
}
