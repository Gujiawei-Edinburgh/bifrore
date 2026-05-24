#![allow(dead_code)]

pub(crate) mod checked;
pub(crate) mod error;

use std::collections::HashSet;

use crate::parser::ast::{
    BinaryOp, DecodeAst, EmitAst, ExprAst, ExprKindAst, LiteralAst, MetadataFieldAst,
    ProjectionItemAst, RuleAst, SourceAst, Span, UnaryOp,
};

use checked::{
    CheckedDecode, CheckedEmit, CheckedExpr, CheckedExprKind, CheckedProjectionItem, CheckedRule,
    CheckedRuleSet, CheckedSource, NumberKind, ValueType,
};
use error::{SemanticError, SemanticErrorKind};

pub(crate) fn analyze_rules(rules: Vec<RuleAst>) -> Result<CheckedRuleSet, SemanticError> {
    rules
        .into_iter()
        .map(analyze_rule)
        .collect::<Result<Vec<_>, _>>()
        .map(CheckedRuleSet::new)
}

fn analyze_rule(rule: RuleAst) -> Result<CheckedRule, SemanticError> {
    let alias = rule.decode.alias.clone();
    let guard = rule
        .guard
        .map(|expr| analyze_expr(expr, &alias))
        .transpose()?;
    if let Some(expr) = &guard {
        require_guard_type(expr)?;
    }
    Ok(CheckedRule {
        source: analyze_source(rule.source),
        decode: analyze_decode(rule.decode),
        guard,
        emit: analyze_emit(rule.emit, &alias)?,
        span: rule.span,
    })
}

fn analyze_source(source: SourceAst) -> CheckedSource {
    CheckedSource {
        kind: source.kind,
        span: source.span,
    }
}

fn analyze_decode(decode: DecodeAst) -> CheckedDecode {
    CheckedDecode {
        alias: decode.alias,
        span: decode.span,
    }
}

fn analyze_emit(emit: EmitAst, alias: &str) -> Result<CheckedEmit, SemanticError> {
    reject_duplicate_destinations(&emit)?;
    Ok(CheckedEmit {
        destinations: emit.destinations,
        projection: emit
            .projection
            .into_iter()
            .map(|item| analyze_projection_item(item, alias))
            .collect::<Result<Vec<_>, _>>()?,
        span: emit.span,
    })
}

fn analyze_projection_item(
    item: ProjectionItemAst,
    alias: &str,
) -> Result<CheckedProjectionItem, SemanticError> {
    Ok(CheckedProjectionItem {
        name: item.name,
        expr: analyze_expr(item.expr, alias)?,
        span: item.span,
    })
}

fn analyze_expr(expr: ExprAst, alias: &str) -> Result<CheckedExpr, SemanticError> {
    let span = expr.span;
    let (kind, value_type) = match expr.kind {
        ExprKindAst::Literal(value) => {
            let value_type = literal_type(&value);
            (CheckedExprKind::Literal(value), value_type)
        }
        ExprKindAst::TopicLevel(level) => (CheckedExprKind::TopicLevel(level), ValueType::String),
        ExprKindAst::Property(key) => (CheckedExprKind::Property(key), ValueType::Dynamic),
        ExprKindAst::Metadata(field) => {
            let value_type = metadata_type(field);
            (CheckedExprKind::Metadata(field), value_type)
        }
        ExprKindAst::VariableRoot(name) if name == alias => {
            (CheckedExprKind::PayloadRoot, ValueType::Dynamic)
        }
        ExprKindAst::VariableField { name, path } if name == alias => {
            (CheckedExprKind::PayloadField(path), ValueType::Dynamic)
        }
        ExprKindAst::VariableRoot(name) | ExprKindAst::VariableField { name, .. } => {
            return Err(SemanticError::new(
                SemanticErrorKind::UnknownVariable,
                span,
                format!("unknown variable root: {name}"),
            ));
        }
        ExprKindAst::Unary { op, expr } => {
            let checked = analyze_expr(*expr, alias)?;
            let value_type = infer_unary_type(op, &checked, span)?;
            (
                CheckedExprKind::Unary {
                    op,
                    expr: Box::new(checked),
                },
                value_type,
            )
        }
        ExprKindAst::Binary { op, left, right } => {
            let left = analyze_expr(*left, alias)?;
            let right = analyze_expr(*right, alias)?;
            let value_type = infer_binary_type(op, &left, &right, span)?;
            (
                CheckedExprKind::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                value_type,
            )
        }
    };

    Ok(CheckedExpr::new(kind, value_type, span))
}

