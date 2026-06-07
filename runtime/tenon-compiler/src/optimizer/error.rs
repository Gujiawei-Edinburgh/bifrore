#![allow(dead_code)]

use crate::parser::ast::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OptimizerError {
    pub(crate) kind: OptimizerErrorKind,
    pub(crate) span: Span,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OptimizerErrorKind {
    DeadRule,
}

impl OptimizerError {
    pub(crate) fn new(
        kind: OptimizerErrorKind,
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

impl std::fmt::Display for OptimizerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for OptimizerError {}
