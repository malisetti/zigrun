#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub main: MainFn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MainFn {
    pub body: Stmt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Return(Expr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Int(u8),
    BinOp {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
}
