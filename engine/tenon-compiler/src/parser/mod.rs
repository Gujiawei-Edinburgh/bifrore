pub mod ast;

mod literal;
mod token;

use ast::RuleAst;

lalrpop_util::lalrpop_mod!(tenon, "/parser/tenon.rs");

pub type ParseError<'input> =
    lalrpop_util::ParseError<usize, lalrpop_util::lexer::Token<'input>, String>;

pub fn parse_rules(input: &str) -> Result<Vec<RuleAst>, ParseError<'_>> {
    tenon::RulesParser::new().parse(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::{
        BinaryOp, ExprKindAst, FieldSegmentKindAst, LiteralAst, PayloadFormatAst,
        SourceKindAst,
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
    fn rejects_empty_projection() {
        assert!(
            parse_rules(
                r#"
                on topic "data"
                decode payload as json into p
                emit to local_log {}
                "#,
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_unquoted_projection_key() {
        assert!(
            parse_rules(
                r#"
                on topic "data"
                decode payload as json into p
                emit to local_log {
                  payload: p
                }
                "#,
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_chained_comparison() {
        assert!(
            parse_rules(
                r#"
                on topic "data"
                decode payload as json into p
                guard p.temp > 10 > 5
                emit to local_log {
                  "payload": p
                }
                "#,
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_reserved_decode_alias() {
        assert!(
            parse_rules(
                r#"
                on topic "data"
                decode payload as json into topic
                emit to local_log {
                  "payload": p
                }
                "#,
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_invalid_string_escape() {
        assert!(
            parse_rules(
                r#"
                on topic "data"
                decode payload as json into p
                emit to local_log {
                  "payload": "\q"
                }
                "#,
            )
            .is_err()
        );
    }
}
