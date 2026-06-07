#![allow(dead_code)]

pub(crate) mod error;
mod logical;

use crate::ir::logical::LogicalRuleSet;

use error::OptimizerError;

pub(crate) fn optimize_rule_set(
    rule_set: LogicalRuleSet,
) -> Result<LogicalRuleSet, OptimizerError> {
    logical::optimize_rule_set(rule_set)
}
