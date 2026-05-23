pub mod ast;
mod error;

mod literal;
mod token;

use ast::RuleAst;
pub use error::{ParseError, ParseErrorKind};

lalrpop_util::lalrpop_mod!(tenon, "/parser/tenon.rs");

type RawParseError<'input> =
    lalrpop_util::ParseError<usize, lalrpop_util::lexer::Token<'input>, ParseError>;

pub fn parse_rules(input: &str) -> Result<Vec<RuleAst>, ParseError> {
    tenon::RulesParser::new()
        .parse(input)
        .map_err(map_lalrpop_error)
}

fn map_lalrpop_error(err: RawParseError<'_>) -> ParseError {
    match err {
        lalrpop_util::ParseError::InvalidToken { location } => ParseError::new(
            ParseErrorKind::InvalidToken,
            Some(ast::Span::new(location, location.saturating_add(1))),
            "invalid token",
        ),
        lalrpop_util::ParseError::UnrecognizedEof { location, expected } => ParseError::new(
            ParseErrorKind::UnexpectedEof,
            Some(ast::Span::new(location, location)),
            expected_message("unexpected end of input", expected),
        ),
        lalrpop_util::ParseError::UnrecognizedToken { token, expected } => ParseError::new(
            ParseErrorKind::UnexpectedToken,
            Some(ast::Span::new(token.0, token.2)),
            expected_message("unexpected token", expected),
        ),
        lalrpop_util::ParseError::ExtraToken { token } => ParseError::new(
            ParseErrorKind::ExtraToken,
            Some(ast::Span::new(token.0, token.2)),
            "extra token",
        ),
        lalrpop_util::ParseError::User { error } => error,
    }
}

