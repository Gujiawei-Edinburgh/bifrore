#![allow(dead_code)]

use crate::parser::ast::{
    BinaryOp, FieldSegment, LiteralAst, MetadataFieldAst, SourceKindAst, Span, UnaryOp,
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CheckedRuleSet {
    pub(crate) rules: Vec<CheckedRule>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CheckedRule {
    pub(crate) source: CheckedSource,
    pub(crate) decode: CheckedDecode,
    pub(crate) guard: Option<CheckedExpr>,
    pub(crate) emit: CheckedEmit,
    pub(crate) span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CheckedSource {
    pub(crate) kind: SourceKindAst,
    pub(crate) span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CheckedDecode {
    pub(crate) alias: String,
    pub(crate) span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CheckedEmit {
    pub(crate) destinations: Vec<String>,
    pub(crate) projection: Vec<CheckedProjectionItem>,
    pub(crate) span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CheckedProjectionItem {
    pub(crate) name: String,
    pub(crate) expr: CheckedExpr,
    pub(crate) span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CheckedExpr {
    pub(crate) kind: CheckedExprKind,
    pub(crate) value_type: ValueType,
    pub(crate) span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValueType {
    Null,
    Bool,
    String,
    Number(NumberKind),
    Dynamic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumberKind {
    Int,
    UInt,
    Float,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CheckedExprKind {
    Literal(LiteralAst),
    TopicLevel(usize),
    Property(String),
    Metadata(MetadataFieldAst),
    PayloadRoot,
    PayloadField(Vec<FieldSegment>),
    Unary {
        op: UnaryOp,
        expr: Box<CheckedExpr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<CheckedExpr>,
        right: Box<CheckedExpr>,
    },
}

impl CheckedRuleSet {
    pub(crate) fn new(rules: Vec<CheckedRule>) -> Self {
        Self { rules }
    }
}

impl CheckedExpr {
    pub(crate) fn new(kind: CheckedExprKind, value_type: ValueType, span: Span) -> Self {
        Self {
            kind,
            value_type,
            span,
        }
    }
}
