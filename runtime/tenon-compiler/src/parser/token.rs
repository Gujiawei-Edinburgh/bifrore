use crate::parser::ast::Span;
use crate::parser::error::{ParseError, ParseErrorKind};

pub(crate) fn reject_reserved_keyword(raw: &str, span: Span) -> Result<String, ParseError> {
    Err(ParseError::new(
        ParseErrorKind::ReservedKeyword,
        Some(span),
        format!("reserved keyword cannot be used as identifier: {raw}"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::new(0, 5)
    }

    #[test]
    fn rejects_reserved_keyword() {
        let err = reject_reserved_keyword("topic", span()).expect_err("reserved keyword");
        assert_eq!(err.kind, ParseErrorKind::ReservedKeyword);
        assert_eq!(err.span, Some(span()));
    }
}
