#![allow(dead_code)]

pub(crate) mod checked;
pub(crate) mod error;

use std::collections::HashSet;

use crate::parser::ast::{
    DecodeAst, EmitAst, ExprAst, ExprKindAst, ProjectionItemAst, RuleAst, SourceAst,
};

use checked::{
    CheckedDecode, CheckedEmit, CheckedExpr, CheckedExprKind, CheckedProjectionItem, CheckedRule,
    CheckedRuleSet, CheckedSource,
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
    Ok(CheckedRule {
        source: analyze_source(rule.source),
        decode: analyze_decode(rule.decode),
        guard: rule
            .guard
            .map(|expr| analyze_expr(expr, &alias))
            .transpose()?,
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
    let kind = match expr.kind {
        ExprKindAst::Literal(value) => CheckedExprKind::Literal(value),
        ExprKindAst::TopicLevel(level) => CheckedExprKind::TopicLevel(level),
        ExprKindAst::Property(key) => CheckedExprKind::Property(key),
        ExprKindAst::Metadata(field) => CheckedExprKind::Metadata(field),
        ExprKindAst::VariableRoot(name) if name == alias => CheckedExprKind::PayloadRoot,
        ExprKindAst::VariableField { name, path } if name == alias => {
            CheckedExprKind::PayloadField(path)
        }
        ExprKindAst::VariableRoot(name) | ExprKindAst::VariableField { name, .. } => {
            return Err(SemanticError::new(
                SemanticErrorKind::UnknownVariable,
                span,
                format!("unknown variable root: {name}"),
            ));
        }
        ExprKindAst::Unary { op, expr } => CheckedExprKind::Unary {
            op,
            expr: Box::new(analyze_expr(*expr, alias)?),
        },
        ExprKindAst::Binary { op, left, right } => CheckedExprKind::Binary {
            op,
            left: Box::new(analyze_expr(*left, alias)?),
            right: Box::new(analyze_expr(*right, alias)?),
        },
    };

    Ok(CheckedExpr::new(kind, span))
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
    use crate::semantic::checked::CheckedExprKind;

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
        assert!(matches!(
            rule.emit.projection[1].expr.kind,
            CheckedExprKind::PayloadField(ref path) if path.len() == 1
        ));
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
}
