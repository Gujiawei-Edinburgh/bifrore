#[derive(Debug, Clone, PartialEq)]
pub struct RuleAst {
    pub source: SourceAst,
    pub decode: DecodeAst,
    pub guard: Option<ExprAst>,
    pub emit: EmitAst,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceAst {
    Topic {
        filter: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeAst {
    pub format: PayloadFormatAst,
    pub alias: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadFormatAst {
    Json,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmitAst {
    pub destinations: Vec<String>,
    pub projection: Vec<ProjectionItemAst>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectionItemAst {
    pub name: String,
    pub expr: ExprAst,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprAst {
    Literal(LiteralAst),
    TopicLevel(usize),
    Property(String),
    Metadata(String),
    VariableRoot(String),
    VariableField {
        name: String,
        path: Vec<FieldSegment>,
    },
    Call {
        name: String,
        args: Vec<ExprAst>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<ExprAst>,
    },
    Binary {
        op: BinaryOp,
        left: Box<ExprAst>,
        right: Box<ExprAst>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum LiteralAst {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldSegment {
    Name(String),
    Index(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Neg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Or,
    And,
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

pub(crate) fn binary_expr(op: BinaryOp, left: ExprAst, right: ExprAst) -> ExprAst {
    ExprAst::Binary {
        op,
        left: Box::new(left),
        right: Box::new(right),
    }
}

pub(crate) fn fold_binary_expr(head: ExprAst, tail: Vec<(BinaryOp, ExprAst)>) -> ExprAst {
    tail.into_iter()
        .fold(head, |left, (op, right)| binary_expr(op, left, right))
}
