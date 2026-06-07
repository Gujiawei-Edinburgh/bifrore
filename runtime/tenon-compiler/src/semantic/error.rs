#![allow(dead_code)]

use crate::parser::ast::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SemanticError {
    pub(crate) kind: SemanticErrorKind,
    pub(crate) span: Span,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SemanticErrorKind {
    UnknownVariable,
    DuplicateDestination,
    InvalidTopicFilter,
    TypeMismatch,
}

impl SemanticError {
    pub(crate) fn new(
        kind: SemanticErrorKind,
        span: Span,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            span,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for SemanticError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SemanticError {}
