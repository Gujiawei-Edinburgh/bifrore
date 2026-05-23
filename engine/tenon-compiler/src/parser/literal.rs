pub(crate) fn unescape_string(raw: &str) -> Result<String, String> {
    let inner = raw
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| "string literal must be quoted".to_string())?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let escaped = chars
            .next()
            .ok_or_else(|| "unterminated string escape".to_string())?;
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
                let code = read_unicode_escape(&mut chars)?;
                push_unicode_escape(&mut out, &mut chars, code)?;
            }
            other => return Err(format!("unsupported string escape: \\{other}")),
        }
    }
    Ok(out)
}

pub(crate) fn parse_int_literal(raw: &str) -> Result<i64, String> {
    raw.parse::<i64>()
        .map_err(|err| format!("invalid integer literal {raw}: {err}"))
}

pub(crate) fn parse_index_literal(raw: &str) -> Result<usize, String> {
    raw.parse::<usize>()
        .map_err(|err| format!("invalid index literal {raw}: {err}"))
}

pub(crate) fn parse_float_literal(raw: &str) -> Result<f64, String> {
    raw.parse::<f64>()
        .map_err(|err| format!("invalid float literal {raw}: {err}"))
}

fn read_unicode_escape(chars: &mut std::str::Chars<'_>) -> Result<u32, String> {
    let mut code = 0u32;
    for _ in 0..4 {
        let ch = chars
            .next()
            .ok_or_else(|| "incomplete unicode escape".to_string())?;
        let Some(value) = ch.to_digit(16) else {
            return Err(format!("invalid unicode escape digit: {ch}"));
        };
        code = (code << 4) | value;
    }
    Ok(code)
}

fn push_unicode_escape(
    out: &mut String,
    chars: &mut std::str::Chars<'_>,
    code: u32,
) -> Result<(), String> {
    if (0xD800..=0xDBFF).contains(&code) {
        let Some('\\') = chars.next() else {
            return Err("high surrogate must be followed by a low surrogate".to_string());
        };
        let Some('u') = chars.next() else {
            return Err("high surrogate must be followed by a unicode escape".to_string());
        };
        let low = read_unicode_escape(chars)?;
        if !(0xDC00..=0xDFFF).contains(&low) {
            return Err(format!("invalid low surrogate: {low:x}"));
        }
        let scalar = 0x10000 + (((code - 0xD800) << 10) | (low - 0xDC00));
        let Some(decoded) = char::from_u32(scalar) else {
            return Err(format!("invalid unicode escape: {scalar:x}"));
        };
        out.push(decoded);
        return Ok(());
    }
    if (0xDC00..=0xDFFF).contains(&code) {
        return Err(format!("low surrogate without high surrogate: {code:x}"));
    }
    let Some(decoded) = char::from_u32(code) else {
        return Err(format!("invalid unicode escape: {code:x}"));
    };
    out.push(decoded);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescapes_json_style_string() {
        assert_eq!(
            unescape_string(r#""a\n\"b\"""#).expect("string"),
            "a\n\"b\""
        );
    }

    #[test]
    fn rejects_unknown_escape() {
        assert!(unescape_string(r#""a\q""#).is_err());
    }

    #[test]
    fn unescapes_surrogate_pair() {
        assert_eq!(unescape_string(r#""\uD83D\uDE00""#).expect("string"), "😀");
    }

    #[test]
    fn rejects_integer_overflow() {
        assert!(parse_int_literal("999999999999999999999999999999").is_err());
    }
}
