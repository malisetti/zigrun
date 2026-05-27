// AST for the zigrun Zig subset: a program is a list of functions; statements
// cover let (const/var), assignment, return, if/else, and while; expressions
// cover integer literals, variable refs, function calls, and binary operators.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntType {
    U8,
    U16,
    U32,
}

impl IntType {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "u8" => Some(IntType::U8),
            "u16" => Some(IntType::U16),
            "u32" => Some(IntType::U32),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub functions: Vec<Function>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub params: Vec<(String, IntType)>,
    pub return_type: IntType,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    /// `const`/`var` binding — mutability is not enforced in this subset.
    Let {
        name: String,
        ty: IntType,
        value: Expr,
    },
    Assign { name: String, value: Expr },
    Return(Expr),
    If {
        cond: Expr,
        then_branch: Vec<Stmt>,
        else_branch: Option<Vec<Stmt>>,
    },
    While { cond: Expr, body: Vec<Stmt> },
    Break,
    Continue,
    For {
        capture: Option<String>,
        start: Expr,
        end: Expr,
        body: Vec<Stmt>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Int(u32),
    Var(String),
    Call { name: String, args: Vec<Expr> },
    BinOp {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Switch {
        scrutinee: Box<Expr>,
        arms: Vec<(u32, Expr)>,
        default: Box<Expr>,
    },
    IntCast {
        expr: Box<Expr>,
        target: IntType,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    Ne,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}
