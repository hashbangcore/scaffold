//! Template schema: variables and path selection rules from `schema.json`.

use crate::error::{Error, Result};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use std::collections::BTreeMap;
use std::path::Path;

/// Name of the schema file inside a template directory.
pub const SCHEMA_FILE: &str = "schema.json";

/// Native flag names a schema variable must not shadow.
const RESERVED_NAMES: &[&str] = &["help"];

/// Variables declared by a template: option name → default value.
pub type Variables = BTreeMap<String, String>;

/// Text surrounding a variable name in files and command arguments.
#[derive(Debug, PartialEq, Eq)]
pub struct Delimiters {
    pub open: String,
    pub close: String,
}

impl Default for Delimiters {
    fn default() -> Self {
        Self {
            open: "{{".to_owned(),
            close: "}}".to_owned(),
        }
    }
}

/// Parsed and validated `schema.json`.
#[derive(Debug)]
pub struct Schema {
    /// Tokens replaceable in copied files, keyed by variable name.
    pub variables: Variables,
    /// File selection rules applied while planning the copy.
    pub(crate) files: FileSelection,
    /// Delimiters used to identify variables in text.
    pub delimiters: Delimiters,
    /// Argument lists executed in order after the scaffold is copied.
    pub commands: Vec<Vec<String>>,
}

#[derive(Debug)]
pub(crate) struct FileSelection {
    include: Option<GlobSet>,
    exclude: Option<GlobSet>,
}

impl FileSelection {
    pub(crate) fn has_include(&self) -> bool {
        self.include.is_some()
    }

    pub(crate) fn includes(&self, path: &Path) -> bool {
        self.include
            .as_ref()
            .is_some_and(|patterns| matches_path_or_parent(patterns, path))
    }

    pub(crate) fn excludes(&self, path: &Path) -> bool {
        self.exclude
            .as_ref()
            .is_some_and(|patterns| matches_path_or_parent(patterns, path))
    }
}

/// Load and validate `schema.json` from the template directory; absent is valid.
pub fn load(template_dir: &Path, template_label: &str) -> Result<Option<Schema>> {
    let path = template_dir.join(SCHEMA_FILE);
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(Error::io(
                format!("read schema of {template_label}"),
                source,
            ));
        }
    };
    Ok(Some(parse(&raw, template_label)?))
}

/// Parse raw JSON into a `Schema`, rejecting reserved variable names.
pub fn parse(raw: &str, template_label: &str) -> Result<Schema> {
    let invalid = |reason: String| Error::SchemaInvalid {
        template: template_label.to_owned(),
        reason,
    };
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|source| invalid(source.to_string()))?;
    let object = match value {
        serde_json::Value::Object(map) => map,
        other => {
            return Err(invalid(format!(
                "expected an object, found {}",
                describe(other)
            )));
        }
    };
    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "settings" | "variables" | "include" | "exclude" | "commands"
        ) {
            return Err(invalid(format!("unknown key '{key}'")));
        }
    }
    let include_patterns = parse_path_patterns(object.get("include"), "include", &invalid)?;
    let exclude_patterns = parse_path_patterns(object.get("exclude"), "exclude", &invalid)?;
    let include = compile_path_patterns(&include_patterns, "include", &invalid)?;
    let exclude = compile_path_patterns(&exclude_patterns, "exclude", &invalid)?;
    let variables = parse_variables(
        object
            .get("variables")
            .map_or(&serde_json::Value::Null, |v| v),
        &invalid,
    )?;
    let delimiters = parse_delimiters(object.get("settings"), &invalid)?;
    let commands = parse_commands(object.get("commands"), &invalid)?;
    let schema = Schema {
        variables,
        files: FileSelection { include, exclude },
        delimiters,
        commands,
    };
    validate(&schema, template_label)?;
    Ok(schema)
}

