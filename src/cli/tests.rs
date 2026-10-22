//! Unit tests for the two-phase init parser and its helpers.

use std::ffi::OsString;
use std::path::PathBuf;

use super::*;

fn tokens(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

#[test]
fn scan_reads_the_plain_group() {
    let scan = scan_selection(&tokens(&["just", "./out/"]));
    assert_eq!(
        scan,
        Scan {
            group: Some("just".to_owned()),
            composite: None,
            flagged: None,
        }
    );
    assert_eq!(scan.explicit(), None);
}

#[test]
fn scan_splits_the_composed_form() {
    let scan = scan_selection(&tokens(&["rust/min", "./out"]));
    assert_eq!(scan.group.as_deref(), Some("rust"));
    assert_eq!(scan.composite.as_deref(), Some("min"));
    assert_eq!(scan.explicit().as_deref(), Some("min"));
}

#[test]
fn scan_flagged_template_wins_over_the_composed_one() {
    let scan = scan_selection(&tokens(&["rust/base", "-t", "min", "./out/"]));
    assert_eq!(scan.group.as_deref(), Some("rust"));
    assert_eq!(scan.composite.as_deref(), Some("base"));
    assert_eq!(scan.flagged.as_deref(), Some("min"));
    assert_eq!(scan.explicit().as_deref(), Some("min"));
}

#[test]
fn scan_flags_may_precede_the_selection() {
    let scan = scan_selection(&tokens(&["-t", "min", "rust"]));
    assert_eq!(scan.group.as_deref(), Some("rust"));
    assert_eq!(scan.flagged.as_deref(), Some("min"));
}

#[test]
fn scan_skips_value_taking_native_flags() {
    let scan = scan_selection(&tokens(&["-o", "./out/", "rust", "--name", "demo"]));
    assert_eq!(scan.group.as_deref(), Some("rust"));
}

#[test]
fn scan_reads_inline_template_values() {
    let scan = scan_selection(&tokens(&["--template=min", "rust"]));
    assert_eq!(scan.flagged.as_deref(), Some("min"));
    assert_eq!(scan.group.as_deref(), Some("rust"));
}

#[test]
fn scan_without_a_selection_stays_empty() {
    assert_eq!(scan_selection(&tokens(&["-o", "./out/"])), Scan::default());
}

#[test]
fn typo_suggestion_ranks_close_variables() {
    let variables = vec!["watch".to_owned(), "task".to_owned()];
    assert_eq!(
        typo_suggestion("wtch", &variables).as_deref(),
        Some("watch")
    );
    assert_eq!(
        typo_suggestion("taask", &variables).as_deref(),
        Some("task")
    );
    assert_eq!(typo_suggestion("zzzz", &variables), None);
}

#[test]
fn edit_distance_is_the_classic_metric() {
    assert_eq!(edit_distance("watch", "wtch"), 1);
    assert_eq!(edit_distance("same", "same"), 0);
    assert_eq!(edit_distance("abc", "xyz"), 3);
}

#[test]
fn init_options_maps_matches_to_materialization() {
    let schema = crate::schema::parse(
        r#"{"variables": {"name": {"__name__": "x"}, "watch": {"__watch__": "true"}}}"#,
        "options",
    )
    .expect("schema parses");
    let matches = init_root(&["watch".to_owned()])
        .try_get_matches_from(vec![
            "scaffold", "init", "just", "./out/", "--name", "demo", "--watch", "true", "--force",
        ])
        .expect("static parse for options");
    let (_, init) = matches.subcommand().expect("init matched");
    let options = init_options(init, Some(&schema));
    assert_eq!(options.dest.as_deref(), Some("./out/"));
    assert_eq!(options.name.as_deref(), Some("demo"));
    assert!(options.force);
    assert_eq!(
        options.overrides.get("watch").map(String::as_str),
        Some("true")
    );
    assert_eq!(
        options.overrides.get("name").map(String::as_str),
        Some("demo")
    );
}

#[test]
fn unknown_flags_reject_with_available_variables() {
    let selected = discovery::Selected {
        group: "rust".to_owned(),
        template: "min".to_owned(),
        dir: PathBuf::from("unused"),
    };
    let variables = vec!["watch".to_owned()];
    let argv = tokens(&["init", "--host", "x", "rust", "./out/"]);

    let error =
        reject_unknown_flags(&argv, &variables, &selected).expect_err("unknown flag rejected");
    match error {
        Error::UnknownFlag {
            flag,
            group,
            template,
            suggestion,
            ..
        } => {
            assert_eq!(flag, "--host");
            assert_eq!(group, "rust");
            assert_eq!(template, "min");
            assert_eq!(suggestion, None);
        }
        other => panic!("expected UnknownFlag, got {other:?}"),
    }

    // A known variable passes the gate.
    let okay = tokens(&["init", "--watch", "true", "rust", "./out/"]);
    reject_unknown_flags(&okay, &variables, &selected).expect("declared flag passes");
}

#[test]
fn unknown_flag_with_a_typo_hints_the_closest_variable() {
    let selected = discovery::Selected {
        group: "rust".to_owned(),
        template: "min".to_owned(),
        dir: PathBuf::from("unused"),
    };
    let variables = vec!["watch".to_owned()];
    let argv = tokens(&["init", "--wtch", "true", "rust", "./out/"]);

    match reject_unknown_flags(&argv, &variables, &selected) {
        Err(Error::UnknownFlag { suggestion, .. }) => {
            assert_eq!(suggestion.as_deref(), Some("watch"));
        }
        other => panic!("expected UnknownFlag with hint, got {other:?}"),
    }
}