fn literal_type(literal: &LiteralAst) -> ValueType {
    match literal {
        LiteralAst::Null => ValueType::Null,
        LiteralAst::Bool(_) => ValueType::Bool,
        LiteralAst::Int(_) => ValueType::Number(NumberKind::Int),
        LiteralAst::Float(_) => ValueType::Number(NumberKind::Float),
        LiteralAst::String(_) => ValueType::String,
    }
}

fn metadata_type(field: MetadataFieldAst) -> ValueType {
    match field {
        MetadataFieldAst::Dup | MetadataFieldAst::Retain => ValueType::Bool,
        MetadataFieldAst::Qos | MetadataFieldAst::Pkid => ValueType::Number(NumberKind::UInt),
    }
}

fn infer_unary_type(
    op: UnaryOp,
    expr: &CheckedExpr,
    span: Span,
) -> Result<ValueType, SemanticError> {
    match op {
        UnaryOp::Not if accepts_bool_like(expr.value_type) => Ok(ValueType::Bool),
        UnaryOp::Not => Err(type_mismatch(span, "operator ! expects bool or dynamic")),
        UnaryOp::Neg if expr.value_type == ValueType::Dynamic => Ok(ValueType::Dynamic),
        UnaryOp::Neg if is_number(expr.value_type) => Ok(expr.value_type),
        UnaryOp::Neg => Err(type_mismatch(span, "operator - expects number or dynamic")),
    }
}

fn infer_binary_type(
    op: BinaryOp,
    left: &CheckedExpr,
    right: &CheckedExpr,
    span: Span,
) -> Result<ValueType, SemanticError> {
    match op {
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => {
            infer_arithmetic_type(left.value_type, right.value_type, span)
        }
        BinaryOp::Gt | BinaryOp::Ge | BinaryOp::Lt | BinaryOp::Le => {
            infer_ordering_type(left.value_type, right.value_type, span)
        }
        BinaryOp::Eq | BinaryOp::Ne => infer_equality_type(left.value_type, right.value_type, span),
        BinaryOp::And | BinaryOp::Or => infer_logical_type(left.value_type, right.value_type, span),
    }
}

fn infer_arithmetic_type(
    left: ValueType,
    right: ValueType,
    span: Span,
) -> Result<ValueType, SemanticError> {
    if left == ValueType::Dynamic || right == ValueType::Dynamic {
        return Ok(ValueType::Dynamic);
    }
    let (ValueType::Number(left), ValueType::Number(right)) = (left, right) else {
        return Err(type_mismatch(span, "arithmetic operators expect numbers or dynamic values"));
    };
    Ok(ValueType::Number(promote_number(left, right)))
}

fn infer_ordering_type(
    left: ValueType,
    right: ValueType,
    span: Span,
) -> Result<ValueType, SemanticError> {
    if left == ValueType::Dynamic || right == ValueType::Dynamic {
        return Ok(ValueType::Bool);
    }
    if is_number(left) && is_number(right) {
        return Ok(ValueType::Bool);
    }
    if left == ValueType::String && right == ValueType::String {
        return Ok(ValueType::Bool);
    }
    Err(type_mismatch(
        span,
        "ordering comparisons expect compatible numbers, strings, or dynamic values",
    ))
}

