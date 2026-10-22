//! Template schema: `schema.json` with a closed content whitelist and variable registry.

use std::collections::BTreeMap;
use std::path::Path;

use crate::error::{Error, Result};

/// Name of the schema file inside a template directory.
pub const SCHEMA_FILE: &str = "schema.json";

/// Native flag names a schema variable must not shadow.
const RESERVED_NAMES: &[&str] = &["template", "output", "force", "list", "help", "version"];

/// Variables declared by a template: knob name → replacement token → default value.
pub type Variables = BTreeMap<String, BTreeMap<String, String>>;

/// Parsed and validated `schema.json`.
#[derive(Debug, Default)]
pub struct Schema {
    /// Paths exported by the template; a trailing `/` exports a directory tree.
    pub include: Option<Vec<String>>,
    /// Tokens replaceable in exported files, keyed by variable name.
    pub variables: Variables,
}

/// What a template exports, derived from the schema's presence and keys.
#[derive(Debug, PartialEq)]
pub enum Content {
    /// No `schema.json`: copy the whole template directory verbatim.
    All,
    /// Schema present without `include`: copy everything except `schema.json`.
    AllButSchema,
    /// Schema present with `include`: copy only the listed paths.
    Whitelist(Vec<String>),
}

/// Derive the content decision from whether the template declares a schema.
#[must_use]
pub fn content(schema: Option<&Schema>) -> Content {
    match schema {
        None => Content::All,
        Some(schema) => match &schema.include {
            None => Content::AllButSchema,
            Some(paths) => Content::Whitelist(paths.clone()),
        },
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

/// Parse raw JSON into a `Schema`, rejecting reserved names and escaping paths.
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
        if key.as_str() != "include" && key.as_str() != "variables" {
            return Err(invalid(format!("unknown key '{key}'")));
        }
    }
    let include = parse_include(
        object
            .get("include")
            .map_or(&serde_json::Value::Null, |v| v),
        &invalid,
    );
    let variables = parse_variables(
        object
            .get("variables")
            .map_or(&serde_json::Value::Null, |v| v),
        &invalid,
    )?;
    let schema = Schema {
        include: include?,
        variables,
    };
    validate(&schema, template_label)?;
    Ok(schema)
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

fn parse_include(
    value: &serde_json::Value,
    invalid: &(dyn Fn(String) -> Error + '_),
) -> Result<Option<Vec<String>>> {
    match value {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::Array(items) => {
            let mut paths = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    serde_json::Value::String(path) => {
                        if path.is_empty() {
                            return Err(invalid("include entries must not be empty".into()));
                        }
                        paths.push(path.clone());
                    }
                    other => {
                        return Err(invalid(format!(
                            "include entries must be strings, found {}",
                            describe(other.clone())
                        )));
                    }
                }
            }
            Ok(Some(paths))
        }
        other => Err(invalid(format!(
            "'include' must be an array of paths, found {}",
            describe(other.clone())
        ))),
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
            for (name, tokens) in knobs {
                let default_map = tokens.as_object().ok_or_else(|| {
                    invalid(format!("variable '{name}' must map tokens to defaults"))
                })?;
                if default_map.is_empty() {
                    return Err(invalid(format!("variable '{name}' declares no tokens")));
                }
                let mut token_defaults = BTreeMap::new();
                for (token, default) in default_map {
                    if token.is_empty() {
                        return Err(invalid(format!(
                            "variable '{name}' declares an empty token"
                        )));
                    }
                    let default = default.as_str().ok_or_else(|| {
                        invalid(format!("default of token '{token}' must be a string"))
                    })?;
                    token_defaults.insert(token.clone(), default.to_owned());
                }
                variables.insert(name.clone(), token_defaults);
            }
            Ok(variables)
        }
        other => Err(invalid(format!(
            "'variables' must map names to token maps, found {}",
            describe(other.clone())
        ))),
    }
}

