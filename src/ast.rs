// AST for the zigrun Zig subset: a program is a list of functions; statements
// cover let (const/var), assignment, return, if/else, and while; expressions
// cover integer literals, variable refs, function calls, and binary operators.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Bool,
    Int(IntType),
    Optional { inner: Box<Type> },
    Array { len: usize, elem: Box<Type> },
    Enum(String),
    Struct(String),
}

impl Type {
    pub fn from_name(name: &str) -> Option<Self> {
        if name == "bool" {
            return Some(Type::Bool);
        }
        IntType::from_name(name).map(Type::Int)
    }

    pub fn int_type(&self) -> Option<IntType> {
        match self {
            Type::Bool => None,
            Type::Int(t) => Some(*t),
            Type::Optional { inner } => inner.int_type(),
            Type::Array { elem, .. } => elem.int_type(),
            Type::Enum(_) => None,
            Type::Struct(_) => None,
        }
    }

    pub fn optional_inner(&self) -> Option<Type> {
        match self {
            Type::Optional { inner } => Some(inner.as_ref().clone()),
            _ => None,
        }
    }

    pub fn struct_name(&self) -> Option<&str> {
        match self {
            Type::Struct(name) => Some(name),
            _ => None,
        }
    }

    pub fn array_elem(&self) -> Option<Type> {
        match self {
            Type::Array { elem, .. } => Some(elem.as_ref().clone()),
            _ => None,
        }
    }

    pub fn scalar_int_type(&self) -> Option<IntType> {
        match self {
            Type::Int(t) => Some(*t),
            Type::Optional { inner } => inner.scalar_int_type(),
            Type::Array { elem, .. } => elem.scalar_int_type(),
            _ => None,
        }
    }

    pub fn array_len(&self) -> Option<usize> {
        match self {
            Type::Array { len, .. } => Some(*len),
            _ => None,
        }
    }

}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntType {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
}

impl IntType {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "u8" => Some(IntType::U8),
            "u16" => Some(IntType::U16),
            "u32" => Some(IntType::U32),
            "u64" => Some(IntType::U64),
            "i8" => Some(IntType::I8),
            "i16" => Some(IntType::I16),
            "i32" => Some(IntType::I32),
            "i64" => Some(IntType::I64),
            _ => None,
        }
    }

    pub fn is_signed(self) -> bool {
        matches!(self, IntType::I8 | IntType::I16 | IntType::I32 | IntType::I64)
    }

    pub fn rank(self) -> u8 {
        match self {
            IntType::U8 | IntType::I8 => 0,
            IntType::U16 | IntType::I16 => 1,
            IntType::U32 | IntType::I32 => 2,
            IntType::I64 => 3,
            IntType::U64 => 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructDecl {
    pub name: String,
    pub fields: Vec<(String, Type)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub enums: Vec<EnumDecl>,
    pub structs: Vec<StructDecl>,
    pub functions: Vec<Function>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitchCase {
    Int(u64),
    Variant(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    /// `const`/`var` binding — mutability is not enforced in this subset.
    Let {
        name: String,
        ty: Type,
        value: Expr,
    },
    Assign { target: AssignTarget, value: Expr },
    Return(Expr),
    If {
        cond: Expr,
        then_branch: Vec<Stmt>,
        else_branch: Option<Vec<Stmt>>,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
        /// Zig `while (cond) : (continue_expr)` — run after each iteration.
        continue_stmt: Option<Box<Stmt>>,
    },
    Break,
    Continue,
    ForRange {
        capture: Option<String>,
        start: Expr,
        end: Expr,
        body: Vec<Stmt>,
    },
    ForArray {
        capture: Option<String>,
        array: String,
        body: Vec<Stmt>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssignTarget {
    Name(String),
    Index { base: Box<Expr>, index: Expr },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Int(i64),
    Bool(bool),
    Var(String),
    Call { name: String, args: Vec<Expr> },
    BinOp {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Switch {
        scrutinee: Box<Expr>,
        arms: Vec<(SwitchCase, Expr)>,
        default: Option<Box<Expr>>,
    },
    EnumLiteral {
        enum_name: String,
        variant: String,
    },
    IntCast {
        expr: Box<Expr>,
        target: IntType,
    },
    Mod {
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Rem {
        left: Box<Expr>,
        right: Box<Expr>,
    },
    UnaryNeg(Box<Expr>),
    UnaryNot(Box<Expr>),
    ArrayLiteral {
        elems: Vec<Expr>,
        /// Present for `[N]type{ ... }`; absent for `.{ ... }`.
        annotated: Option<(usize, IntType)>,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Undefined,
    StructLiteral {
        struct_name: String,
        fields: Vec<(String, Expr)>,
    },
    FieldAccess {
        base: Box<Expr>,
        field: String,
    },
    Null,
    Orelse {
        opt: Box<Expr>,
        default: Box<Expr>,
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
    LogicalAnd,
    LogicalOr,
}
