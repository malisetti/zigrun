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
    AssignTarget, BinOp, EnumDef, Expr, Function, IntType, Program, Stmt, StructDef, SwitchArm,
    SwitchTag, Type,
};
use crate::lexer::{Token, TokenKind};
use std::collections::HashMap;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    return_type: Type,
    enums: HashMap<String, Vec<String>>,
    structs: HashMap<String, Vec<(String, Type)>>,
    locals: HashMap<String, Type>,
    expr_enum_hint: Option<String>,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            return_type: Type::Int(IntType::U8),
            enums: HashMap::new(),
            structs: HashMap::new(),
            locals: HashMap::new(),
            expr_enum_hint: None,
        }
    }

    pub fn parse_program(mut self) -> Result<Program, String> {
        let mut enum_defs = Vec::new();
        let mut struct_defs = Vec::new();
        let mut functions = Vec::new();
        while !self.check(&TokenKind::Eof) {
            if self.check(&TokenKind::Const) {
                let decl = self.parse_const_decl()?;
                match decl {
                    TopLevelDecl::Enum(e) => enum_defs.push(e),
                    TopLevelDecl::Struct(s) => struct_defs.push(s),
                }
            } else {
                functions.push(self.parse_function()?);
            }
        }
        if functions.is_empty() {
            return Err("empty program: no functions".to_string());
        }
        Ok(Program {
            enums: enum_defs,
            structs: struct_defs,
            functions,
        })
    }

    fn parse_const_decl(&mut self) -> Result<TopLevelDecl, String> {
        self.expect(TokenKind::Const)?;
        let name = self.expect_ident()?;
        self.expect(TokenKind::Assign)?;
        if self.check(&TokenKind::Struct) {
            Ok(TopLevelDecl::Struct(self.parse_struct_body(name)?))
        } else {
            self.expect(TokenKind::Enum)?;
            Ok(TopLevelDecl::Enum(self.parse_enum_body(name)?))
        }
    }

    fn parse_enum_body(&mut self, name: String) -> Result<EnumDef, String> {
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
            return Err(format!("enum {name} has no variants"));
        }
        self.enums.insert(name.clone(), variants.clone());
        Ok(EnumDef { name, variants })
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
            let elem = match self.parse_type()? {
                Type::Int(t) => t,
                other => return Err(format!("array element must be an integer type, found {other:?}")),
            };
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
        if self.enums.contains_key(&name) {
            return Ok(Type::Enum(name));
        }
        if self.structs.contains_key(&name) {
            return Ok(Type::Struct(name));
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
                let value = self.parse_expr()?;
                self.expr_enum_hint = None;
                let ty = ty.unwrap_or_else(|| {
                    infer_expr_type(&value, &self.enums, &self.structs, &self.locals)
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
                let body = self.parse_block()?;
                Ok(Stmt::While { cond, body })
            }
            TokenKind::Break => {
                self.advance();
                self.expect(TokenKind::Semicolon)?;
                Ok(Stmt::Break)
            }
            TokenKind::Continue => {
                self.advance();
                self.expect(TokenKind::Semicolon)?;
                Ok(Stmt::Continue)
            }
            TokenKind::For => {
                self.advance();
                self.expect(TokenKind::LParen)?;
                let first = self.parse_expr()?;
                if self.check(&TokenKind::DotDot) {
                    self.advance();
                    let end = self.parse_expr()?;
                    self.expect(TokenKind::RParen)?;
                    let capture = self.parse_for_capture()?;
                    let body = self.parse_block()?;
                    Ok(Stmt::ForRange {
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
                    let body = self.parse_block()?;
                    Ok(Stmt::ForArray {
                        capture,
                        array,
                        body,
                    })
                }
            }
            TokenKind::Ident(name) => self.parse_assign_stmt(name),
            other => Err(format!("unexpected token at start of statement: {other:?}")),
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
        let target = if self.check(&TokenKind::LBracket) {
            self.advance();
            let index = self.parse_expr()?;
            self.expect(TokenKind::RBracket)?;
            AssignTarget::Index { base: name, index }
        } else {
            AssignTarget::Name(name)
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
            other => return Err(format!("expected assignment operator, found {other:?}")),
        };
        let rhs = self.parse_expr()?;
        self.expect(TokenKind::Semicolon)?;
        let value = if is_compound {
            let left = match &target {
                AssignTarget::Name(name) => Expr::Var(name.clone()),
                AssignTarget::Index { base, index } => Expr::Index {
                    base: base.clone(),
                    index: Box::new(index.clone()),
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

    fn parse_if_stmt(&mut self) -> Result<Stmt, String> {
        self.expect(TokenKind::LParen)?;
        let cond = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        let then_branch = self.parse_block()?;
        let else_branch = if self.check(&TokenKind::Else) {
            self.advance();
            if self.check(&TokenKind::If) {
                self.advance();
                Some(vec![self.parse_if_stmt()?])
            } else {
                Some(self.parse_block()?)
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
                expr = Expr::FieldAccess {
                    base: Box::new(expr),
                    field,
                };
            } else if self.check(&TokenKind::LBracket) {
                self.advance();
                let index = self.parse_expr()?;
                self.expect(TokenKind::RBracket)?;
                let base = match expr {
                    Expr::Var(name) => name,
                    other => {
                        return Err(format!("expected variable before index, found {other:?}"))
                    }
                };
                expr = Expr::Index {
                    base,
                    index: Box::new(index),
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.peek_kind() {
            TokenKind::LBracket => self.parse_typed_array_literal(),
            TokenKind::Dot => {
                self.advance();
                if self.check(&TokenKind::LBrace) {
                    self.expect(TokenKind::LBrace)?;
                    let elems = self.parse_array_elems()?;
                    self.expect(TokenKind::RBrace)?;
                    Ok(Expr::ArrayLiteral {
                        elems,
                        annotated: None,
                    })
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
            TokenKind::Int(n) => {
                self.advance();
                Ok(Expr::Int(n as i64))
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
                    if !self.structs.contains_key(&name) {
                        return Err(format!("unknown struct type {name:?}"));
                    }
                    self.advance();
                    let fields = self.parse_struct_literal_fields()?;
                    self.expect(TokenKind::RBrace)?;
                    Ok(Expr::StructLiteral {
                        struct_name: name,
                        fields,
                    })
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
                n as usize
            }
            other => return Err(format!("expected array length, found {other:?}")),
        };
        self.expect(TokenKind::RBracket)?;
        let elem = match self.parse_type()? {
            Type::Int(t) => t,
            other => {
                return Err(format!(
                    "array element must be an integer type, found {other:?}"
                ))
            }
        };
        self.expect(TokenKind::LBrace)?;
        let elems = self.parse_array_elems()?;
        self.expect(TokenKind::RBrace)?;
        Ok(Expr::ArrayLiteral {
            elems,
            annotated: Some((len, elem)),
        })
    }

    fn parse_struct_literal_fields(&mut self) -> Result<Vec<(String, Expr)>, String> {
        let mut fields = Vec::new();
        if !self.check(&TokenKind::RBrace) {
            loop {
                self.expect(TokenKind::Dot)?;
                let field = self.expect_ident()?;
                self.expect(TokenKind::Assign)?;
                let value = self.parse_expr()?;
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
        self.expect(TokenKind::RParen)?;
        self.expect(TokenKind::LBrace)?;
        let mut arms = Vec::new();
        let mut default = None;
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            if self.check(&TokenKind::Else) {
                if scrutinee_enum.is_some() {
                    return Err("enum switch cannot have `else` arm".to_string());
                }
                self.advance();
                self.expect(TokenKind::FatArrow)?;
                default = Some(Box::new(self.parse_expr()?));
            } else if self.check(&TokenKind::Dot) {
                let enum_name = scrutinee_enum.clone().ok_or_else(|| {
                    "enum switch arm requires enum scrutinee".to_string()
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
            } else {
                if scrutinee_enum.is_some() {
                    return Err("enum switch requires `.variant =>` arms".to_string());
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
        if scrutinee_enum.is_none() && default.is_none() {
            return Err("switch missing `else =>` arm".to_string());
        }
        Ok(Expr::Switch {
            scrutinee: Box::new(scrutinee),
            arms,
            default,
        })
    }

    fn infer_enum_type(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Var(name) => self.locals.get(name).and_then(|t| t.enum_name().map(str::to_string)),
            _ => None,
        }
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
    locals: &HashMap<String, Type>,
) -> Type {
    match expr {
        Expr::Int(_) => Type::Int(IntType::U8),
        Expr::Bool(_) => Type::Bool,
        Expr::Var(name) => locals
            .get(name)
            .cloned()
            .unwrap_or(Type::Int(IntType::U8)),
        Expr::Call { .. } => Type::Int(IntType::U8),
        Expr::BinOp { op, left, right } => match op {
            BinOp::LogicalAnd | BinOp::LogicalOr => Type::Bool,
            _ => {
                let lt = infer_expr_type(left, enums, structs, locals);
                let rt = infer_expr_type(right, enums, structs, locals);
                match (lt, rt) {
                    (Type::Int(a), Type::Int(b)) => Type::Int(wider_int_type(a, b)),
                    (Type::Int(a), _) => Type::Int(a),
                    (_, Type::Int(b)) => Type::Int(b),
                    _ => Type::Bool,
                }
            }
        },
        Expr::Switch { default, .. } => default
            .as_ref()
            .map(|d| infer_expr_type(d, enums, structs, locals))
            .unwrap_or(Type::Int(IntType::U8)),
        Expr::EnumLiteral { enum_name, .. } => Type::Enum(enum_name.clone()),
        Expr::IntCast { target, .. } => Type::Int(*target),
        Expr::Mod { left, right } | Expr::Rem { left, right } => {
            let lt = infer_expr_type(left, enums, structs, locals);
            let rt = infer_expr_type(right, enums, structs, locals);
            match (lt, rt) {
                (Type::Int(a), Type::Int(b)) => Type::Int(wider_int_type(a, b)),
                (Type::Int(a), _) => Type::Int(a),
                (_, Type::Int(b)) => Type::Int(b),
                _ => Type::Int(IntType::U8),
            }
        }
        Expr::UnaryNeg(inner) => infer_expr_type(inner, enums, structs, locals),
        Expr::UnaryNot(_) => Type::Bool,
        Expr::ArrayLiteral { elems, annotated } => {
            if let Some((len, elem)) = annotated {
                Type::Array {
                    len: *len,
                    elem: *elem,
                }
            } else if let Some(first) = elems.first() {
                let elem = infer_expr_type(first, enums, structs, locals)
                    .int_type()
                    .unwrap_or(IntType::U8);
                Type::Array {
                    len: elems.len(),
                    elem,
                }
            } else {
                Type::Array {
                    len: 0,
                    elem: IntType::U8,
                }
            }
        }
        Expr::Index { base: _, index: _ } => Type::Int(IntType::U8),
        Expr::StructLiteral { struct_name, .. } => Type::Struct(struct_name.clone()),
        Expr::FieldAccess { base, field } => field_type(base, field, enums, structs, locals),
    }
}

fn field_type(
    base: &Expr,
    field: &str,
    _enums: &HashMap<String, Vec<String>>,
    structs: &HashMap<String, Vec<(String, Type)>>,
    locals: &HashMap<String, Type>,
) -> Type {
    let base_ty = match base {
        Expr::Var(name) => locals.get(name).cloned(),
        Expr::FieldAccess {
            base,
            field: parent_field,
        } => Some(field_type(base, parent_field, _enums, structs, locals)),
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
    Struct(StructDef),
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
