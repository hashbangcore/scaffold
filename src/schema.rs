//! Template schema: `schema.json` with a closed variable registry.

use crate::error::{Error, Result};
use std::collections::BTreeMap;
use std::path::Path;

/// Name of the schema file inside a template directory.
pub const SCHEMA_FILE: &str = "schema.json";

/// Native flag names a schema variable must not shadow.
const RESERVED_NAMES: &[&str] = &["help"];

/// Variables declared by a template: knob name → replacement token → default value.
pub type Variables = BTreeMap<String, BTreeMap<String, String>>;

/// Parsed and validated `schema.json`.
#[derive(Debug, Default)]
pub struct Schema {
    /// Tokens replaceable in copied files, keyed by variable name.
    pub variables: Variables,
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
        if key.as_str() != "variables" {
            return Err(invalid(format!("unknown key '{key}'")));
        }
    }
    let variables = parse_variables(
        object
            .get("variables")
            .map_or(&serde_json::Value::Null, |v| v),
        &invalid,
    )?;
    let schema = Schema { variables };
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
    fn parse_reads_variable_tokens_and_defaults() {
        let schema = parse(
            r#"{"variables":{"name":{"__name__":"demo"},"watch":{"__watch__":"true"}}}"#,
            "just/publish",
        )
        .expect("schema parses");

        assert_eq!(schema.variables["name"]["__name__"], "demo");
        assert_eq!(schema.variables["watch"]["__watch__"], "true");
    }

    #[test]
    fn parse_rejects_unknown_keys_and_reserved_help_variable() {
        assert!(matches!(
            parse(r#"{"include":["file"]}"#, "template"),
            Err(Error::SchemaInvalid { .. })
        ));
        assert!(matches!(
            parse(r#"{"variables":{"help":{"__help__":"x"}}}"#, "template"),
            Err(Error::ReservedVariable { variable, .. }) if variable == "help"
        ));
    }

    #[test]
    fn parse_rejects_duplicate_tokens_and_non_string_defaults() {
        assert!(matches!(
            parse(
                r#"{"variables":{"a":{"__same__":"x"},"b":{"__same__":"y"}}}"#,
                "template"
            ),
            Err(Error::SchemaInvalid { .. })
        ));
        assert!(matches!(
            parse(r#"{"variables":{"a":{"__a__":false}}}"#, "template"),
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
