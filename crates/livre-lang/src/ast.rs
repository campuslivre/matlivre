use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    MatMul,
    Mul,
    MatDiv,
    Div,
    MatLeftDiv,
    LeftDiv,
    MatPow,
    Pow,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    AndAnd,
    OrOr,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        use BinOp::*;
        match self {
            Add => "+",
            Sub => "-",
            MatMul => "*",
            Mul => ".*",
            MatDiv => "/",
            Div => "./",
            MatLeftDiv => "\\",
            LeftDiv => ".\\",
            MatPow => "^",
            Pow => ".^",
            Eq => "==",
            Ne => "~=",
            Lt => "<",
            Le => "<=",
            Gt => ">",
            Ge => ">=",
            And => "&",
            Or => "|",
            AndAnd => "&&",
            OrOr => "||",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Plus,
    Not,
    /// `'` complex-conjugate transpose (equal to `.'` for real data).
    CTranspose,
    Transpose,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Num(f64),
    /// Imaginary literal `3i`.
    Imag(f64),
    /// `name@Super`: superclass constructor (`obj@Super(...)`) or superclass method call.
    SuperRef { name: String, class: String },
    Chars(String),
    Str(String),
    Ident(String),
    /// Bare `:` inside an index.
    Colon,
    /// `end` inside an index.
    End,
    /// `~` placeholder on the left of a multi-assignment.
    Tilde,
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Range(Box<Expr>, Option<Box<Expr>>, Box<Expr>),
    /// `a(args)`: indexing or a function call.
    Index(Box<Expr>, Vec<Expr>),
    /// `a{args}`.
    CellIndex(Box<Expr>, Vec<Expr>),
    Field(Box<Expr>, String),
    DynField(Box<Expr>, Box<Expr>),
    Matrix(Vec<Vec<Expr>>),
    Cell(Vec<Vec<Expr>>),
    AnonFn(Rc<AnonFn>),
    FuncHandle(String),
}

#[derive(Debug)]
pub struct AnonFn {
    pub params: Vec<String>,
    pub body: Expr,
    /// Source text of the body for display (`@(x)x.^2`).
    pub text: String,
}

#[derive(Debug, Clone)]
pub enum Accessor {
    Paren(Vec<Expr>),
    Brace(Vec<Expr>),
    Field(String),
    DynField(Expr),
}

#[derive(Debug, Clone)]
pub enum LValue {
    Tilde,
    Target { name: String, chain: Vec<Accessor> },
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    Expr { expr: Expr, print: bool },
    Assign { lhs: Vec<LValue>, rhs: Expr, print: bool },
    Command { name: String, args: Vec<String>, print: bool },
    If { clauses: Vec<(Expr, Vec<Stmt>)>, otherwise: Option<Vec<Stmt>> },
    For { var: String, iter: Expr, body: Vec<Stmt> },
    While { cond: Expr, body: Vec<Stmt> },
    Switch { subject: Expr, cases: Vec<(Expr, Vec<Stmt>)>, otherwise: Option<Vec<Stmt>> },
    Try { body: Vec<Stmt>, catch_var: Option<String>, catch_body: Vec<Stmt> },
    Global(Vec<String>),
    Break,
    Continue,
    Return,
}

#[derive(Debug)]
pub struct Function {
    pub name: String,
    pub params: Vec<String>,
    pub outputs: Vec<String>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Default)]
pub struct Program {
    pub body: Vec<Stmt>,
    pub functions: Vec<Rc<Function>>,
}
