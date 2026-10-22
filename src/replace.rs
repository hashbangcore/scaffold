//! Literal token substitution with a closed, schema-declared token set.

use std::borrow::Cow;
use std::collections::BTreeMap;

use crate::schema::Variables;

/// Token → value pairs ready for substitution, longest token first.
#[derive(Debug, Default, PartialEq)]
pub struct Replacements(Vec<(String, String)>);

impl Replacements {
    /// Bind every token: the knob's flag value replaces all of its tokens,
    /// otherwise each token keeps its own default.
    ///
    /// The universal `--name` alias reaches this layer as an override for the
    /// variable `name`; the CLI decides whether the schema declares it.
    #[must_use]
    pub fn bind(variables: &Variables, overrides: &BTreeMap<String, String>) -> Self {
        let mut pairs = Vec::new();
        for (name, tokens) in variables {
            let given = overrides.get(name);
            for (token, default) in tokens {
                let value = match given {
                    Some(value) => value.clone(),
                    None => default.clone(),
                };
                pairs.push((token.clone(), value));
            }
        }
        // Longest first so `__name__` is never eaten by a `__name` prefix.
        pairs.sort_by_key(|(token, _)| std::cmp::Reverse(token.len()));
        Self(pairs)
    }

    /// The ordered pairs backing the substitution.
    #[must_use]
    pub fn pairs(&self) -> &[(String, String)] {
        &self.0
    }
}

/// Replace every occurrence of every token in a UTF-8 document.
#[must_use]
pub fn substitute(input: &str, replacements: &Replacements) -> String {
    let mut out = input.to_owned();
    for (token, value) in replacements.pairs() {
        out = out.replace(token, value);
    }
    out
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
        Replacements::bind(&schema.variables, &overrides)
    }

    #[test]
    fn replaces_every_occurrence_of_every_token() {
        let replacements = bind(r#"{"variables": {"name": {"__name__": "demo"}}}"#, &[]);
        let out = substitute("__name__ x __name__ __name__", &replacements);
        assert_eq!(out, "demo x demo demo");
    }

    #[test]
    fn longest_token_wins_over_its_strict_prefix() {
        let replacements = bind(
            r#"{"variables": {"name": {"__name__": "FULL", "__name": "SHORT"}}}"#,
            &[],
        );
        assert_eq!(out_order(&replacements), ["__name__", "__name"]);
        let out = substitute("__name__ __name __name_x", &replacements);
        assert_eq!(out, "FULL SHORT SHORT_x");
    }

    #[test]
    fn unregistered_tokens_pass_through_untouched() {
        let replacements = bind(r#"{"variables": {"name": {"__name__": "demo"}}}"#, &[]);
        let out = substitute("{{ ansible_var }} __init__ __name__", &replacements);
        assert_eq!(out, "{{ ansible_var }} __init__ demo");
    }

    #[test]
    fn knob_flag_replaces_all_of_its_tokens() {
        let replacements = bind(
            r#"{"variables": {"watch": {"__watch__": "true", "__WATCH__": "TRUE"}}}"#,
            &[("watch", "false")],
        );
        let out = substitute("__watch__ __WATCH__", &replacements);
        assert_eq!(out, "false false");
    }

    #[test]
    fn name_alias_binds_the_name_variable() {
        let replacements = bind(
            r#"{"variables": {"name": {"__name__": "a", "__slug__": "b"}}}"#,
            &[("name", "demo")],
        );
        let out = substitute("__name__ __slug__", &replacements);
        assert_eq!(out, "demo demo");
    }

    #[test]
    fn binary_bytes_pass_through_verbatim() {
        let replacements = bind(r#"{"variables": {"name": {"__name__": "demo"}}}"#, &[]);
        // Built at runtime so the compiler cannot statically check UTF-8 validity.
        let bad = u8::MAX;
        let bytes = [bad, 0xFE, 0x00, b'a'];
        assert!(std::str::from_utf8(&bytes).is_err());
        let result = substitute_bytes(&bytes, &replacements);
        assert_eq!(result, Cow::<[u8]>::Borrowed(bytes.as_slice()));
    }

    #[test]
    fn valid_utf8_bytes_are_substituted() {
        let replacements = bind(r#"{"variables": {"name": {"__name__": "demo"}}}"#, &[]);
        let result = substitute_bytes(b"pub fn __name__() {}", &replacements);
        assert_eq!(result, Cow::<[u8]>::Owned(b"pub fn demo() {}".to_vec()));
    }

    fn out_order(replacements: &Replacements) -> Vec<&str> {
        replacements
            .pairs()
            .iter()
            .map(|(token, _)| token.as_str())
            .collect()
    }
}