fn infer_equality_type(
    left: ValueType,
    right: ValueType,
    span: Span,
) -> Result<ValueType, SemanticError> {
    if left == ValueType::Dynamic || right == ValueType::Dynamic {
        return Ok(ValueType::Bool);
    }
    if left == right || (is_number(left) && is_number(right)) {
        return Ok(ValueType::Bool);
    }
    Err(type_mismatch(
        span,
        "equality comparisons expect compatible values or dynamic values",
    ))
}

fn infer_logical_type(
    left: ValueType,
    right: ValueType,
    span: Span,
) -> Result<ValueType, SemanticError> {
    if accepts_bool_like(left) && accepts_bool_like(right) {
        return Ok(ValueType::Bool);
    }
    Err(type_mismatch(
        span,
        "logical operators expect bool or dynamic values",
    ))
}

fn require_guard_type(expr: &CheckedExpr) -> Result<(), SemanticError> {
    if accepts_bool_like(expr.value_type) {
        return Ok(());
    }
    Err(type_mismatch(
        expr.span,
        "guard expression must be bool or dynamic",
    ))
}

fn accepts_bool_like(value_type: ValueType) -> bool {
    matches!(value_type, ValueType::Bool | ValueType::Dynamic)
}

fn is_number(value_type: ValueType) -> bool {
    matches!(value_type, ValueType::Number(_))
}

fn promote_number(left: NumberKind, right: NumberKind) -> NumberKind {
    if left == NumberKind::Float || right == NumberKind::Float {
        return NumberKind::Float;
    }
    if left == NumberKind::Int || right == NumberKind::Int {
        return NumberKind::Int;
    }
    NumberKind::UInt
}

fn type_mismatch(span: Span, message: impl Into<String>) -> SemanticError {
    SemanticError::new(SemanticErrorKind::TypeMismatch, span, message)
}