fn parse_delimiters(
    value: Option<&serde_json::Value>,
    invalid: &(dyn Fn(String) -> Error + '_),
) -> Result<Delimiters> {
    let Some(value) = value else {
        return Ok(Delimiters::default());
    };
    let settings = value
        .as_object()
        .ok_or_else(|| invalid("'settings' must be an object".to_owned()))?;
    for key in settings.keys() {
        if key != "delimiters" {
            return Err(invalid(format!("unknown settings key '{key}'")));
        }
    }
    let Some(value) = settings.get("delimiters") else {
        return Ok(Delimiters::default());
    };
    let delimiters = value
        .as_object()
        .ok_or_else(|| invalid("'settings.delimiters' must be an object".to_owned()))?;
    for key in delimiters.keys() {
        if !matches!(key.as_str(), "open" | "close") {
            return Err(invalid(format!("unknown delimiter key '{key}'")));
        }
    }
    let parse_delimiter = |key: &str| -> Result<String> {
        let Some(value) = delimiters.get(key) else {
            return Err(invalid(format!("'settings.delimiters.{key}' is required")));
        };
        let Some(value) = value.as_str() else {
            return Err(invalid(format!(
                "'settings.delimiters.{key}' must be a string"
            )));
        };
        if value.is_empty() {
            return Err(invalid(format!(
                "'settings.delimiters.{key}' must not be empty"
            )));
        }
        Ok(value.to_owned())
    };
    let open = parse_delimiter("open")?;
    let close = parse_delimiter("close")?;
    Ok(Delimiters { open, close })
}

fn parse_commands(
    value: Option<&serde_json::Value>,
    invalid: &(dyn Fn(String) -> Error + '_),
) -> Result<Vec<Vec<String>>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let commands = value
        .as_array()
        .ok_or_else(|| invalid("'commands' must be an array of argument arrays".to_owned()))?;
    commands
        .iter()
        .enumerate()
        .map(|(command_index, command)| {
            let arguments = command.as_array().ok_or_else(|| {
                invalid(format!(
                    "'commands[{command_index}]' must be an argument array"
                ))
            })?;
            if arguments.is_empty() {
                return Err(invalid(format!(
                    "'commands[{command_index}]' must include a program"
                )));
            }
            arguments
                .iter()
                .enumerate()
                .map(|(argument_index, argument)| {
                    let argument = argument.as_str().ok_or_else(|| {
                        invalid(format!(
                            "'commands[{command_index}][{argument_index}]' must be a string"
                        ))
                    })?;
                    if argument_index == 0 && argument.is_empty() {
                        return Err(invalid(format!(
                            "'commands[{command_index}]' must include a program"
                        )));
                    }
                    Ok(argument.to_owned())
                })
                .collect()
        })
        .collect()
}

fn parse_path_patterns(
    value: Option<&serde_json::Value>,
    key: &str,
    invalid: &(dyn Fn(String) -> Error + '_),
) -> Result<Vec<String>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let patterns = value
        .as_array()
        .ok_or_else(|| invalid(format!("'{key}' must be an array of path patterns")))?;
    patterns
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let pattern = value.as_str().ok_or_else(|| {
                invalid(format!("'{key}[{index}]' must be a string path pattern"))
            })?;
            validate_path_pattern(pattern, key, index, invalid)?;
            Ok(pattern.to_owned())
        })
        .collect()
}

fn validate_path_pattern(
    pattern: &str,
    key: &str,
    index: usize,
    invalid: &(dyn Fn(String) -> Error + '_),
) -> Result<()> {
    let drive_prefix = pattern.as_bytes().get(1) == Some(&b':')
        && pattern
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic);
    if pattern.is_empty()
        || pattern.starts_with('/')
        || pattern.starts_with('\\')
        || pattern.contains('\\')
        || drive_prefix
        || pattern
            .split('/')
            .any(|part| matches!(part, "" | "." | ".."))
    {
        return Err(invalid(format!(
            "'{key}[{index}]' must be a relative path pattern without '.' or '..' components"
        )));
    }
    Ok(())
}

fn compile_path_patterns(
    patterns: &[String],
    key: &str,
    invalid: &(dyn Fn(String) -> Error + '_),
) -> Result<Option<GlobSet>> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for (index, pattern) in patterns.iter().enumerate() {
        let glob = GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .map_err(|error| invalid(format!("invalid '{key}[{index}]' pattern: {error}")))?;
        builder.add(glob);
        if let Some(directory) = pattern.strip_suffix("/**")
            && !directory.is_empty()
        {
            let glob = GlobBuilder::new(directory)
                .literal_separator(true)
                .build()
                .map_err(|error| invalid(format!("invalid '{key}[{index}]' pattern: {error}")))?;
            builder.add(glob);
        }
    }
    builder
        .build()
        .map(Some)
        .map_err(|error| invalid(format!("invalid '{key}' patterns: {error}")))
}

fn matches_path_or_parent(patterns: &GlobSet, path: &Path) -> bool {
    path.ancestors()
        .take_while(|ancestor| !ancestor.as_os_str().is_empty())
        .any(|ancestor| patterns.is_match(ancestor))
}

fn describe(value: serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_owned(),
        serde_json::Value::Array(_) => "an array".to_owned(),
        serde_json::Value::String(_) => "a string".to_owned(),
        serde_json::Value::Number(_) => "a number".to_owned(),
        serde_json::Value::Bool(_) => "a boolean".to_owned(),
        serde_json::Value::Object(_) => "an object".to_owned(),
    }
}

