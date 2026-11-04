//! Literal token substitution with a closed, schema-declared token set.

use std::borrow::Cow;
use std::collections::BTreeMap;

use crate::schema::{Delimiters, Variables};

/// Variable values and delimiters used during text substitution.
#[derive(Debug, PartialEq)]
pub struct Replacements {
    values: BTreeMap<String, String>,
    open: String,
    close: String,
}

impl Default for Replacements {
    fn default() -> Self {
        let delimiters = Delimiters::default();
        Self {
            values: BTreeMap::new(),
            open: delimiters.open,
            close: delimiters.close,
        }
    }
}

impl Replacements {
    /// Bind command-line overrides to schema defaults.
    #[must_use]
    pub fn bind(
        variables: &Variables,
        overrides: &BTreeMap<String, String>,
        delimiters: &Delimiters,
    ) -> Self {
        let values = variables
            .iter()
            .map(|(name, default)| (name.clone(), overrides.get(name).unwrap_or(default).clone()))
            .collect();
        Self {
            values,
            open: delimiters.open.clone(),
            close: delimiters.close.clone(),
        }
    }
}

/// Replace declared variables between the configured delimiters.
#[must_use]
pub fn substitute(input: &str, replacements: &Replacements) -> String {
    let mut output = String::with_capacity(input.len());
    let mut remaining = input;
    while let Some(open_index) = remaining.find(&replacements.open) {
        output.push_str(&remaining[..open_index]);
        let value_start = open_index + replacements.open.len();
        let after_open = &remaining[value_start..];
        let Some(close_index) = after_open.find(&replacements.close) else {
            output.push_str(&remaining[open_index..]);
            return output;
        };
        let key = after_open[..close_index].trim();
        if let Some(value) = replacements.values.get(key) {
            output.push_str(value);
        } else {
            let value_end = value_start + close_index + replacements.close.len();
            output.push_str(&remaining[open_index..value_end]);
        }
        let consumed = value_start + close_index + replacements.close.len();
        remaining = &remaining[consumed..];
    }
    output.push_str(remaining);
    output
}

/// Substitute a file's bytes when valid UTF-8, otherwise copy them verbatim.
pub fn substitute_bytes<'a>(input: &'a [u8], replacements: &Replacements) -> Cow<'a, [u8]> {
    match std::str::from_utf8(input) {
        Ok(text) => Cow::Owned(substitute(text, replacements).into_bytes()),
        Err(_) => Cow::Borrowed(input),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bind(raw: &str, overrides: &[(&str, &str)]) -> Replacements {
        let schema = crate::schema::parse(raw, "engine").expect("schema parses");
        let overrides: BTreeMap<String, String> = overrides
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        Replacements::bind(&schema.variables, &overrides, &schema.delimiters)
    }

    #[test]
    fn replaces_every_occurrence_of_every_token() {
        let replacements = bind(r#"{"variables": {"name": "demo"}}"#, &[]);
        let out = substitute("{{name}} x {{name}} {{name}}", &replacements);
        assert_eq!(out, "demo x demo demo");
    }

    #[test]
    fn trims_whitespace_inside_default_delimiters() {
        let replacements = bind(r#"{"variables": {"name": "demo"}}"#, &[]);
        assert_eq!(substitute("{{ name }}", &replacements), "demo");
    }

    #[test]
    fn custom_delimiters_trim_whitespace_and_leave_unknowns_untouched() {
        let replacements = bind(
            r##"{"settings":{"delimiters":{"open":"#{","close":"}"}},"variables":{"name":"demo"}}"##,
            &[],
        );
        assert_eq!(
            substitute("#{ name } #{other} {{name}}", &replacements),
            "demo #{other} {{name}}"
        );
    }

    #[test]
    fn inserted_values_are_not_substituted_again() {
        let replacements = bind(
            r#"{"variables":{"name":"{{other}}","other":"changed"}}"#,
            &[],
        );
        assert_eq!(substitute("{{name}}", &replacements), "{{other}}");
    }

    #[test]
    fn variable_override_replaces_its_default() {
        let replacements = bind(r#"{"variables":{"name":"default"}}"#, &[("name", "demo")]);
        assert_eq!(substitute("{{name}}", &replacements), "demo");
    }

    #[test]
    fn binary_bytes_pass_through_verbatim() {
        let replacements = bind(r#"{"variables": {"name": "demo"}}"#, &[]);
        // Built at runtime so the compiler cannot statically check UTF-8 validity.
        let bad = u8::MAX;
        let bytes = [bad, 0xFE, 0x00, b'a'];
        assert!(std::str::from_utf8(&bytes).is_err());
        let result = substitute_bytes(&bytes, &replacements);
        assert_eq!(result, Cow::<[u8]>::Borrowed(bytes.as_slice()));
    }

    #[test]
    fn valid_utf8_bytes_are_substituted() {
        let replacements = bind(r#"{"variables": {"name": "demo"}}"#, &[]);
        let result = substitute_bytes(b"pub fn {{name}}() {}", &replacements);
        assert_eq!(result, Cow::<[u8]>::Owned(b"pub fn demo() {}".to_vec()));
    }
}