fn reject_duplicate_destinations(emit: &EmitAst) -> Result<(), SemanticError> {
    let mut destinations = HashSet::with_capacity(emit.destinations.len());
    for destination in &emit.destinations {
        if !destinations.insert(destination.as_str()) {
            return Err(SemanticError::new(
                SemanticErrorKind::DuplicateDestination,
                emit.span,
                format!("duplicate destination: {destination}"),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_rules;
    use crate::semantic::checked::{CheckedExprKind, NumberKind, ValueType};

    #[test]
    fn resolves_payload_alias_references() {
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
        let rule = &checked.rules[0];
        assert!(matches!(
            rule.emit.projection[0].expr.kind,
            CheckedExprKind::PayloadRoot
        ));
        assert_eq!(rule.emit.projection[0].expr.value_type, ValueType::Dynamic);
        assert!(matches!(
            rule.emit.projection[1].expr.kind,
            CheckedExprKind::PayloadField(ref path) if path.len() == 1
        ));
        assert_eq!(rule.emit.projection[1].expr.value_type, ValueType::Dynamic);
    }

    #[test]
    fn rejects_unknown_variable_root() {
        let source = r#"
            on topic "data"
            decode payload as json into p
            emit to local_log {
              "temp": device.temp
            }
            "#;
        let rules = parse_rules(source).expect("parse");
        let err = analyze_rules(rules).expect_err("unknown variable");

        assert_eq!(err.kind, SemanticErrorKind::UnknownVariable);
        assert_eq!(&source[err.span.start..err.span.end], "device.temp");
    }

    #[test]
    fn rejects_duplicate_destinations() {
        let rules = parse_rules(
            r#"
            on topic "data"
            decode payload as json into p
            emit to local_log, local_log {
              "payload": p
            }
            "#,
        )
        .expect("parse");
        let err = analyze_rules(rules).expect_err("duplicate destination");

        assert_eq!(err.kind, SemanticErrorKind::DuplicateDestination);
    }

    #[test]
    fn infers_known_root_types() {
        let rules = parse_rules(
            r#"
            on topic "data"
            decode payload as json into p
            emit to local_log {
              "topic": topic[0],
              "prop": properties["x"],
              "dup": metadata.dup,
              "qos": metadata.qos,
              "retain": metadata.retain,
              "pkid": metadata.pkid
            }
            "#,
        )
        .expect("parse");
        let checked = analyze_rules(rules).expect("semantic");
        let projection = &checked.rules[0].emit.projection;

        assert_eq!(projection[0].expr.value_type, ValueType::String);
        assert_eq!(projection[1].expr.value_type, ValueType::Dynamic);
        assert_eq!(projection[2].expr.value_type, ValueType::Bool);
        assert_eq!(projection[3].expr.value_type, ValueType::Number(NumberKind::UInt));
        assert_eq!(projection[4].expr.value_type, ValueType::Bool);
        assert_eq!(projection[5].expr.value_type, ValueType::Number(NumberKind::UInt));
    }

    #[test]
    fn promotes_int_float_arithmetic_to_float() {
        let rules = parse_rules(
            r#"
            on topic "data"
            decode payload as json into p
            emit to local_log {
              "value": metadata.qos + 1.5
            }
            "#,
        )
        .expect("parse");
        let checked = analyze_rules(rules).expect("semantic");

        assert_eq!(
            checked.rules[0].emit.projection[0].expr.value_type,
            ValueType::Number(NumberKind::Float)
        );
    }

    #[test]
    fn allows_dynamic_payload_arithmetic_and_comparison() {
        let rules = parse_rules(
            r#"
            on topic "data"
            decode payload as json into p
            guard p.temp > 10
            emit to local_log {
              "value": p.temp + 10,
              "cmp": p.temp > p.hum,
              "flag": p.is_hot && metadata.retain,
              "prop": properties["x"] + 10
            }
            "#,
        )
        .expect("parse");
        let checked = analyze_rules(rules).expect("semantic");
        let rule = &checked.rules[0];

        assert_eq!(rule.guard.as_ref().unwrap().value_type, ValueType::Bool);
        assert_eq!(rule.emit.projection[0].expr.value_type, ValueType::Dynamic);
        assert_eq!(rule.emit.projection[1].expr.value_type, ValueType::Bool);
        assert_eq!(rule.emit.projection[2].expr.value_type, ValueType::Bool);
        assert_eq!(rule.emit.projection[3].expr.value_type, ValueType::Dynamic);
    }

    #[test]
    fn rejects_static_arithmetic_type_mismatch() {
        let rules = parse_rules(
            r#"
            on topic "data"
            decode payload as json into p
            emit to local_log {
              "value": metadata.retain + 10
            }
            "#,
        )
        .expect("parse");
        let err = analyze_rules(rules).expect_err("type mismatch");

        assert_eq!(err.kind, SemanticErrorKind::TypeMismatch);
    }

    #[test]
    fn rejects_static_topic_numeric_comparison() {
        let source = r#"
            on topic "data"
            decode payload as json into p
            guard topic[0] > 10
            emit to local_log {
              "payload": p
            }
            "#;
        let rules = parse_rules(source).expect("parse");
        let err = analyze_rules(rules).expect_err("type mismatch");

        assert_eq!(err.kind, SemanticErrorKind::TypeMismatch);
        assert_eq!(&source[err.span.start..err.span.end], "topic[0] > 10");
    }

    #[test]
    fn rejects_non_bool_guard() {
        let rules = parse_rules(
            r#"
            on topic "data"
            decode payload as json into p
            guard metadata.qos
            emit to local_log {
              "payload": p
            }
            "#,
        )
        .expect("parse");
        let err = analyze_rules(rules).expect_err("guard type mismatch");

        assert_eq!(err.kind, SemanticErrorKind::TypeMismatch);
    }
}