fn parse_variables(
    value: &serde_json::Value,
    invalid: &(dyn Fn(String) -> Error + '_),
) -> Result<Variables> {
    match value {
        serde_json::Value::Null => Ok(Variables::new()),
        serde_json::Value::Object(knobs) => {
            let mut variables = Variables::new();
            for (name, default) in knobs {
                if name.is_empty() {
                    return Err(invalid("variable name must not be empty".to_owned()));
                }
                let default = default.as_str().ok_or_else(|| {
                    invalid(format!("default of variable '{name}' must be a string"))
                })?;
                variables.insert(name.clone(), default.to_owned());
            }
            Ok(variables)
        }
        other => Err(invalid(format!(
            "'variables' must map names to string defaults, found {}",
            describe(other.clone())
        ))),
    }
}

/// Check reserved variable names and duplicated tokens.
fn validate(schema: &Schema, template_label: &str) -> Result<()> {
    for name in schema.variables.keys() {
        if RESERVED_NAMES.contains(&name.as_str()) {
            return Err(Error::ReservedVariable {
                template: template_label.to_owned(),
                variable: name.clone(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_reads_variable_defaults() {
        let schema = parse(
            r#"{"variables":{"name":"demo","watch":"true"}}"#,
            "just/publish",
        )
        .expect("schema parses");

        assert_eq!(schema.variables["name"], "demo");
        assert_eq!(schema.variables["watch"], "true");
        assert_eq!(schema.delimiters, Delimiters::default());
    }

    #[test]
    fn parse_accepts_optional_custom_delimiters() {
        for raw in [r#"{"variables":{}}"#, r#"{"settings":{},"variables":{}}"#] {
            assert_eq!(
                parse(raw, "template").unwrap().delimiters,
                Delimiters::default()
            );
        }
        let schema = parse(
            r##"{"settings":{"delimiters":{"open":"#{","close":"}"}},"variables":{}}"##,
            "template",
        )
        .expect("custom delimiters parse");
        assert_eq!(
            schema.delimiters,
            Delimiters {
                open: "#{".to_owned(),
                close: "}".to_owned(),
            }
        );
    }

    #[test]
    fn parse_rejects_invalid_custom_delimiters_and_variable_defaults() {
        for raw in [
            r##"{"settings":{"delimiters":{"open":"#{"}},"variables":{}}"##,
            r#"{"settings":{"delimiters":{"open":"","close":"}"}},"variables":{}}"#,
            r#"{"settings":{"delimiters":{"open":1,"close":"}"}},"variables":{}}"#,
            r#"{"variables":{"name":{"token":"default"}}}"#,
            r#"{"variables":{"name":false}}"#,
        ] {
            assert!(matches!(
                parse(raw, "template"),
                Err(Error::SchemaInvalid { .. })
            ));
        }
    }

    #[test]
    fn parse_rejects_unknown_keys_and_reserved_help_variable() {
        assert!(matches!(
            parse(r#"{"other":["file"]}"#, "template"),
            Err(Error::SchemaInvalid { .. })
        ));
        assert!(matches!(
            parse(r#"{"variables":{"help":"x"}}"#, "template"),
            Err(Error::ReservedVariable { variable, .. }) if variable == "help"
        ));
    }

    #[test]
    fn parse_accepts_multiple_command_argument_lists() {
        let schema = parse(
            r#"{"commands":[["git","init"],["gh","repo","create"]]}"#,
            "template",
        )
        .expect("schema parses");

        assert_eq!(
            schema.commands,
            vec![
                vec!["git".to_owned(), "init".to_owned()],
                vec!["gh".to_owned(), "repo".to_owned(), "create".to_owned()],
            ]
        );
    }

    #[test]
    fn parse_rejects_commands_without_a_program_or_string_arguments() {
        for raw in [
            r#"{"commands":[[]]}"#,
            r#"{"commands":[[""]]}"#,
            r#"{"commands":[["git",1]]}"#,
            r#"{"commands":"git init"}"#,
        ] {
            assert!(matches!(
                parse(raw, "template"),
                Err(Error::SchemaInvalid { .. })
            ));
        }
    }

    #[test]
    fn parse_rejects_non_string_defaults() {
        assert!(matches!(
            parse(r#"{"variables":{"a":false}}"#, "template"),
            Err(Error::SchemaInvalid { .. })
        ));
    }

    #[test]
    fn load_returns_none_when_schema_is_absent() {
        let path = std::env::temp_dir().join(format!("scaffold-schema-{}", std::process::id()));
        std::fs::create_dir_all(&path).expect("create fixture");
        assert!(load(&path, "empty").unwrap().is_none());
        let _ = std::fs::remove_dir_all(path);
    }
}