fn expected_message(prefix: &str, expected: Vec<String>) -> String {
    if expected.is_empty() {
        return prefix.to_string();
    }
    format!("{prefix}; expected {}", expected.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::{
        BinaryOp, ExprKindAst, FieldSegmentKindAst, LiteralAst, MetadataFieldAst,
        PayloadFormatAst, SourceKindAst,
    };

    #[test]
    fn parses_pipeline_rule() {
        let source = r#"
            on topic "sensor/+/data"
            decode payload as json into p
            guard p.temp > 30 && topic[1] == "room1"
            emit to raw_kafka, local-log {
              "topic": topic[1],
              "payload": p,
              "first_value": p.values[0],
              "score": p.temp + 10
            }
            "#;
        let rules = parse_rules(source).expect("parse rules");
        assert_eq!(rules.len(), 1);
        let rule = &rules[0];

        assert_eq!(
            rule.source.kind,
            SourceKindAst::Topic {
                filter: "sensor/+/data".to_string()
            }
        );
        assert_eq!(rule.decode.format, PayloadFormatAst::Json);
        assert_eq!(rule.decode.alias, "p");
        assert_eq!(rule.emit.destinations, vec!["raw_kafka", "local-log"]);
        assert_eq!(rule.emit.projection.len(), 4);
        assert_eq!(rule.emit.projection[1].name, "payload");
        assert!(matches!(rule.emit.projection[1].expr.kind, ExprKindAst::VariableRoot(ref name) if name == "p"));
        assert_eq!(&source[rule.span.start..rule.span.start + 2], "on");
        assert_eq!(
            &source[rule.emit.projection[0].expr.span.start
                ..rule.emit.projection[0].expr.span.end],
            "topic[1]"
        );
    }

    #[test]
    fn parses_multi_rule_file() {
        let rules = parse_rules(
            r#"
            on topic "a"
            decode payload as json into p
            emit to raw-kafka {
              "payload": p
            };

            on topic "b"
            decode payload as json into p
            emit to json {
              "value": p.value
            }
            "#,
        )
        .expect("parse rules");

        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].emit.destinations, vec!["raw-kafka"]);
        assert_eq!(rules[1].emit.destinations, vec!["json"]);
    }

    #[test]
    fn parses_array_index_and_arithmetic() {
        let rules = parse_rules(
            r#"
            on topic "data"
            decode payload as json into p
            emit to local_log {
              "score": p.values[0] + 10
            }
            "#,
        )
        .expect("parse rules");
        assert_eq!(rules.len(), 1);
        let rule = &rules[0];

        let ExprKindAst::Binary { op, left, right } = &rule.emit.projection[0].expr.kind else {
            panic!("expected binary expression");
        };
        assert_eq!(*op, BinaryOp::Add);
        assert!(matches!(right.kind, ExprKindAst::Literal(LiteralAst::Int(10))));
        assert!(matches!(
            left.kind,
            ExprKindAst::VariableField {
                ref name,
                ref path
            } if name == "p"
                && path.len() == 2
                && matches!(path[0].kind, FieldSegmentKindAst::Name(ref value) if value == "values")
                && matches!(path[1].kind, FieldSegmentKindAst::Index(0))
        ));
    }

    #[test]
    fn parses_mqtt_publish_metadata_fields() {
        let rules = parse_rules(
            r#"
            on topic "data"
            decode payload as json into p
            emit to local_log {
              "dup": metadata.dup,
              "qos": metadata.qos,
              "retain": metadata.retain,
              "pkid": metadata.pkid
            }
            "#,
        )
        .expect("parse rules");
        let projection = &rules[0].emit.projection;

        assert!(matches!(
            projection[0].expr.kind,
            ExprKindAst::Metadata(MetadataFieldAst::Dup)
        ));
        assert!(matches!(
            projection[1].expr.kind,
            ExprKindAst::Metadata(MetadataFieldAst::Qos)
        ));
        assert!(matches!(
            projection[2].expr.kind,
            ExprKindAst::Metadata(MetadataFieldAst::Retain)
        ));
        assert!(matches!(
            projection[3].expr.kind,
            ExprKindAst::Metadata(MetadataFieldAst::Pkid)
        ));
    }

    #[test]
    fn rejects_empty_projection() {
        let err = parse_rules(
            r#"
                on topic "data"
                decode payload as json into p
                emit to local_log {}
                "#,
        )
        .expect_err("empty projection should fail");
        assert_eq!(err.kind, ParseErrorKind::UnexpectedToken);
        assert!(err.span.is_some());
    }

    #[test]
    fn rejects_unquoted_projection_key() {
        let err = parse_rules(
            r#"
                on topic "data"
                decode payload as json into p
                emit to local_log {
                  payload: p
                }
                "#,
        )
        .expect_err("unquoted projection key should fail");
        assert_eq!(err.kind, ParseErrorKind::UnexpectedToken);
        assert!(err.span.is_some());
    }

    #[test]
    fn rejects_chained_comparison() {
        let err = parse_rules(
            r#"
                on topic "data"
                decode payload as json into p
                guard p.temp > 10 > 5
                emit to local_log {
                  "payload": p
                }
                "#,
        )
        .expect_err("chained comparison should fail");
        assert_eq!(err.kind, ParseErrorKind::UnexpectedToken);
        assert!(err.span.is_some());
    }

    #[test]
    fn rejects_reserved_decode_alias() {
        let source = r#"
                on topic "data"
                decode payload as json into topic
                emit to local_log {
                  "payload": p
                }
                "#;
        let err = parse_rules(source).expect_err("reserved alias should fail");
        assert_eq!(err.kind, ParseErrorKind::ReservedKeyword);
        let span = err.span.expect("error span");
        assert_eq!(&source[span.start..span.end], "topic");
    }

    #[test]
    fn rejects_invalid_string_escape() {
        let source = r#"
                on topic "data"
                decode payload as json into p
                emit to local_log {
                  "payload": "\q"
                }
                "#;
        let err = parse_rules(source).expect_err("invalid string escape should fail");
        assert_eq!(err.kind, ParseErrorKind::InvalidLiteral);
        let span = err.span.expect("error span");
        assert_eq!(&source[span.start..span.end], r#""\q""#);
    }

    #[test]
    fn rejects_invalid_token() {
        let source = r#"
            on topic "data"
            decode payload as json into p
            emit to local_log {
                "payload": @
            }
            "#;
        let err = parse_rules(source).expect_err("invalid token");
        assert_eq!(err.kind, ParseErrorKind::InvalidToken);
    }

    #[test]
    fn rejects_unknown_metadata_field() {
        let source = r#"
            on topic "data"
            decode payload as json into p
            emit to local_log {
                "payload": metadata.username
            }
            "#;
        let err = parse_rules(source).expect_err("unknown metadata field");
        assert_eq!(err.kind, ParseErrorKind::UnexpectedToken);
        assert!(err.span.is_some());
    }

    #[test]
    fn rejects_metadata_bracket_access() {
        let source = r#"
            on topic "data"
            decode payload as json into p
            emit to local_log {
                "payload": metadata["qos"]
            }
            "#;
        let err = parse_rules(source).expect_err("metadata bracket access should fail");
        assert_eq!(err.kind, ParseErrorKind::UnexpectedToken);
        assert!(err.span.is_some());
    }

    #[test]
    fn rejects_function_call_syntax() {
        let source = r#"
            on topic "data"
            decode payload as json into p
            emit to local_log {
                "payload": lower(p.name)
            }
            "#;
        let err = parse_rules(source).expect_err("function calls are not in v1 grammar");
        assert_eq!(err.kind, ParseErrorKind::UnexpectedToken);
        assert!(err.span.is_some());
    }
}
