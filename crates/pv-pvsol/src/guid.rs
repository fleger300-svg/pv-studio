//! PV*SOL writes the same GUID as `8f1e5b3f-…` in `Project.xml` and as `{8F1E5B3F-…}` in the
//! 3-D files: both are normalised to lower case without braces.

/// Normalised GUID text.
#[must_use]
pub fn normalise(s: &str) -> String {
    s.trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .to_ascii_lowercase()
}

/// As a UUID (`None` when the text is not one).
#[must_use]
pub fn uuid(s: &str) -> Option<uuid::Uuid> {
    uuid::Uuid::parse_str(&normalise(s)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_spellings_meet() {
        assert_eq!(
            normalise("{8F1E5B3F-0EA8-4FAC-B113-0F1725422BD9}"),
            normalise("8f1e5b3f-0ea8-4fac-b113-0f1725422bd9")
        );
        assert!(uuid("{8F1E5B3F-0EA8-4FAC-B113-0F1725422BD9}").is_some());
        assert!(uuid("nope").is_none());
    }
}
