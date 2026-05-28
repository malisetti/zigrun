// AST for the zigrun Zig subset: a program is a list of functions; statements
// cover let (const/var), assignment, return, if/else, and while; expressions
// cover integer literals, variable refs, function calls, and binary operators.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Bool,
    Int(IntType),
    Array { len: usize, elem: Box<Type> },
    Enum(String),
    Struct(String),
    Union(String),
    ErrorUnion {
        err_set: String,
        payload: Box<Type>,
    },
    Optional(Box<Type>),
    Void,
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
            Type::Array { elem, .. } => elem.int_type(),
            Type::Enum(_) => None,
            Type::Struct(_) => None,
            Type::Union(_) => None,
            Type::ErrorUnion { payload, .. } => payload.int_type(),
            Type::Optional(inner) => inner.int_type(),
            Type::Void => None,
        }
    }

    pub fn error_union_err_set(&self) -> Option<&str> {
        match self {
            Type::ErrorUnion { err_set, .. } => Some(err_set),
            _ => None,
        }
    }

    pub fn error_union_payload(&self) -> Option<Type> {
        match self {
            Type::ErrorUnion { payload, .. } => Some((**payload).clone()),
            _ => None,
        }
    }

    pub fn struct_name(&self) -> Option<&str> {
        match self {
            Type::Struct(name) => Some(name),
            _ => None,
        }
    }

    pub fn union_name(&self) -> Option<&str> {
        match self {
            Type::Union(name) => Some(name),
            _ => None,
        }
    }

    pub fn index_result_type(&self) -> Option<Type> {
        match self {
            Type::Array { elem, .. } => Some((**elem).clone()),
            _ => None,
        }
    }

    pub fn array_leaf_int(&self) -> Option<IntType> {
        match self {
            Type::Array { elem, .. } => match elem.as_ref() {
                Type::Int(t) => Some(*t),
                Type::Array { .. } => elem.array_leaf_int(),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn array_len(&self) -> Option<usize> {
        match self {
            Type::Array { len, .. } => Some(*len),
            _ => None,
        }
    }

    pub fn enum_name(&self) -> Option<&str> {
        match self {
            Type::Enum(name) => Some(name),
            _ => None,
        }
    }

    pub fn optional_inner(&self) -> Option<Type> {
        match self {
            Type::Optional(inner) => Some((**inner).clone()),
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
pub struct EnumVariant {
    pub name: String,
    /// Explicit `variant = N`; omitted variants get sequential values at codegen.
    pub value: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumDef {
    pub name: String,
    pub backing: Option<IntType>,
    pub variants: Vec<EnumVariant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorSetDef {
    pub name: String,
    pub variants: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<(String, Type)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnionVariant {
    pub name: String,
    pub payload: Option<Type>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnionDef {
    pub name: String,
    /// `union(enum) { ... }` uses an inline tag enum; `union(SomeEnum) { ... }` tags with `SomeEnum`.
    pub tag_enum: Option<String>,
    pub variants: Vec<UnionVariant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub enums: Vec<EnumDef>,
    pub error_sets: Vec<ErrorSetDef>,
    pub structs: Vec<StructDef>,
    pub unions: Vec<UnionDef>,
    pub functions: Vec<Function>,
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
        /// `while (cond) : (cont) { ... }` — runs after each iteration and on `continue`.
        cont: Option<Expr>,
        body: Vec<Stmt>,
    },
    Break {
        label: Option<String>,
    },
    Continue,
    ForRange {
        label: Option<String>,
        capture: Option<String>,
        start: Expr,
        end: Expr,
        body: Vec<Stmt>,
    },
    ForArray {
        label: Option<String>,
        capture: Option<String>,
        array: String,
        body: Vec<Stmt>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssignTarget {
    Name(String),
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitchTag {
    Int(u64),
    EnumVariant {
        enum_name: String,
        variant: String,
    },
    UnionVariant {
        union_name: String,
        variant: String,
        capture: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchArm {
    pub tag: SwitchTag,
    pub expr: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Int(i64),
    Bool(bool),
    Undefined,
    Var(String),
    Call { name: String, args: Vec<Expr> },
    BinOp {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Switch {
        scrutinee: Box<Expr>,
        arms: Vec<SwitchArm>,
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
        /// Present for `[N]type{ ... }` or `[_]type{ ... }`; absent for `.{ ... }`.
        annotated: Option<(Option<usize>, Type)>,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    StructLiteral {
        struct_name: String,
        fields: Vec<(String, Expr)>,
    },
    FieldAccess {
        base: Box<Expr>,
        field: String,
    },
    UnionLiteral {
        union_name: Option<String>,
        variant: String,
        value: Option<Box<Expr>>,
    },
    EmptyInit,
    Try(Box<Expr>),
    Catch {
        expr: Box<Expr>,
        fallback: Box<Expr>,
    },
    CatchReturn {
        expr: Box<Expr>,
        ret_val: Box<Expr>,
    },
    ErrorLiteral {
        err_set: String,
        variant: String,
    },
    IntFromEnum(Box<Expr>),
    Orelse {
        left: Box<Expr>,
        right: Box<Expr>,
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
