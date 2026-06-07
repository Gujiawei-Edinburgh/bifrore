#![allow(dead_code)]

use crate::parser::ast::{
    BinaryOp, FieldSegment, LiteralAst, MetadataFieldAst, SourceKindAst, Span, UnaryOp,
};
use crate::semantic::checked::{
    CheckedExpr, CheckedExprKind, CheckedRule, CheckedRuleSet, ValueType,
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LogicalRuleSet {
    pub(crate) rules: Vec<LogicalRule>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LogicalRule {
    pub(crate) source: LogicalSource,
    pub(crate) guard: Option<LogicalExpr>,
    pub(crate) projection: Vec<LogicalProjectionItem>,
    pub(crate) destinations: Vec<String>,
    pub(crate) span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LogicalSource {
    Topic { filter: String, span: Span },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LogicalProjectionItem {
    pub(crate) name: String,
    pub(crate) expr: LogicalExpr,
    pub(crate) span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LogicalExpr {
    pub(crate) kind: LogicalExprKind,
    pub(crate) value_type: ValueType,
    pub(crate) span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LogicalExprKind {
    Literal(LiteralAst),
    TopicLevel(usize),
    Property(String),
    Metadata(MetadataFieldAst),
    PayloadRoot,
    PayloadField(Vec<FieldSegment>),
    Unary {
        op: UnaryOp,
        expr: Box<LogicalExpr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<LogicalExpr>,
        right: Box<LogicalExpr>,
    },
}

pub(crate) fn lower_checked_rule_set(checked: CheckedRuleSet) -> LogicalRuleSet {
    LogicalRuleSet {
        rules: checked.rules.into_iter().map(lower_rule).collect(),
    }
}

fn lower_rule(rule: CheckedRule) -> LogicalRule {
    LogicalRule {
        source: match rule.source.kind {
            SourceKindAst::Topic { filter } => LogicalSource::Topic {
                filter,
                span: rule.source.span,
            },
        },
        guard: rule.guard.map(lower_expr),
        projection: rule
            .emit
            .projection
            .into_iter()
            .map(|item| LogicalProjectionItem {
                name: item.name,
                expr: lower_expr(item.expr),
                span: item.span,
            })
            .collect(),
        destinations: rule.emit.destinations,
        span: rule.span,
    }
}

fn lower_expr(expr: CheckedExpr) -> LogicalExpr {
    let kind = match expr.kind {
        CheckedExprKind::Literal(value) => LogicalExprKind::Literal(value),
        CheckedExprKind::TopicLevel(level) => LogicalExprKind::TopicLevel(level),
        CheckedExprKind::Property(key) => LogicalExprKind::Property(key),
        CheckedExprKind::Metadata(field) => LogicalExprKind::Metadata(field),
        CheckedExprKind::PayloadRoot => LogicalExprKind::PayloadRoot,
        CheckedExprKind::PayloadField(path) => LogicalExprKind::PayloadField(path),
        CheckedExprKind::Unary { op, expr } => LogicalExprKind::Unary {
            op,
            expr: Box::new(lower_expr(*expr)),
        },
        CheckedExprKind::Binary { op, left, right } => LogicalExprKind::Binary {
            op,
            left: Box::new(lower_expr(*left)),
            right: Box::new(lower_expr(*right)),
        },
    };

    LogicalExpr {
        kind,
        value_type: expr.value_type,
        span: expr.span,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_rules;
    use crate::semantic::analyze_rules;

    #[test]
    fn lowers_checked_rules_to_logical_ir() {
        let rules = parse_rules(
            r#"
            on topic "data"
            decode payload as json into p
            guard p.temp > 30
            emit to local_log {
              "payload": p,
              "temp": p.temp
            }
            "#,
        )
        .expect("parse");
        let checked = analyze_rules(rules).expect("semantic");
        let logical = lower_checked_rule_set(checked);

        assert_eq!(logical.rules.len(), 1);
        let rule = &logical.rules[0];
        assert!(matches!(
            rule.source,
            LogicalSource::Topic {
                ref filter,
                ..
            } if filter == "data"
        ));
        assert!(rule.guard.is_some());
        assert_eq!(rule.destinations, vec!["local_log"]);
        assert_eq!(rule.projection.len(), 2);
        assert!(matches!(
            rule.projection[0].expr.kind,
            LogicalExprKind::PayloadRoot
        ));
        assert!(matches!(
            rule.projection[1].expr.kind,
            LogicalExprKind::PayloadField(ref path) if path.len() == 1
        ));
    }
}
