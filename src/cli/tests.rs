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
        r#"{"variables":{"name":{"__name__":"default"},"watch":{"__watch__":"true"}}}"#,
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
