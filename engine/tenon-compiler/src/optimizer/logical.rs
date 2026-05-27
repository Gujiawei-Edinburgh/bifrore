#![allow(dead_code)]

use crate::ir::logical::{
    LogicalExpr, LogicalExprKind, LogicalProjectionItem, LogicalRule, LogicalRuleSet,
};
use crate::optimizer::error::{OptimizerError, OptimizerErrorKind};
use crate::parser::ast::{BinaryOp, LiteralAst, MetadataFieldAst, Span, UnaryOp};
use crate::semantic::checked::{NumberKind, ValueType};

pub(crate) fn optimize_rule_set(rule_set: LogicalRuleSet) -> Result<LogicalRuleSet, OptimizerError> {
    rule_set
        .rules
        .into_iter()
        .map(optimize_rule)
        .collect::<Result<Vec<_>, _>>()
        .map(|rules| LogicalRuleSet { rules })
}

fn optimize_rule(rule: LogicalRule) -> Result<LogicalRule, OptimizerError> {
    let guard = rule.guard.map(optimize_expr);
    let guard = match guard {
        Some(expr) if bool_literal(&expr) == Some(false) => {
            return Err(OptimizerError::new(
                OptimizerErrorKind::DeadRule,
                expr.span,
                "rule guard is always false after static optimization",
            ));
        }
        Some(expr) if bool_literal(&expr) == Some(true) => None,
        other => other,
    };

    Ok(LogicalRule {
        source: rule.source,
        guard,
        projection: rule
            .projection
            .into_iter()
            .map(|item| LogicalProjectionItem {
                name: item.name,
                expr: optimize_expr(item.expr),
                span: item.span,
            })
            .collect(),
        destinations: rule.destinations,
        span: rule.span,
    })
}

