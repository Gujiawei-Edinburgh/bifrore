#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub(crate) fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub(crate) fn join(self, other: Self) -> Self {
        Self {
            start: self.start,
            end: other.end,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuleAst {
    pub source: SourceAst,
    pub decode: DecodeAst,
    pub guard: Option<ExprAst>,
    pub emit: EmitAst,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceAst {
    pub kind: SourceKindAst,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceKindAst {
    Topic {
        filter: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeAst {
    pub format: PayloadFormatAst,
    pub alias: String,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadFormatAst {
    Json,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmitAst {
    pub destinations: Vec<String>,
    pub projection: Vec<ProjectionItemAst>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectionItemAst {
    pub name: String,
    pub expr: ExprAst,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExprAst {
    pub kind: ExprKindAst,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKindAst {
    Literal(LiteralAst),
    TopicLevel(usize),
    Property(String),
    Metadata(String),
    VariableRoot(String),
    VariableField {
        name: String,
        path: Vec<FieldSegment>,
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
pub struct FieldSegment {
    pub kind: FieldSegmentKindAst,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldSegmentKindAst {
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

impl ExprAst {
    pub(crate) fn new(kind: ExprKindAst, span: Span) -> Self {
        Self { kind, span }
    }

    pub(crate) fn with_span(mut self, span: Span) -> Self {
        self.span = span;
        self
    }
}

impl FieldSegment {
    pub(crate) fn new(kind: FieldSegmentKindAst, span: Span) -> Self {
        Self { kind, span }
    }
}

pub(crate) fn binary_expr(op: BinaryOp, left: ExprAst, right: ExprAst) -> ExprAst {
    let span = left.span.join(right.span);
    ExprAst::new(
        ExprKindAst::Binary {
            op,
            left: Box::new(left),
            right: Box::new(right),
        },
        span,
    )
}

pub(crate) fn fold_binary_expr(head: ExprAst, tail: Vec<(BinaryOp, ExprAst)>) -> ExprAst {
    tail.into_iter()
        .fold(head, |left, (op, right)| binary_expr(op, left, right))
}

pub(crate) fn unary_expr(op: UnaryOp, expr: ExprAst, start: usize) -> ExprAst {
    let span = Span::new(start, expr.span.end);
    ExprAst::new(
        ExprKindAst::Unary {
            op,
            expr: Box::new(expr),
        },
        span,
    )
}