/// Check reserved variable names, path escapes, and duplicated tokens.
fn validate(schema: &Schema, template_label: &str) -> Result<()> {
    for name in schema.variables.keys() {
        if RESERVED_NAMES.contains(&name.as_str()) {
            return Err(Error::ReservedVariable {
                template: template_label.to_owned(),
                variable: name.clone(),
            });
        }
    }
    if let Some(paths) = &schema.include {
        for path in paths {
            let normalized = Path::new(path);
            if normalized.is_absolute() {
                return Err(Error::IncludeInvalid {
                    template: template_label.to_owned(),
                    path: path.clone(),
                    reason: "is absolute and would escape the template".to_owned(),
                });
            }
            if normalized
                .components()
                .any(|component| component == std::path::Component::ParentDir)
            {
                return Err(Error::IncludeInvalid {
                    template: template_label.to_owned(),
                    path: path.clone(),
                    reason: "contains '..' and would escape the template".to_owned(),
                });
            }
        }
    }
    let mut seen: Vec<&str> = Vec::new();
    for tokens in schema.variables.values() {
        for token in tokens.keys() {
            if seen.contains(&token.as_str()) {
                return Err(Error::SchemaInvalid {
                    template: template_label.to_owned(),
                    reason: format!("token '{token}' is declared by several variables"),
                });
            }
            seen.push(token);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_reads_include_and_nested_variables() {
        let raw = r#"{
            "include": ["Cargo.toml", "src/"],
            "variables": { "name": { "__name__": "mi_nombre" } }
        }"#;
        let schema = parse(raw, "rust").expect("schema parses");
        assert_eq!(
            schema.include,
            Some(vec!["Cargo.toml".to_owned(), "src/".to_owned()])
        );
        assert_eq!(schema.variables["name"]["__name__"], "mi_nombre");
    }

    #[test]
    fn parse_allows_name_and_rejects_other_natives() {
        let ok = parse(r#"{"variables": {"name": {"__name__": "x"}}}"#, "t").unwrap();
        assert_eq!(ok.variables["name"]["__name__"], "x");

        let raw = r#"{"variables": {"output": {"__out__": "-"}}}"#;
        match parse(raw, "cli") {
            Err(Error::ReservedVariable { template, variable }) => {
                assert_eq!(template, "cli");
                assert_eq!(variable, "output");
            }
            other => panic!("expected ReservedVariable, got {other:?}"),
        }
    }

    #[test]
    fn parse_identifies_the_template_on_broken_json() {
        match parse("{", "broken") {
            Err(Error::SchemaInvalid { template, reason }) => {
                assert_eq!(template, "broken");
                assert!(!reason.is_empty(), "the underlying reason is surfaced");
            }
            other => panic!("expected SchemaInvalid, got {other:?}"),
        }
    }

    #[test]
    fn parse_rejects_unknown_top_level_keys() {
        let raw = r#"{"inculde": ["x"]}"#;
        match parse(raw, "typo") {
            Err(Error::SchemaInvalid { reason, .. }) => {
                assert!(reason.contains("unknown key"), "reason: {reason}");
            }
            other => panic!("expected SchemaInvalid, got {other:?}"),
        }
    }

    #[test]
    fn parse_rejects_malformed_include_and_variables() {
        let raw = r#"{"include": "Cargo.toml"}"#;
        assert!(parse(raw, "t").is_err());

        let raw = r#"{"variables": {"watch": "__watch__"}}"#;
        assert!(parse(raw, "t").is_err());

        let raw = r#"{"include": ["ok", 7]}"#;
        assert!(parse(raw, "t").is_err());

        let raw = r#"{"variables": {"name": {"" : "x"}}}"#;
        assert!(parse(raw, "t").is_err());
    }

    #[test]
    fn parse_rejects_includes_escaping_the_template() {
        let raw = r#"{"include": ["a/../b"]}"#;
        match parse(raw, "escape") {
            Err(Error::IncludeInvalid {
                template,
                path,
                reason,
            }) => {
                assert_eq!(template, "escape");
                assert_eq!(path, "a/../b");
                assert!(reason.contains(".."));
            }
            other => panic!("expected IncludeInvalid, got {other:?}"),
        }

        let raw = r#"{"include": ["/etc/os-release"]}"#;
        assert!(parse(raw, "abs").is_err());
    }

    #[test]
    fn parse_rejects_tokens_declared_by_several_variables() {
        let raw = r#"{"variables": {
            "name": {"__name__": "a"},
            "slug": {"__name__": "b"}
        }}"#;
        match parse(raw, "dup") {
            Err(Error::SchemaInvalid { reason, .. }) => {
                assert!(reason.contains("__name__"), "reason: {reason}");
            }
            other => panic!("expected SchemaInvalid, got {other:?}"),
        }
    }

    #[test]
    fn content_derives_the_three_levels() {
        assert_eq!(content(None), Content::All);
        assert_eq!(
            content(Some(
                &parse(r#"{"variables": {"name": {"__n__": "x"}}}"#, "t").unwrap()
            )),
            Content::AllButSchema
        );
        assert_eq!(
            content(Some(&parse(r#"{"include": ["a"]}"#, "t").unwrap())),
            Content::Whitelist(vec!["a".to_owned()])
        );
    }

    #[test]
    fn load_reports_absence_as_none() {
        let dir = std::env::temp_dir().join(format!("schema-load-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(load(&dir, "no-schema").unwrap().is_none());

        std::fs::write(dir.join(SCHEMA_FILE), r#"{"include": []}"#).unwrap();
        let loaded = load(&dir, "with-schema").expect("loads");
        assert!(loaded.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