fn optimize_expr(expr: LogicalExpr) -> LogicalExpr {
    match expr.kind {
        LogicalExprKind::Unary { op, expr: inner } => {
            let inner = optimize_expr(*inner);
            fold_unary(op, inner, expr.span).unwrap_or_else(|inner| LogicalExpr {
                kind: LogicalExprKind::Unary {
                    op,
                    expr: Box::new(inner),
                },
                value_type: expr.value_type,
                span: expr.span,
            })
        }
        LogicalExprKind::Binary { op, left, right } => {
            let left = optimize_expr(*left);
            let right = optimize_expr(*right);
            fold_binary(op, left, right, expr.span, expr.value_type).unwrap_or_else(
                |(left, right)| LogicalExpr {
                    kind: LogicalExprKind::Binary {
                        op,
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                    value_type: expr.value_type,
                    span: expr.span,
                },
            )
        }
        _ => expr,
    }
}

fn fold_unary(
    op: UnaryOp,
    expr: LogicalExpr,
    span: Span,
) -> Result<LogicalExpr, LogicalExpr> {
    match (op, &expr.kind) {
        (UnaryOp::Not, LogicalExprKind::Literal(LiteralAst::Bool(value))) => {
            Ok(literal_expr(LiteralAst::Bool(!value), span))
        }
        (UnaryOp::Neg, LogicalExprKind::Literal(LiteralAst::Int(value))) => value
            .checked_neg()
            .map(|value| literal_expr(LiteralAst::Int(value), span))
            .ok_or(expr),
        (UnaryOp::Neg, LogicalExprKind::Literal(LiteralAst::Float(value))) => {
            Ok(literal_expr(LiteralAst::Float(-value), span))
        }
        (UnaryOp::Not, LogicalExprKind::Unary { op: UnaryOp::Not, expr: inner })
            if inner.value_type == ValueType::Bool =>
        {
            Ok((**inner).clone())
        }
        _ => Err(expr),
    }
}

fn fold_binary(
    op: BinaryOp,
    left: LogicalExpr,
    right: LogicalExpr,
    span: Span,
    value_type: ValueType,
) -> Result<LogicalExpr, (LogicalExpr, LogicalExpr)> {
    if let Some(expr) = fold_literal_binary(op, &left, &right, span) {
        return Ok(expr);
    }
    if let Some(expr) = fold_domain_binary(op, &left, &right, span) {
        return Ok(expr);
    }
    if let Some(expr) = fold_bool_binary(op, &left, &right, span, value_type) {
        return Ok(expr);
    }
    Err((left, right))
}

fn fold_bool_binary(
    op: BinaryOp,
    left: &LogicalExpr,
    right: &LogicalExpr,
    span: Span,
    value_type: ValueType,
) -> Option<LogicalExpr> {
    match (op, bool_literal(left), bool_literal(right)) {
        (BinaryOp::And, Some(false), _) => Some(literal_expr(LiteralAst::Bool(false), span)),
        (BinaryOp::And, Some(true), _) => Some(right.clone()),
        (BinaryOp::And, _, Some(true)) => Some(left.clone()),
        (BinaryOp::Or, Some(true), _) => Some(literal_expr(LiteralAst::Bool(true), span)),
        (BinaryOp::Or, Some(false), _) => Some(right.clone()),
        (BinaryOp::Or, _, Some(false)) => Some(left.clone()),
        (BinaryOp::And | BinaryOp::Or, _, _) if value_type == ValueType::Bool => None,
        _ => None,
    }
}

fn fold_literal_binary(
    op: BinaryOp,
    left: &LogicalExpr,
    right: &LogicalExpr,
    span: Span,
) -> Option<LogicalExpr> {
    let (LogicalExprKind::Literal(left), LogicalExprKind::Literal(right)) =
        (&left.kind, &right.kind)
    else {
        return None;
    };

    match op {
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => {
            fold_literal_arithmetic(op, left, right, span)
        }
        BinaryOp::Gt | BinaryOp::Ge | BinaryOp::Lt | BinaryOp::Le => {
            fold_literal_ordering(op, left, right, span)
        }
        BinaryOp::Eq | BinaryOp::Ne => fold_literal_equality(op, left, right, span),
        BinaryOp::And | BinaryOp::Or => {
            let (LiteralAst::Bool(left), LiteralAst::Bool(right)) = (left, right) else {
                return None;
            };
            let value = match op {
                BinaryOp::And => *left && *right,
                BinaryOp::Or => *left || *right,
                _ => unreachable!(),
            };
            Some(literal_expr(LiteralAst::Bool(value), span))
        }
    }
}

fn fold_literal_arithmetic(
    op: BinaryOp,
    left: &LiteralAst,
    right: &LiteralAst,
    span: Span,
) -> Option<LogicalExpr> {
    match (left, right) {
        (LiteralAst::Int(left), LiteralAst::Int(right)) => {
            let value = match op {
                BinaryOp::Add => left.checked_add(*right)?,
                BinaryOp::Sub => left.checked_sub(*right)?,
                BinaryOp::Mul => left.checked_mul(*right)?,
                BinaryOp::Div if *right != 0 => left.checked_div(*right)?,
                BinaryOp::Rem if *right != 0 => left.checked_rem(*right)?,
                _ => return None,
            };
            Some(literal_expr(LiteralAst::Int(value), span))
        }
        _ => {
            let left = numeric_literal_as_f64(left)?;
            let right = numeric_literal_as_f64(right)?;
            if matches!(op, BinaryOp::Div | BinaryOp::Rem) && right == 0.0 {
                return None;
            }
            let value = match op {
                BinaryOp::Add => left + right,
                BinaryOp::Sub => left - right,
                BinaryOp::Mul => left * right,
                BinaryOp::Div => left / right,
                BinaryOp::Rem => left % right,
                _ => return None,
            };
            Some(literal_expr(LiteralAst::Float(value), span))
        }
    }
}

fn fold_literal_ordering(
    op: BinaryOp,
    left: &LiteralAst,
    right: &LiteralAst,
    span: Span,
) -> Option<LogicalExpr> {
    let value = if let (Some(left), Some(right)) =
        (numeric_literal_as_f64(left), numeric_literal_as_f64(right))
    {
        match op {
            BinaryOp::Gt => left > right,
            BinaryOp::Ge => left >= right,
            BinaryOp::Lt => left < right,
            BinaryOp::Le => left <= right,
            _ => return None,
        }
    } else {
        let (LiteralAst::String(left), LiteralAst::String(right)) = (left, right) else {
            return None;
        };
        match op {
            BinaryOp::Gt => left > right,
            BinaryOp::Ge => left >= right,
            BinaryOp::Lt => left < right,
            BinaryOp::Le => left <= right,
            _ => return None,
        }
    };
    Some(literal_expr(LiteralAst::Bool(value), span))
}

fn fold_literal_equality(
    op: BinaryOp,
    left: &LiteralAst,
    right: &LiteralAst,
    span: Span,
) -> Option<LogicalExpr> {
    let value = if let (Some(left), Some(right)) =
        (numeric_literal_as_f64(left), numeric_literal_as_f64(right))
    {
        left == right
    } else {
        match (left, right) {
            (LiteralAst::Null, LiteralAst::Null) => true,
            (LiteralAst::Bool(left), LiteralAst::Bool(right)) => left == right,
            (LiteralAst::String(left), LiteralAst::String(right)) => left == right,
            _ => return None,
        }
    };
    Some(literal_expr(LiteralAst::Bool(matches!(op, BinaryOp::Eq) == value), span))
}

fn fold_domain_binary(
    op: BinaryOp,
    left: &LogicalExpr,
    right: &LogicalExpr,
    span: Span,
) -> Option<LogicalExpr> {
    if let (Some(domain), Some(value)) = (metadata_domain(left), numeric_expr_as_f64(right)) {
        return fold_domain_comparison(domain, op, value)
            .map(|value| literal_expr(LiteralAst::Bool(value), span));
    }
    if let (Some(value), Some(domain)) = (numeric_expr_as_f64(left), metadata_domain(right)) {
        return fold_domain_comparison(domain, swap_comparison(op)?, value)
            .map(|value| literal_expr(LiteralAst::Bool(value), span));
    }
    None
}

fn fold_domain_comparison(domain: NumericDomain, op: BinaryOp, value: f64) -> Option<bool> {
    if !value.is_finite() {
        return None;
    }
    match domain {
        NumericDomain::IntegerSet(values) => fold_integer_set_comparison(values, op, value),
        NumericDomain::IntegerRange { min, max } => {
            fold_integer_range_comparison(min, max, op, value)
        }
    }
}

fn fold_integer_set_comparison(values: &[i64], op: BinaryOp, value: f64) -> Option<bool> {
    let (first, rest) = values.split_first()?;
    let first = compare_integer_to_number(*first, op, value);
    if rest
        .iter()
        .all(|domain_value| compare_integer_to_number(*domain_value, op, value) == first)
    {
        return Some(first);
    }
    None
}

fn fold_integer_range_comparison(min: i64, max: i64, op: BinaryOp, value: f64) -> Option<bool> {
    let min_float = min as f64;
    let max_float = max as f64;
    match op {
        BinaryOp::Gt if min_float > value => Some(true),
        BinaryOp::Gt if max_float <= value => Some(false),
        BinaryOp::Ge if min_float >= value => Some(true),
        BinaryOp::Ge if max_float < value => Some(false),
        BinaryOp::Lt if max_float < value => Some(true),
        BinaryOp::Lt if min_float >= value => Some(false),
        BinaryOp::Le if max_float <= value => Some(true),
        BinaryOp::Le if min_float > value => Some(false),
        BinaryOp::Eq if value < min_float || value > max_float || !value_is_integer(value) => {
            Some(false)
        }
        BinaryOp::Eq if min == max => Some(compare_integer_to_number(min, op, value)),
        BinaryOp::Ne if value < min_float || value > max_float || !value_is_integer(value) => {
            Some(true)
        }
        BinaryOp::Ne if min == max => Some(compare_integer_to_number(min, op, value)),
        _ => None,
    }
}

fn compare_integer_to_number(domain_value: i64, op: BinaryOp, value: f64) -> bool {
    let domain_value = domain_value as f64;
    match op {
        BinaryOp::Gt => domain_value > value,
        BinaryOp::Ge => domain_value >= value,
        BinaryOp::Lt => domain_value < value,
        BinaryOp::Le => domain_value <= value,
        BinaryOp::Eq => domain_value == value,
        BinaryOp::Ne => domain_value != value,
        _ => false,
    }
}

fn swap_comparison(op: BinaryOp) -> Option<BinaryOp> {
    match op {
        BinaryOp::Gt => Some(BinaryOp::Lt),
        BinaryOp::Ge => Some(BinaryOp::Le),
        BinaryOp::Lt => Some(BinaryOp::Gt),
        BinaryOp::Le => Some(BinaryOp::Ge),
        BinaryOp::Eq => Some(BinaryOp::Eq),
        BinaryOp::Ne => Some(BinaryOp::Ne),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy)]
enum NumericDomain {
    IntegerRange { min: i64, max: i64 },
    IntegerSet(&'static [i64]),
}

fn metadata_domain(expr: &LogicalExpr) -> Option<NumericDomain> {
    match expr.kind {
        LogicalExprKind::Metadata(MetadataFieldAst::Qos) => {
            Some(NumericDomain::IntegerSet(&[0, 1, 2]))
        }
        LogicalExprKind::Metadata(MetadataFieldAst::Pkid) => Some(NumericDomain::IntegerRange {
            min: 0,
            max: u16::MAX as i64,
        }),
        _ => None,
    }
}

fn numeric_expr_as_f64(expr: &LogicalExpr) -> Option<f64> {
    let LogicalExprKind::Literal(literal) = &expr.kind else {
        return None;
    };
    numeric_literal_as_f64(literal)
}

fn numeric_literal_as_f64(literal: &LiteralAst) -> Option<f64> {
    match literal {
        LiteralAst::Int(value) => Some(*value as f64),
        LiteralAst::Float(value) => Some(*value),
        _ => None,
    }
}

fn value_is_integer(value: f64) -> bool {
    value.fract() == 0.0
}

fn bool_literal(expr: &LogicalExpr) -> Option<bool> {
    let LogicalExprKind::Literal(LiteralAst::Bool(value)) = expr.kind else {
        return None;
    };
    Some(value)
}

fn literal_expr(literal: LiteralAst, span: Span) -> LogicalExpr {
    let value_type = match &literal {
        LiteralAst::Null => ValueType::Null,
        LiteralAst::Bool(_) => ValueType::Bool,
        LiteralAst::Int(_) => ValueType::Number(NumberKind::Int),
        LiteralAst::Float(_) => ValueType::Number(NumberKind::Float),
        LiteralAst::String(_) => ValueType::String,
    };
    LogicalExpr {
        kind: LogicalExprKind::Literal(literal),
        value_type,
        span,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::logical::{lower_checked_rule_set, LogicalExprKind};
    use crate::parser::parse_rules;
    use crate::semantic::analyze_rules;

    fn optimize_source(source: &str) -> Result<LogicalRuleSet, OptimizerError> {
        let rules = parse_rules(source).expect("parse");
        let checked = analyze_rules(rules).expect("semantic");
        optimize_rule_set(lower_checked_rule_set(checked))
    }

    #[test]
    fn folds_literal_arithmetic_and_comparison() {
        let optimized = optimize_source(
            r#"
            on topic "data"
            decode payload as json into p
            emit to local_log {
              "value": 1 + 2 * 3,
              "cmp": "b" > "a"
            }
            "#,
        )
        .expect("optimize");
        let projection = &optimized.rules[0].projection;

        assert!(matches!(
            projection[0].expr.kind,
            LogicalExprKind::Literal(LiteralAst::Int(7))
        ));
        assert!(matches!(
            projection[1].expr.kind,
            LogicalExprKind::Literal(LiteralAst::Bool(true))
        ));
    }

    #[test]
    fn simplifies_safe_boolean_expressions() {
        let optimized = optimize_source(
            r#"
            on topic "data"
            decode payload as json into p
            emit to local_log {
              "and": true && metadata.retain,
              "or": false || metadata.retain,
              "not": !!metadata.retain
            }
            "#,
        )
        .expect("optimize");
        let projection = &optimized.rules[0].projection;

        for item in projection {
            assert!(matches!(
                item.expr.kind,
                LogicalExprKind::Metadata(MetadataFieldAst::Retain)
            ));
        }
    }

    #[test]
    fn folds_mqtt_metadata_domains() {
        let optimized = optimize_source(
            r#"
            on topic "data"
            decode payload as json into p
            emit to local_log {
              "qos_hi": metadata.qos > 2,
              "pkid_range": metadata.pkid <= 65535,
              "qos_fraction": metadata.qos == 1.5,
              "qos_partial": metadata.qos > 1.5
            }
            "#,
        )
        .expect("optimize");
        let projection = &optimized.rules[0].projection;

        assert!(matches!(
            projection[0].expr.kind,
            LogicalExprKind::Literal(LiteralAst::Bool(false))
        ));
        assert!(matches!(
            projection[1].expr.kind,
            LogicalExprKind::Literal(LiteralAst::Bool(true))
        ));
        assert!(matches!(
            projection[2].expr.kind,
            LogicalExprKind::Literal(LiteralAst::Bool(false))
        ));
        assert!(matches!(
            projection[3].expr.kind,
            LogicalExprKind::Binary {
                op: BinaryOp::Gt,
                ..
            }
        ));
    }

    #[test]
    fn rejects_dead_rule_when_guard_is_proven_false() {
        let err = optimize_source(
            r#"
            on topic "data"
            decode payload as json into p
            guard metadata.qos > 2
            emit to local_log {
              "payload": p
            }
            "#,
        )
        .expect_err("dead rule");

        assert_eq!(err.kind, OptimizerErrorKind::DeadRule);
    }

    #[test]
    fn does_not_hide_dynamic_left_side_errors() {
        let optimized = optimize_source(
            r#"
            on topic "data"
            decode payload as json into p
            guard p.temp > 10 && metadata.qos > 2
            emit to local_log {
              "payload": p
            }
            "#,
        )
        .expect("dynamic left side should not be folded to dead rule");

        assert!(matches!(
            optimized.rules[0].guard.as_ref().unwrap().kind,
            LogicalExprKind::Binary {
                op: BinaryOp::And,
                ..
            }
        ));
    }

    #[test]
    fn rejects_dead_rule_when_static_false_short_circuits_dynamic_right() {
        let err = optimize_source(
            r#"
            on topic "data"
            decode payload as json into p
            guard metadata.qos > 2 && p.temp > 10
            emit to local_log {
              "payload": p
            }
            "#,
        )
        .expect_err("static false left side should create dead rule");

        assert_eq!(err.kind, OptimizerErrorKind::DeadRule);
    }
}
