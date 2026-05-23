const RESERVED_KEYWORDS: &[&str] = &[
    "on",
    "topic",
    "decode",
    "payload",
    "as",
    "json",
    "into",
    "guard",
    "emit",
    "to",
    "true",
    "false",
    "null",
    "properties",
    "metadata",
    "raw_payload",
];

pub fn validate_ident(raw: &str) -> Result<String, String> {
    if RESERVED_KEYWORDS.contains(&raw) {
        return Err(format!("reserved keyword cannot be used as identifier: {raw}"));
    }
    Ok(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_reserved_keyword() {
        assert!(validate_ident("topic").is_err());
    }

    #[test]
    fn accepts_regular_identifier() {
        assert_eq!(validate_ident("p").expect("identifier"), "p");
    }
}
