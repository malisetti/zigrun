// Recursive-descent parser for the zigrun Zig subset.
//
// program     := function+
// function    := "pub"? "fn" ident "(" params? ")" type block
// params      := ident ":" type ("," ident ":" type)*
// block       := "{" stmt* "}"
// stmt        := ("const"|"var") ident ":" type "=" expr ";"
//              | ident "=" expr ";"
//              | "return" expr ";"
//              | "if" "(" expr ")" block ("else" (block | "if" ...))?
//              | "while" "(" expr ")" block
//              | "for" "(" expr ".." expr ")" "|" ident "|" block
// expr        := comparison
// comparison  := additive ((<|>|<=|>=|==|!=) additive)?
// additive    := multiplicative ((+|-) multiplicative)*
// multiplicative := unary ((*|/|%) unary)*
// unary       := '-' unary | primary
// primary     := int | ident ("(" args? ")")? | "(" expr ")"
//              | "switch" "(" expr ")" "{" ( int "=>" expr "," )* "else" "=>" expr "}"
//              | "@intCast" "(" expr ")"

use crate::ast::{
    AssignTarget, BinOp, EnumDef, EnumVariant, ErrorSetDef, Expr, Function, IntType, Program, Stmt,
    StructDef, SwitchArm, SwitchStmtArm, SwitchTag, Type, UnionDef, UnionVariant,
};
use crate::lexer::{Token, TokenKind};
use std::collections::HashMap;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    return_type: Type,
    enums: HashMap<String, Vec<String>>,
    error_sets: HashMap<String, Vec<String>>,
    structs: HashMap<String, Vec<(String, Type)>>,
    unions: HashMap<String, Vec<UnionVariant>>,
    functions: HashMap<String, Type>,
    locals: HashMap<String, Type>,
    expr_enum_hint: Option<String>,
    expr_union_hint: Option<String>,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            return_type: Type::Int(IntType::U8),
            enums: HashMap::new(),
            error_sets: HashMap::new(),
            structs: HashMap::new(),
            unions: HashMap::new(),
            functions: HashMap::new(),
            locals: HashMap::new(),
            expr_enum_hint: None,
            expr_union_hint: None,
        }
    }

    pub fn parse_program(mut self) -> Result<Program, String> {
        let mut enum_defs = Vec::new();
        let mut error_set_defs = Vec::new();
        let mut struct_defs = Vec::new();
        let mut union_defs = Vec::new();
        let mut functions = Vec::new();
        while !self.check(&TokenKind::Eof) {
            if self.check(&TokenKind::Const) {
                let decl = self.parse_const_decl()?;
                match decl {
                    TopLevelDecl::Enum(e) => enum_defs.push(e),
                    TopLevelDecl::ErrorSet(e) => error_set_defs.push(e),
                    TopLevelDecl::Struct(s) => struct_defs.push(s),
                    TopLevelDecl::Union(u) => union_defs.push(u),
                }
            } else {
                functions.push(self.parse_function()?);
            }
        }
        if functions.is_empty() {
            return Err("empty program: no functions".to_string());
        }
        for f in &functions {
            self.functions.insert(f.name.clone(), f.return_type.clone());
        }
        Ok(Program {
            enums: enum_defs,
            error_sets: error_set_defs,
            structs: struct_defs,
            unions: union_defs,
            functions,
        })
    }

    fn parse_const_decl(&mut self) -> Result<TopLevelDecl, String> {
        self.expect(TokenKind::Const)?;
        let name = self.expect_ident()?;
        self.expect(TokenKind::Assign)?;
        if self.check(&TokenKind::Struct) {
            Ok(TopLevelDecl::Struct(self.parse_struct_body(name)?))
        } else if self.check(&TokenKind::Union) {
            Ok(TopLevelDecl::Union(self.parse_union_body(name)?))
        } else if self.check(&TokenKind::Error) {
            self.advance();
            Ok(TopLevelDecl::ErrorSet(self.parse_error_set_body(name)?))
        } else {
            self.expect(TokenKind::Enum)?;
            Ok(TopLevelDecl::Enum(self.parse_enum_body(name)?))
        }
    }

    fn parse_union_body(&mut self, name: String) -> Result<UnionDef, String> {
        self.expect(TokenKind::Union)?;
        self.expect(TokenKind::LParen)?;
        let tag_enum = if self.check(&TokenKind::Enum) {
            self.advance();
            None
        } else {
            let tag_name = self.expect_ident()?;
            if !self.enums.contains_key(&tag_name) {
                return Err(format!("union tag type {tag_name:?} is not a known enum"));
            }
            Some(tag_name)
        };
        self.expect(TokenKind::RParen)?;
        self.expect(TokenKind::LBrace)?;
        let mut variants = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let variant = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let payload = if self.check(&TokenKind::Void) {
                self.advance();
                None
            } else {
                Some(self.parse_type()?)
            };
            variants.push(UnionVariant { name: variant, payload });
            if self.check(&TokenKind::Comma) {
                self.advance();
            }
        }
        self.expect(TokenKind::RBrace)?;
        self.expect(TokenKind::Semicolon)?;
        if variants.is_empty() {
            return Err(format!("union {name} has no variants"));
        }
        self.unions.insert(name.clone(), variants.clone());
        Ok(UnionDef {
            name,
            tag_enum,
            variants,
        })
    }

    fn parse_error_set_body(&mut self, name: String) -> Result<ErrorSetDef, String> {
        self.expect(TokenKind::LBrace)?;
        let mut variants = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            variants.push(self.expect_ident()?);
            if self.check(&TokenKind::Comma) {
                self.advance();
            }
        }
        self.expect(TokenKind::RBrace)?;
        self.expect(TokenKind::Semicolon)?;
        if variants.is_empty() {
            return Err(format!("error set {name} has no variants"));
        }
        self.error_sets.insert(name.clone(), variants.clone());
        Ok(ErrorSetDef { name, variants })
    }

    fn parse_enum_body(&mut self, name: String) -> Result<EnumDef, String> {
        let backing = if self.check(&TokenKind::LParen) {
            self.advance();
            let ty = self.parse_type()?;
            self.expect(TokenKind::RParen)?;
            Some(
                ty.int_type()
                    .ok_or_else(|| format!("enum {name} backing must be an integer type"))?,
            )
        } else {
            None
        };
        self.expect(TokenKind::LBrace)?;
        let mut variants = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let vname = self.expect_ident()?;
            let value = if self.check(&TokenKind::Assign) {
                self.advance();
                match self.peek_kind() {
                    TokenKind::Int(n) => {
                        self.advance();
                        Some(n as i64)
                    }
                    other => {
                        return Err(format!(
                            "enum variant value must be integer literal, found {other:?}"
                        ));
                    }
                }
            } else {
                None
            };
            variants.push(EnumVariant { name: vname, value });
            if self.check(&TokenKind::Comma) {
                self.advance();
            }
        }
        self.expect(TokenKind::RBrace)?;
        self.expect(TokenKind::Semicolon)?;
        if variants.is_empty() {
            return Err(format!("enum {name} has no variants"));
        }
        let variant_names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
        self.enums.insert(name.clone(), variant_names);
        Ok(EnumDef {
            name,
            backing,
            variants,
        })
    }

    fn parse_struct_body(&mut self, name: String) -> Result<StructDef, String> {
        self.expect(TokenKind::Struct)?;
        self.expect(TokenKind::LBrace)?;
        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let field = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let ty = self.parse_type()?;
            fields.push((field, ty));
            if self.check(&TokenKind::Comma) {
                self.advance();
            }
        }
        self.expect(TokenKind::RBrace)?;
        self.expect(TokenKind::Semicolon)?;
        if fields.is_empty() {
            return Err(format!("struct {name} has no fields"));
        }
        self.structs.insert(name.clone(), fields.clone());
        Ok(StructDef { name, fields })
    }

    fn parse_function(&mut self) -> Result<Function, String> {
        if self.check(&TokenKind::Pub) {
            self.advance();
        }
        self.expect(TokenKind::Fn)?;
        let name = self.expect_ident()?;
        self.expect(TokenKind::LParen)?;
        let mut params = Vec::new();
        if !self.check(&TokenKind::RParen) {
            loop {
                let p = self.expect_ident()?;
                self.expect(TokenKind::Colon)?;
                let ty = self.parse_type()?;
                params.push((p, ty));
                if self.check(&TokenKind::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen)?;
        let return_type = self.parse_type()?;
        self.return_type = return_type.clone();
        self.locals.clear();
        for (p, ty) in &params {
            self.locals.insert(p.clone(), ty.clone());
        }
        let body = self.parse_block()?;
        Ok(Function {
            name,
            params,
            return_type,
            body,
        })
    }

    fn parse_type(&mut self) -> Result<Type, String> {
        if self.check(&TokenKind::Question) {
            self.advance();
            return Ok(Type::Optional(Box::new(self.parse_type()?)));
        }
        if self.check(&TokenKind::Void) {
            self.advance();
            return Ok(Type::Void);
        }
        if self.check(&TokenKind::LBracket) {
            self.advance();
            let len = match self.peek_kind() {
                TokenKind::Int(n) => {
                    self.advance();
                    n as usize
                }
                other => return Err(format!("expected array length, found {other:?}")),
            };
            self.expect(TokenKind::RBracket)?;
            let elem = Box::new(self.parse_type()?);
            return Ok(Type::Array { len, elem });
        }
        let name = match self.peek_kind() {
            TokenKind::Bool => {
                self.advance();
                return Ok(Type::Bool);
            }
            TokenKind::Ident(name) => {
                self.advance();
                name
            }
            other => return Err(format!("expected a type, found {other:?}")),
        };
        if self.error_sets.contains_key(&name) {
            if self.check(&TokenKind::Bang) {
                self.advance();
                let payload = Box::new(self.parse_type()?);
                return Ok(Type::ErrorUnion {
                    err_set: name,
                    payload,
                });
            }
            return Err(format!("error set {name:?} requires `!payload` type"));
        }
        if self.enums.contains_key(&name) {
            return Ok(Type::Enum(name));
        }
        if self.structs.contains_key(&name) {
            return Ok(Type::Struct(name));
        }
        if self.unions.contains_key(&name) {
            return Ok(Type::Union(name));
        }
        Type::from_name(&name).ok_or_else(|| format!("unsupported type {name:?}"))
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, String> {
        self.expect(TokenKind::LBrace)?;
        let mut stmts = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            stmts.push(self.parse_stmt()?);
        }
        self.expect(TokenKind::RBrace)?;
        Ok(stmts)
    }

    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        match self.peek_kind() {
            TokenKind::Const | TokenKind::Var => {
                self.advance();
                let name = self.expect_ident()?;
                let ty = if self.check(&TokenKind::Colon) {
                    self.advance();
                    Some(self.parse_type()?)
                } else {
                    None
                };
                self.expect(TokenKind::Assign)?;
                if let Some(Type::Enum(enum_name)) = &ty {
                    self.expr_enum_hint = Some(enum_name.clone());
                }
                if let Some(Type::Union(union_name)) = &ty {
                    self.expr_union_hint = Some(union_name.clone());
                }
                let value = self.parse_expr()?;
                self.expr_enum_hint = None;
                self.expr_union_hint = None;
                let ty = ty.unwrap_or_else(|| {
                    infer_expr_type(
                        &value,
                        &self.enums,
                        &self.structs,
                        &self.unions,
                        &self.locals,
                        &self.functions,
                    )
                });
                self.expect(TokenKind::Semicolon)?;
                self.locals.insert(name.clone(), ty.clone());
                Ok(Stmt::Let { name, ty, value })
            }
            TokenKind::Return => {
                self.advance();
                let value = self.parse_expr()?;
                self.expect(TokenKind::Semicolon)?;
                Ok(Stmt::Return(value))
            }
            TokenKind::If => {
                self.advance();
                self.parse_if_stmt()
            }
            TokenKind::While => {
                self.advance();
                self.expect(TokenKind::LParen)?;
                let cond = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                let cont = if self.check(&TokenKind::Colon) {
                    self.advance();
                    self.expect(TokenKind::LParen)?;
                    let e = self.parse_while_cont_expr()?;
                    self.expect(TokenKind::RParen)?;
                    Some(e)
                } else {
                    None
                };
                let body = self.parse_block()?;
                Ok(Stmt::While { cond, cont, body })
            }
            TokenKind::Break => {
                self.advance();
                let label = if self.check(&TokenKind::Colon) {
                    self.advance();
                    Some(self.expect_ident()?)
                } else {
                    None
                };
                self.expect(TokenKind::Semicolon)?;
                Ok(Stmt::Break { label })
            }
            TokenKind::Continue => {
                self.advance();
                self.expect(TokenKind::Semicolon)?;
                Ok(Stmt::Continue)
            }
            TokenKind::For => self.parse_for_stmt(None),
            TokenKind::Switch => self.parse_switch_stmt(),
            TokenKind::Ident(name) if self.check_next(&TokenKind::Colon) => {
                let label = name;
                self.advance();
                self.advance();
                match self.peek_kind() {
                    TokenKind::For => self.parse_for_stmt(Some(label)),
                    other => Err(format!(
                        "loop label must be followed by for, found {other:?}"
                    )),
                }
            }
            TokenKind::Ident(name) => self.parse_assign_stmt(name),
            other => Err(format!("unexpected token at start of statement: {other:?}")),
        }
    }

    fn parse_for_stmt(&mut self, label: Option<String>) -> Result<Stmt, String> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let first = self.parse_expr()?;
        if self.check(&TokenKind::DotDot) {
            self.advance();
            let end = self.parse_expr()?;
            self.expect(TokenKind::RParen)?;
            let capture = self.parse_for_capture()?;
            let body = self.parse_for_body()?;
            Ok(Stmt::ForRange {
                label,
                capture,
                start: first,
                end,
                body,
            })
        } else {
            let array = match first {
                Expr::Var(name) => name,
                other => {
                    return Err(format!(
                        "expected array identifier in for-loop, found {other:?}"
                    ))
                }
            };
            self.expect(TokenKind::RParen)?;
            let capture = self.parse_for_capture()?;
            let body = self.parse_for_body()?;
            Ok(Stmt::ForArray {
                label,
                capture,
                array,
                body,
            })
        }
    }

    fn parse_for_body(&mut self) -> Result<Vec<Stmt>, String> {
        if self.check(&TokenKind::LBrace) {
            self.parse_block()
        } else {
            Ok(vec![self.parse_stmt()?])
        }
    }

    fn parse_for_capture(&mut self) -> Result<Option<String>, String> {
        self.expect(TokenKind::Pipe)?;
        let capture = match self.peek_kind() {
            TokenKind::Ident(name) if name == "_" => {
                self.advance();
                None
            }
            TokenKind::Ident(name) => {
                self.advance();
                Some(name)
            }
            other => {
                return Err(format!("expected capture identifier or '_', found {other:?}"))
            }
        };
        self.expect(TokenKind::Pipe)?;
        Ok(capture)
    }

    fn parse_assign_stmt(&mut self, name: String) -> Result<Stmt, String> {
        self.advance();
        let mut target_expr = Expr::Var(name);
        while self.check(&TokenKind::LBracket) {
            self.advance();
            let index = self.parse_expr()?;
            self.expect(TokenKind::RBracket)?;
            target_expr = Expr::Index {
                base: Box::new(target_expr),
                index: Box::new(index),
            };
        }
        let target = match target_expr {
            Expr::Var(name) => AssignTarget::Name(name),
            Expr::Index { base, index } => AssignTarget::Index { base, index },
            other => {
                return Err(format!(
                    "expected variable or indexed lvalue, found {other:?}"
                ))
            }
        };
        let (op, is_compound) = match self.peek_kind() {
            TokenKind::Assign => {
                self.advance();
                (None, false)
            }
            TokenKind::PlusAssign => {
                self.advance();
                (Some(BinOp::Add), true)
            }
            TokenKind::MinusAssign => {
                self.advance();
                (Some(BinOp::Sub), true)
            }
            TokenKind::StarAssign => {
                self.advance();
                (Some(BinOp::Mul), true)
            }
            TokenKind::SlashAssign => {
                self.advance();
                (Some(BinOp::Div), true)
            }
            TokenKind::PercentAssign => {
                self.advance();
                (Some(BinOp::Mod), true)
            }
            TokenKind::AmpAssign => {
                self.advance();
                (Some(BinOp::BitAnd), true)
            }
            TokenKind::PipeAssign => {
                self.advance();
                (Some(BinOp::BitOr), true)
            }
            TokenKind::CaretAssign => {
                self.advance();
                (Some(BinOp::BitXor), true)
            }
            TokenKind::ShlAssign => {
                self.advance();
                (Some(BinOp::Shl), true)
            }
            TokenKind::ShrAssign => {
                self.advance();
                (Some(BinOp::Shr), true)
            }
            other => return Err(format!("expected assignment operator, found {other:?}")),
        };
        let rhs = self.parse_expr()?;
        self.expect(TokenKind::Semicolon)?;
        let value = if is_compound {
            let left = match &target {
                AssignTarget::Name(name) => Expr::Var(name.clone()),
                AssignTarget::Index { base, index } => Expr::Index {
                    base: base.clone(),
                    index: index.clone(),
                },
            };
            Expr::BinOp {
                op: op.unwrap(),
                left: Box::new(left),
                right: Box::new(rhs),
            }
        } else {
            rhs
        };
        Ok(Stmt::Assign { target, value })
    }

    /// `while (cond) : (i += 1)` — compound assignment in the continue slot.
    fn parse_while_cont_expr(&mut self) -> Result<Expr, String> {
        if let TokenKind::Ident(name) = self.peek_kind().clone() {
            self.advance();
            let compound = match self.peek_kind() {
                TokenKind::PlusAssign => Some(BinOp::Add),
                TokenKind::MinusAssign => Some(BinOp::Sub),
                TokenKind::StarAssign => Some(BinOp::Mul),
                TokenKind::SlashAssign => Some(BinOp::Div),
                TokenKind::PercentAssign => Some(BinOp::Mod),
                TokenKind::AmpAssign => Some(BinOp::BitAnd),
                TokenKind::PipeAssign => Some(BinOp::BitOr),
                TokenKind::CaretAssign => Some(BinOp::BitXor),
                TokenKind::ShlAssign => Some(BinOp::Shl),
                TokenKind::ShrAssign => Some(BinOp::Shr),
                _ => None,
            };
            if let Some(op) = compound {
                self.advance();
                let rhs = self.parse_expr()?;
                return Ok(Expr::BinOp {
                    op,
                    left: Box::new(Expr::Var(name)),
                    right: Box::new(rhs),
                });
            }
            return Err(format!(
                "while continue expression: expected compound assignment after `{name}`"
            ));
        }
        self.parse_expr()
    }

    fn parse_if_branch(&mut self) -> Result<Vec<Stmt>, String> {
        if self.check(&TokenKind::LBrace) {
            self.parse_block()
        } else {
            Ok(vec![self.parse_stmt()?])
        }
    }

    fn parse_if_stmt(&mut self) -> Result<Stmt, String> {
        self.expect(TokenKind::LParen)?;
        let cond = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        let then_branch = self.parse_if_branch()?;
        let else_branch = if self.check(&TokenKind::Else) {
            self.advance();
            if self.check(&TokenKind::If) {
                self.advance();
                Some(vec![self.parse_if_stmt()?])
            } else {
                Some(self.parse_if_branch()?)
            }
        } else {
            None
        };
        Ok(Stmt::If {
            cond,
            then_branch,
            else_branch,
        })
    }

    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_logical_or()
    }

    fn parse_logical_or(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_logical_and()?;
        while self.check(&TokenKind::Or) {
            self.advance();
            let right = self.parse_logical_and()?;
            left = Expr::BinOp {
                op: BinOp::LogicalOr,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_logical_and(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_logical_not()?;
        while self.check(&TokenKind::And) {
            self.advance();
            let right = self.parse_logical_not()?;
            left = Expr::BinOp {
                op: BinOp::LogicalAnd,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_logical_not(&mut self) -> Result<Expr, String> {
        if self.check(&TokenKind::Bang) {
            self.advance();
            let operand = self.parse_logical_not()?;
            return Ok(Expr::UnaryNot(Box::new(operand)));
        }
        self.parse_bitwise()
    }

    fn parse_bitwise(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_comparison()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Amp => BinOp::BitAnd,
                TokenKind::Caret => BinOp::BitXor,
                TokenKind::Pipe => BinOp::BitOr,
                _ => break,
            };
            self.advance();
            let right = self.parse_comparison()?;
            left = Expr::BinOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr, String> {
        let left = self.parse_shift()?;
        let op = match self.peek_kind() {
            TokenKind::Lt => BinOp::Lt,
            TokenKind::Gt => BinOp::Gt,
            TokenKind::Le => BinOp::Le,
            TokenKind::Ge => BinOp::Ge,
            TokenKind::EqEq => BinOp::Eq,
            TokenKind::Ne => BinOp::Ne,
            _ => return Ok(left),
        };
        self.advance();
        let right = self.parse_shift()?;
        Ok(Expr::BinOp {
            op,
            left: Box::new(left),
            right: Box::new(right),
        })
    }

    fn parse_shift(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_additive()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Shl => BinOp::Shl,
                TokenKind::Shr => BinOp::Shr,
                _ => break,
            };
            self.advance();
            let right = self.parse_additive()?;
            left = Expr::BinOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative()?;
            left = Expr::BinOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            left = Expr::BinOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        if self.check(&TokenKind::Try) {
            self.advance();
            let operand = self.parse_unary()?;
            return Ok(Expr::Try(Box::new(operand)));
        }
        if self.check(&TokenKind::Minus) {
            self.advance();
            let operand = self.parse_unary()?;
            return Ok(Expr::UnaryNeg(Box::new(operand)));
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.check(&TokenKind::Dot) {
                self.advance();
                let field = self.expect_ident()?;
                if let Expr::Var(err_set) = &expr {
                    if self.error_sets.contains_key(err_set) {
                        expr = Expr::ErrorLiteral {
                            err_set: err_set.clone(),
                            variant: field,
                        };
                        continue;
                    }
                    if self.enums.contains_key(err_set) {
                        expr = Expr::EnumLiteral {
                            enum_name: err_set.clone(),
                            variant: field,
                        };
                        continue;
                    }
                }
                expr = Expr::FieldAccess {
                    base: Box::new(expr),
                    field,
                };
            } else if self.check(&TokenKind::LBracket) {
                self.advance();
                let index = self.parse_expr()?;
                self.expect(TokenKind::RBracket)?;
                expr = Expr::Index {
                    base: Box::new(expr),
                    index: Box::new(index),
                };
            } else {
                break;
            }
        }
        if self.check(&TokenKind::Catch) {
            self.advance();
            if self.check(&TokenKind::Return) {
                self.advance();
                let ret_val = self.parse_unary()?;
                expr = Expr::CatchReturn {
                    expr: Box::new(expr),
                    ret_val: Box::new(ret_val),
                };
            } else {
                let fallback = self.parse_unary()?;
                expr = Expr::Catch {
                    expr: Box::new(expr),
                    fallback: Box::new(fallback),
                };
            }
        }
        if self.check(&TokenKind::Orelse) {
            self.advance();
            let fallback = self.parse_unary()?;
            expr = Expr::Orelse {
                left: Box::new(expr),
                right: Box::new(fallback),
            };
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.peek_kind() {
            TokenKind::LBracket => self.parse_typed_array_literal(),
            TokenKind::Dot => {
                self.advance();
                if self.check(&TokenKind::LBrace) {
                    self.advance();
                    if self.check(&TokenKind::Dot) {
                        let union_lit = self.parse_anon_union_literal()?;
                        self.expect(TokenKind::RBrace)?;
                        Ok(union_lit)
                    } else {
                        let elems = self.parse_array_elems()?;
                        self.expect(TokenKind::RBrace)?;
                        Ok(Expr::ArrayLiteral {
                            elems,
                            annotated: None,
                        })
                    }
                } else {
                    let variant = self.expect_ident()?;
                    let enum_name = self
                        .expr_enum_hint
                        .clone()
                        .ok_or_else(|| format!("enum literal .{variant} requires type context"))?;
                    Ok(Expr::EnumLiteral { enum_name, variant })
                }
            }
            TokenKind::At => {
                self.advance();
                let builtin = self.expect_ident()?;
                self.expect(TokenKind::LParen)?;
                match builtin.as_str() {
                    "intCast" => {
                        let expr = self.parse_expr()?;
                        self.expect(TokenKind::RParen)?;
                        let target = self.return_type.int_type().ok_or_else(|| {
                            "@intCast target must be an integer type".to_string()
                        })?;
                        Ok(Expr::IntCast {
                            expr: Box::new(expr),
                            target,
                        })
                    }
                    "mod" | "rem" => {
                        let left = self.parse_expr()?;
                        self.expect(TokenKind::Comma)?;
                        let right = self.parse_expr()?;
                        self.expect(TokenKind::RParen)?;
                        if builtin == "mod" {
                            Ok(Expr::Mod {
                                left: Box::new(left),
                                right: Box::new(right),
                            })
                        } else {
                            Ok(Expr::Rem {
                                left: Box::new(left),
                                right: Box::new(right),
                            })
                        }
                    }
                    "intFromEnum" => {
                        let expr = self.parse_expr()?;
                        self.expect(TokenKind::RParen)?;
                        Ok(Expr::IntFromEnum(Box::new(expr)))
                    }
                    other => Err(format!("unsupported builtin @{other}")),
                }
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::Bool(true))
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::Bool(false))
            }
            TokenKind::Undefined => {
                self.advance();
                Ok(Expr::Undefined)
            }
            TokenKind::Int(n) => {
                self.advance();
                Ok(Expr::Int(n as i64))
            }
            TokenKind::If => {
                self.advance();
                self.expect(TokenKind::LParen)?;
                let cond = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                let then_expr = self.parse_expr()?;
                self.expect(TokenKind::Else)?;
                let else_expr = self.parse_expr()?;
                Ok(Expr::If {
                    cond: Box::new(cond),
                    then_expr: Box::new(then_expr),
                    else_expr: Box::new(else_expr),
                })
            }
            TokenKind::Switch => self.parse_switch(),
            TokenKind::LParen => {
                self.advance();
                let e = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                Ok(e)
            }
            TokenKind::Ident(name) => {
                self.advance();
                if self.check(&TokenKind::LBrace) {
                    if self.structs.contains_key(&name) {
                        self.advance();
                        let fields = self.parse_struct_literal_fields()?;
                        self.expect(TokenKind::RBrace)?;
                        Ok(Expr::StructLiteral {
                            struct_name: name,
                            fields,
                        })
                    } else if self.unions.contains_key(&name) {
                        self.advance();
                        let lit = self.parse_union_literal_fields(&name)?;
                        self.expect(TokenKind::RBrace)?;
                        Ok(lit)
                    } else {
                        return Err(format!("unknown struct or union type {name:?}"));
                    }
                } else if self.check(&TokenKind::LParen) {
                    self.advance();
                    let mut args = Vec::new();
                    if !self.check(&TokenKind::RParen) {
                        loop {
                            args.push(self.parse_expr()?);
                            if self.check(&TokenKind::Comma) {
                                self.advance();
                            } else {
                                break;
                            }
                        }
                    }
                    self.expect(TokenKind::RParen)?;
                    Ok(Expr::Call { name, args })
                } else {
                    Ok(Expr::Var(name))
                }
            }
            other => Err(format!("expected an expression, found {other:?}")),
        }
    }

    fn parse_typed_array_literal(&mut self) -> Result<Expr, String> {
        self.advance();
        let len = match self.peek_kind() {
            TokenKind::Int(n) => {
                self.advance();
                Some(n as usize)
            }
            TokenKind::Ident(name) if name == "_" => {
                self.advance();
                None
            }
            other => return Err(format!("expected array length or '_', found {other:?}")),
        };
        self.expect(TokenKind::RBracket)?;
        let elem_ty = self.parse_type()?;
        if let Type::Union(union_name) = &elem_ty {
            self.expr_union_hint = Some(union_name.clone());
        }
        self.expect(TokenKind::LBrace)?;
        let elems = self.parse_array_elems()?;
        self.expr_union_hint = None;
        self.expect(TokenKind::RBrace)?;
        Ok(Expr::ArrayLiteral {
            elems,
            annotated: Some((len, elem_ty)),
        })
    }

    fn parse_anon_union_literal(&mut self) -> Result<Expr, String> {
        let fields = self.parse_struct_literal_fields()?;
        if fields.len() != 1 {
            return Err("union literal must have exactly one variant field".to_string());
        }
        let (variant, value) = fields.into_iter().next().unwrap();
        let union_name = self.expr_union_hint.clone();
        Ok(Expr::UnionLiteral {
            union_name,
            variant,
            value: if matches!(value, Expr::EmptyInit) {
                None
            } else {
                Some(Box::new(value))
            },
        })
    }

    fn parse_union_literal_fields(&mut self, union_name: &str) -> Result<Expr, String> {
        let fields = self.parse_struct_literal_fields()?;
        if fields.len() != 1 {
            return Err("union literal must have exactly one variant field".to_string());
        }
        let (variant, value) = fields.into_iter().next().unwrap();
        if !self
            .unions
            .get(union_name)
            .is_some_and(|vs| vs.iter().any(|v| v.name == variant))
        {
            return Err(format!(
                "unknown variant {variant:?} for union {union_name:?}"
            ));
        }
        Ok(Expr::UnionLiteral {
            union_name: Some(union_name.to_string()),
            variant,
            value: if matches!(value, Expr::EmptyInit) {
                None
            } else {
                Some(Box::new(value))
            },
        })
    }

    fn parse_struct_literal_fields(&mut self) -> Result<Vec<(String, Expr)>, String> {
        let mut fields = Vec::new();
        if !self.check(&TokenKind::RBrace) {
            loop {
                self.expect(TokenKind::Dot)?;
                let field = self.expect_ident()?;
                self.expect(TokenKind::Assign)?;
                let value = if self.check(&TokenKind::LBrace) {
                    let saved = self.pos;
                    self.advance();
                    if self.check(&TokenKind::RBrace) {
                        self.advance();
                        Expr::EmptyInit
                    } else {
                        self.pos = saved;
                        self.parse_expr()?
                    }
                } else {
                    self.parse_expr()?
                };
                fields.push((field, value));
                if self.check(&TokenKind::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        Ok(fields)
    }

    fn parse_array_elems(&mut self) -> Result<Vec<Expr>, String> {
        let mut elems = Vec::new();
        if !self.check(&TokenKind::RBrace) {
            loop {
                elems.push(self.parse_expr()?);
                if self.check(&TokenKind::Comma) {
                    self.advance();
                    if self.check(&TokenKind::RBrace) {
                        break;
                    }
                } else {
                    break;
                }
            }
        }
        Ok(elems)
    }

    fn parse_switch(&mut self) -> Result<Expr, String> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let scrutinee = self.parse_expr()?;
        let scrutinee_enum = self.infer_enum_type(&scrutinee);
        let scrutinee_union = self.infer_union_type(&scrutinee);
        self.expect(TokenKind::RParen)?;
        self.expect(TokenKind::LBrace)?;
        let mut arms = Vec::new();
        let mut default = None;
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            if self.check(&TokenKind::Else) {
                if scrutinee_enum.is_some() || scrutinee_union.is_some() {
                    return Err("enum/union switch cannot have `else` arm".to_string());
                }
                self.advance();
                self.expect(TokenKind::FatArrow)?;
                default = Some(Box::new(self.parse_expr()?));
            } else if self.check(&TokenKind::Dot) {
                if let Some(union_name) = scrutinee_union.clone() {
                    self.advance();
                    let variant = self.expect_ident()?;
                    if !self.unions.get(&union_name).is_some_and(|vs| {
                        vs.iter().any(|v| v.name == variant)
                    }) {
                        return Err(format!(
                            "unknown variant {variant:?} for union {union_name:?}"
                        ));
                    }
                    self.expect(TokenKind::FatArrow)?;
                    let capture = self.parse_switch_capture()?;
                    arms.push(SwitchArm {
                        tag: SwitchTag::UnionVariant {
                            union_name: union_name.clone(),
                            variant,
                            capture,
                        },
                        expr: self.parse_expr()?,
                    });
                } else {
                    let enum_name = scrutinee_enum.clone().ok_or_else(|| {
                        "tagged switch arm requires enum or union scrutinee".to_string()
                    })?;
                    self.advance();
                    let variant = self.expect_ident()?;
                    if !self
                        .enums
                        .get(&enum_name)
                        .is_some_and(|vs| vs.iter().any(|v| v == &variant))
                    {
                        return Err(format!(
                            "unknown variant {variant:?} for enum {enum_name:?}"
                        ));
                    }
                    self.expect(TokenKind::FatArrow)?;
                    arms.push(SwitchArm {
                        tag: SwitchTag::EnumVariant {
                            enum_name: enum_name.clone(),
                            variant,
                        },
                        expr: self.parse_expr()?,
                    });
                }
            } else {
                if scrutinee_enum.is_some() || scrutinee_union.is_some() {
                    return Err("enum/union switch requires `.variant =>` arms".to_string());
                }
                let val = match self.peek_kind() {
                    TokenKind::Int(n) => {
                        self.advance();
                        n
                    }
                    other => {
                        return Err(format!(
                            "expected integer literal in switch arm, found {other:?}"
                        ));
                    }
                };
                self.expect(TokenKind::FatArrow)?;
                arms.push(SwitchArm {
                    tag: SwitchTag::Int(val),
                    expr: self.parse_expr()?,
                });
            }
            if self.check(&TokenKind::Comma) {
                self.advance();
            }
        }
        self.expect(TokenKind::RBrace)?;
        if scrutinee_enum.is_none() && scrutinee_union.is_none() && default.is_none() {
            return Err("switch missing `else =>` arm".to_string());
        }
        Ok(Expr::Switch {
            scrutinee: Box::new(scrutinee),
            arms,
            default,
        })
    }

    fn parse_switch_stmt(&mut self) -> Result<Stmt, String> {
        self.advance();
        self.expect(TokenKind::LParen)?;
        let scrutinee = self.parse_expr()?;
        let scrutinee_union = self.infer_union_type(&scrutinee);
        let scrutinee_enum = self.infer_enum_type(&scrutinee);
        self.expect(TokenKind::RParen)?;
        self.expect(TokenKind::LBrace)?;
        let mut arms = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let tag = if self.check(&TokenKind::Dot) {
                self.advance();
                let variant = self.expect_ident()?;
                if let Some(ref union_name) = scrutinee_union {
                    if !self.unions.get(union_name).is_some_and(|vs| {
                        vs.iter().any(|v| v.name == variant)
                    }) {
                        return Err(format!(
                            "unknown variant {variant:?} for union {union_name:?}"
                        ));
                    }
                    SwitchTag::UnionVariant {
                        union_name: union_name.clone(),
                        variant,
                        capture: None,
                    }
                } else if let Some(ref enum_name) = scrutinee_enum {
                    if !self
                        .enums
                        .get(enum_name)
                        .is_some_and(|vs| vs.iter().any(|v| v == &variant))
                    {
                        return Err(format!(
                            "unknown variant {variant:?} for enum {enum_name:?}"
                        ));
                    }
                    SwitchTag::EnumVariant {
                        enum_name: enum_name.clone(),
                        variant,
                    }
                } else {
                    return Err("dot arm requires enum or union scrutinee".to_string());
                }
            } else {
                let val = match self.peek_kind() {
                    TokenKind::Int(n) => { self.advance(); n }
                    other => return Err(format!("expected integer literal in switch arm, found {other:?}")),
                };
                SwitchTag::Int(val)
            };
            self.expect(TokenKind::FatArrow)?;
            let tag = match tag {
                SwitchTag::UnionVariant {
                    union_name,
                    variant,
                    ..
                } => SwitchTag::UnionVariant {
                    union_name,
                    variant,
                    capture: self.parse_switch_capture()?,
                },
                other => other,
            };
            let body = if self.check(&TokenKind::LBrace) {
                self.parse_block()?
            } else {
                vec![self.parse_switch_arm_stmt()?]
            };
            arms.push(SwitchStmtArm { tag, body });
            if self.check(&TokenKind::Comma) {
                self.advance();
            }
        }
        self.expect(TokenKind::RBrace)?;
        Ok(Stmt::Switch { scrutinee, arms })
    }

    fn parse_switch_arm_stmt(&mut self) -> Result<Stmt, String> {
        match self.peek_kind() {
            TokenKind::Return => {
                self.advance();
                let expr = self.parse_expr()?;
                Ok(Stmt::Return(expr))
            }
            TokenKind::If => {
                self.advance();
                self.parse_if_stmt()
            }
            other => Err(format!("expected statement in switch arm, found {other:?}")),
        }
    }

    fn infer_enum_type(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Var(name) => self
                .locals
                .get(name)
                .and_then(|t| t.enum_name().map(str::to_string)),
            Expr::EnumLiteral { enum_name, .. } => Some(enum_name.clone()),
            _ => None,
        }
    }

    fn infer_union_type(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Var(name) => self
                .locals
                .get(name)
                .and_then(|t| t.union_name().map(str::to_string)),
            _ => None,
        }
    }

    fn parse_switch_capture(&mut self) -> Result<Option<String>, String> {
        if !self.check(&TokenKind::Pipe) {
            return Ok(None);
        }
        self.advance();
        let name = self.expect_ident()?;
        self.expect(TokenKind::Pipe)?;
        Ok(Some(name))
    }

    fn expect_ident(&mut self) -> Result<String, String> {
        match self.peek_kind() {
            TokenKind::Ident(name) => {
                self.advance();
                Ok(name)
            }
            other => Err(format!("expected an identifier, found {other:?}")),
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Result<(), String> {
        if self.check(&kind) {
            self.advance();
            Ok(())
        } else {
            Err(format!("expected {:?}, found {:?}", kind, self.peek_kind()))
        }
    }

    fn check(&self, kind: &TokenKind) -> bool {
        &self.peek_kind() == kind
    }

    fn check_next(&self, kind: &TokenKind) -> bool {
        self.tokens
            .get(self.pos + 1)
            .is_some_and(|t| &t.kind == kind)
    }

    fn peek_kind(&self) -> TokenKind {
        self.tokens
            .get(self.pos)
            .map(|t| t.kind.clone())
            .unwrap_or(TokenKind::Eof)
    }

    fn advance(&mut self) {
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
    }
}

fn infer_expr_type(
    expr: &Expr,
    enums: &HashMap<String, Vec<String>>,
    structs: &HashMap<String, Vec<(String, Type)>>,
    unions: &HashMap<String, Vec<UnionVariant>>,
    locals: &HashMap<String, Type>,
    functions: &HashMap<String, Type>,
) -> Type {
    match expr {
        Expr::Int(_) => Type::Int(IntType::U8),
        Expr::Bool(_) => Type::Bool,
        Expr::Undefined => Type::Int(IntType::U8),
        Expr::Var(name) => locals
            .get(name)
            .cloned()
            .unwrap_or(Type::Int(IntType::U8)),
        Expr::Call { name, .. } => functions
            .get(name)
            .cloned()
            .unwrap_or(Type::Int(IntType::U8)),
        Expr::BinOp { op, left, right } => match op {
            BinOp::LogicalAnd | BinOp::LogicalOr => Type::Bool,
            _ => {
                let lt = infer_expr_type(left, enums, structs, unions, locals, functions);
                let rt = infer_expr_type(right, enums, structs, unions, locals, functions);
                match (lt, rt) {
                    (Type::Int(a), Type::Int(b)) => Type::Int(wider_int_type(a, b)),
                    (Type::Int(a), _) => Type::Int(a),
                    (_, Type::Int(b)) => Type::Int(b),
                    _ => Type::Bool,
                }
            }
        },
        Expr::If {
            then_expr,
            else_expr,
            ..
        } => {
            let lt = infer_expr_type(then_expr, enums, structs, unions, locals, functions);
            let rt = infer_expr_type(else_expr, enums, structs, unions, locals, functions);
            match (lt, rt) {
                (Type::Int(a), Type::Int(b)) => Type::Int(wider_int_type(a, b)),
                (Type::Int(a), _) => Type::Int(a),
                (_, Type::Int(b)) => Type::Int(b),
                (other, _) => other,
            }
        }
        Expr::Switch { default, .. } => default
            .as_ref()
            .map(|d| infer_expr_type(d, enums, structs, unions, locals, functions))
            .unwrap_or(Type::Int(IntType::U8)),
        Expr::EnumLiteral { enum_name, .. } => Type::Enum(enum_name.clone()),
        Expr::IntFromEnum(inner) => infer_expr_type(inner, enums, structs, unions, locals, functions)
            .enum_name()
            .map(|_| Type::Int(IntType::U8))
            .unwrap_or(Type::Int(IntType::U8)),
        Expr::ErrorLiteral { .. } => Type::Int(IntType::U8),
        Expr::Try(inner) => infer_expr_type(inner, enums, structs, unions, locals, functions)
            .error_union_payload()
            .unwrap_or(Type::Int(IntType::U8)),
        Expr::Catch { expr, .. } | Expr::CatchReturn { expr, .. } => infer_expr_type(
            expr,
            enums,
            structs,
            unions,
            locals,
            functions,
        )
        .error_union_payload()
        .unwrap_or(Type::Int(IntType::U8)),
        Expr::IntCast { target, .. } => Type::Int(*target),
        Expr::Mod { left, right } | Expr::Rem { left, right } => {
            let lt = infer_expr_type(left, enums, structs, unions, locals, functions);
            let rt = infer_expr_type(right, enums, structs, unions, locals, functions);
            match (lt, rt) {
                (Type::Int(a), Type::Int(b)) => Type::Int(wider_int_type(a, b)),
                (Type::Int(a), _) => Type::Int(a),
                (_, Type::Int(b)) => Type::Int(b),
                _ => Type::Int(IntType::U8),
            }
        }
        Expr::UnaryNeg(inner) => infer_expr_type(inner, enums, structs, unions, locals, functions),
        Expr::UnaryNot(_) => Type::Bool,
        Expr::ArrayLiteral { elems, annotated } => {
            if let Some((len_opt, elem)) = annotated {
                Type::Array {
                    len: len_opt.unwrap_or(elems.len()),
                    elem: Box::new(elem.clone()),
                }
            } else if let Some(first) = elems.first() {
                let elem = infer_expr_type(first, enums, structs, unions, locals, functions);
                Type::Array {
                    len: elems.len(),
                    elem: Box::new(elem),
                }
            } else {
                Type::Array {
                    len: 0,
                    elem: Box::new(Type::Int(IntType::U8)),
                }
            }
        }
        Expr::Index { base, .. } => infer_expr_type(base, enums, structs, unions, locals, functions)
            .index_result_type()
            .unwrap_or(Type::Int(IntType::U8)),
        Expr::StructLiteral { struct_name, .. } => Type::Struct(struct_name.clone()),
        Expr::UnionLiteral { union_name, .. } => Type::Union(
            union_name
                .clone()
                .expect("union literal requires type context"),
        ),
        Expr::EmptyInit => Type::Void,
        Expr::FieldAccess { base, field } => field_type(base, field, enums, structs, unions, locals),
        Expr::Orelse { right, .. } => infer_expr_type(right, enums, structs, unions, locals, functions),
    }
}

fn field_type(
    base: &Expr,
    field: &str,
    _enums: &HashMap<String, Vec<String>>,
    structs: &HashMap<String, Vec<(String, Type)>>,
    _unions: &HashMap<String, Vec<UnionVariant>>,
    locals: &HashMap<String, Type>,
) -> Type {
    let base_ty = match base {
        Expr::Var(name) => {
            if _enums.contains_key(name) {
                return Type::Enum(name.clone());
            }
            locals.get(name).cloned()
        }
        Expr::FieldAccess {
            base,
            field: parent_field,
        } => Some(field_type(base, parent_field, _enums, structs, _unions, locals)),
        Expr::StructLiteral { struct_name, .. } => Some(Type::Struct(struct_name.clone())),
        _ => None,
    };
    base_ty
        .and_then(|t| t.struct_name().map(str::to_string))
        .and_then(|sn| structs.get(&sn))
        .and_then(|fields| {
            fields
                .iter()
                .find(|(f, _)| f == field)
                .map(|(_, ty)| ty.clone())
        })
        .unwrap_or(Type::Int(IntType::U8))
}

enum TopLevelDecl {
    Enum(EnumDef),
    ErrorSet(ErrorSetDef),
    Struct(StructDef),
    Union(UnionDef),
}

fn wider_int_type(a: IntType, b: IntType) -> IntType {
    let ra = a.rank();
    let rb = b.rank();
    if ra > rb {
        a
    } else if rb > ra {
        b
    } else if a.is_signed() {
        a
    } else if b.is_signed() {
        b
    } else {
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    #[test]
    fn parses_recursive_function() {
        let src = "fn fib(n: u8) u8 { if (n < 2) { return n; } return fib(n - 1) + fib(n - 2); } pub fn main() u8 { return fib(10); }";
        let toks = Lexer::new(src).tokenize().unwrap();
        let prog = Parser::new(toks).parse_program().unwrap();
        assert_eq!(prog.functions.len(), 2);
        assert_eq!(prog.functions[0].name, "fib");
        assert_eq!(
            prog.functions[0].params,
            vec![("n".to_string(), Type::Int(IntType::U8))]
        );
    }
}
