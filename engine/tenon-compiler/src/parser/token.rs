use crate::parser::ast::Span;
use crate::parser::error::{ParseError, ParseErrorKind};

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

pub fn validate_ident(raw: &str, span: Span) -> Result<String, ParseError> {
    if RESERVED_KEYWORDS.contains(&raw) {
        return Err(ParseError::new(
            ParseErrorKind::ReservedKeyword,
            Some(span),
            format!("reserved keyword cannot be used as identifier: {raw}"),
        ));
    }
    Ok(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::new(0, 5)
    }

    #[test]
    fn rejects_reserved_keyword() {
        let err = validate_ident("topic", span()).expect_err("reserved keyword");
        assert_eq!(err.kind, ParseErrorKind::ReservedKeyword);
        assert_eq!(err.span, Some(span()));
    }

    #[test]
    fn accepts_regular_identifier() {
        assert_eq!(validate_ident("p", span()).expect("identifier"), "p");
    }
}
