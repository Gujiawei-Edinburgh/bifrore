use crate::parser::ast::Span;
use crate::parser::error::{ParseError, ParseErrorKind};

pub(crate) fn unescape_string(raw: &str, span: Span) -> Result<String, ParseError> {
    let inner = raw
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| invalid_literal(span, "string literal must be quoted"))?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let escaped = chars
            .next()
            .ok_or_else(|| invalid_literal(span, "unterminated string escape"))?;
        match escaped {
            '"' => out.push('"'),
            '\\' => out.push('\\'),
            '/' => out.push('/'),
            'b' => out.push('\u{0008}'),
            'f' => out.push('\u{000c}'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'u' => {
                let code = read_unicode_escape(&mut chars, span)?;
                push_unicode_escape(&mut out, &mut chars, code, span)?;
            }
            other => return Err(invalid_literal(span, format!("unsupported string escape: \\{other}"))),
        }
    }
    Ok(out)
}

pub(crate) fn parse_int_literal(raw: &str, span: Span) -> Result<i64, ParseError> {
    raw.parse::<i64>()
        .map_err(|err| invalid_literal(span, format!("invalid integer literal {raw}: {err}")))
}

pub(crate) fn parse_index_literal(raw: &str, span: Span) -> Result<usize, ParseError> {
    raw.parse::<usize>()
        .map_err(|err| invalid_literal(span, format!("invalid index literal {raw}: {err}")))
}

pub(crate) fn parse_float_literal(raw: &str, span: Span) -> Result<f64, ParseError> {
    raw.parse::<f64>()
        .map_err(|err| invalid_literal(span, format!("invalid float literal {raw}: {err}")))
}

fn read_unicode_escape(chars: &mut std::str::Chars<'_>, span: Span) -> Result<u32, ParseError> {
    let mut code = 0u32;
    for _ in 0..4 {
        let ch = chars
            .next()
            .ok_or_else(|| invalid_literal(span, "incomplete unicode escape"))?;
        let Some(value) = ch.to_digit(16) else {
            return Err(invalid_literal(span, format!("invalid unicode escape digit: {ch}")));
        };
        code = (code << 4) | value;
    }
    Ok(code)
}

fn push_unicode_escape(
    out: &mut String,
    chars: &mut std::str::Chars<'_>,
    code: u32,
    span: Span,
) -> Result<(), ParseError> {
    if (0xD800..=0xDBFF).contains(&code) {
        let Some('\\') = chars.next() else {
            return Err(invalid_literal(span, "high surrogate must be followed by a low surrogate"));
        };
        let Some('u') = chars.next() else {
            return Err(invalid_literal(span, "high surrogate must be followed by a unicode escape"));
        };
        let low = read_unicode_escape(chars, span)?;
        if !(0xDC00..=0xDFFF).contains(&low) {
            return Err(invalid_literal(span, format!("invalid low surrogate: {low:x}")));
        }
        let scalar = 0x10000 + (((code - 0xD800) << 10) | (low - 0xDC00));
        let Some(decoded) = char::from_u32(scalar) else {
            return Err(invalid_literal(span, format!("invalid unicode escape: {scalar:x}")));
        };
        out.push(decoded);
        return Ok(());
    }
    if (0xDC00..=0xDFFF).contains(&code) {
        return Err(invalid_literal(span, format!("low surrogate without high surrogate: {code:x}")));
    }
    let Some(decoded) = char::from_u32(code) else {
        return Err(invalid_literal(span, format!("invalid unicode escape: {code:x}")));
    };
    out.push(decoded);
    Ok(())
}

fn invalid_literal(span: Span, message: impl Into<String>) -> ParseError {
    ParseError::new(ParseErrorKind::InvalidLiteral, Some(span), message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::new(0, 1)
    }

    #[test]
    fn unescapes_json_style_string() {
        assert_eq!(
            unescape_string(r#""a\n\"b\"""#, span()).expect("string"),
            "a\n\"b\""
        );
    }

    #[test]
    fn rejects_unknown_escape() {
        let err = unescape_string(r#""a\q""#, span()).expect_err("invalid escape");
        assert_eq!(err.kind, ParseErrorKind::InvalidLiteral);
    }

    #[test]
    fn unescapes_surrogate_pair() {
        assert_eq!(
            unescape_string(r#""\uD83D\uDE00""#, span()).expect("string"),
            "😀"
        );
    }

    #[test]
    fn rejects_integer_overflow() {
        let err = parse_int_literal("999999999999999999999999999999", span())
            .expect_err("integer overflow");
        assert_eq!(err.kind, ParseErrorKind::InvalidLiteral);
    }
}
