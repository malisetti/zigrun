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
    AssignTarget, BinOp, EnumDecl, Expr, Function, IntType, Program, Stmt, StructDecl,
    SwitchCase, Type,
};
use crate::lexer::{Token, TokenKind};
use std::collections::HashMap;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    return_type: Type,
    enums: Vec<EnumDecl>,
    structs: HashMap<String, Vec<(String, Type)>>,
    struct_defs: Vec<StructDecl>,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            return_type: Type::Int(IntType::U8),
            enums: Vec::new(),
            structs: HashMap::new(),
            struct_defs: Vec::new(),
        }
    }

    pub fn parse_program(mut self) -> Result<Program, String> {
        let mut functions = Vec::new();
        while !self.check(&TokenKind::Eof) {
            if self.check(&TokenKind::Const) {
                self.parse_const_decl()?;
            } else {
                functions.push(self.parse_function()?);
            }
        }
        if functions.is_empty() {
            return Err("empty program: no functions".to_string());
        }
        Ok(Program {
            enums: self.enums,
            structs: self.struct_defs,
            functions,
        })
    }

    fn parse_const_decl(&mut self) -> Result<(), String> {
        self.expect(TokenKind::Const)?;
        let name = self.expect_ident()?;
        self.expect(TokenKind::Assign)?;
        if self.check(&TokenKind::Enum) {
            self.parse_enum_body(name)?;
        } else if self.check(&TokenKind::Struct) {
            let s = self.parse_struct_body(name)?;
            self.struct_defs.push(s);
        } else {
            return Err(format!(
                "expected `enum` or `struct` after const {name} ="
            ));
        }
        self.expect(TokenKind::Semicolon)?;
        Ok(())
    }

    fn parse_enum_body(&mut self, name: String) -> Result<(), String> {
        self.expect(TokenKind::Enum)?;
        self.expect(TokenKind::LBrace)?;
        let mut variants = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            variants.push(self.expect_ident()?);
            if self.check(&TokenKind::Comma) {
                self.advance();
            }
        }
        self.expect(TokenKind::RBrace)?;
        self.enums.push(EnumDecl { name, variants });
        Ok(())
    }

    fn parse_struct_body(&mut self, name: String) -> Result<StructDecl, String> {
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
        if fields.is_empty() {
            return Err(format!("struct {name} has no fields"));
        }
        self.structs.insert(name.clone(), fields.clone());
        Ok(StructDecl { name, fields })
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
            let inner = self.parse_type()?;
            return Ok(Type::Optional {
                inner: Box::new(inner),
            });
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
            let elem = self.parse_type()?;
            return Ok(Type::Array {
                len,
                elem: Box::new(elem),
            });
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
        if self.enums.iter().any(|e| e.name == name) {
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

    /// `if (cond) return x;` (single stmt) or `if (cond) { ... }` (block).
    fn parse_block_or_stmt(&mut self) -> Result<Vec<Stmt>, String> {
        if self.check(&TokenKind::LBrace) {
            self.parse_block()
        } else {
            Ok(vec![self.parse_stmt()?])
        }
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
                let value = self.parse_expr()?;
                let ty = ty.unwrap_or_else(|| infer_expr_type(&value, &self.enums, &self.structs));
                self.expect(TokenKind::Semicolon)?;
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
            Expr::Index { base, index } => AssignTarget::Index { base, index: *index },
            other => {
                return Err(format!(
                    "expected variable or indexed lvalue for assignment, found {other:?}"
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
        let then_branch = self.parse_block_or_stmt()?;
        let else_branch = if self.check(&TokenKind::Else) {
            self.advance();
            if self.check(&TokenKind::If) {
                self.advance();
                Some(vec![self.parse_if_stmt()?])
            } else {
                Some(self.parse_block_or_stmt()?)
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
        self.parse_orelse()
    }

    fn parse_orelse(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_logical_or()?;
        if self.check(&TokenKind::Orelse) {
            self.advance();
            let right = self.parse_orelse()?;
            left = Expr::Orelse {
                opt: Box::new(left),
                default: Box::new(right),
            };
        }
        Ok(left)
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
            if self.check(&TokenKind::LBracket) {
                self.advance();
                let index = self.parse_expr()?;
                self.expect(TokenKind::RBracket)?;
                expr = Expr::Index {
                    base: Box::new(expr),
                    index: Box::new(index),
                };
            } else if self.check(&TokenKind::Dot) {
                self.advance();
                let field = self.expect_ident()?;
                expr = Expr::FieldAccess {
                    base: Box::new(expr),
                    field,
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
                    self.advance();
                    let elems = self.parse_array_elems()?;
                    self.expect(TokenKind::RBrace)?;
                    Ok(Expr::ArrayLiteral {
                        elems,
                        annotated: None,
                    })
                } else {
                    let variant = self.expect_ident()?;
                    Ok(Expr::EnumLiteral {
                        enum_name: String::new(),
                        variant,
                    })
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
            TokenKind::Undefined => {
                self.advance();
                Ok(Expr::Undefined)
            }
            TokenKind::Null => {
                self.advance();
                Ok(Expr::Null)
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
                if self.check(&TokenKind::LParen) {
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
                } else if self.check(&TokenKind::LBrace) && self.structs.contains_key(&name) {
                    self.advance();
                    let fields = self.parse_struct_literal_fields()?;
                    self.expect(TokenKind::RBrace)?;
                    Ok(Expr::StructLiteral {
                        struct_name: name,
                        fields,
                    })
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
        let elem_ty = self.parse_type()?;
        self.expect(TokenKind::LBrace)?;
        let elems = self.parse_array_elems()?;
        self.expect(TokenKind::RBrace)?;
        let elem = elem_ty
            .scalar_int_type()
            .ok_or_else(|| format!("array element must be an integer type, found {elem_ty:?}"))?;
        Ok(Expr::ArrayLiteral {
            elems,
            annotated: Some((len, elem)),
        })
    }

    fn parse_struct_literal_fields(&mut self) -> Result<Vec<(String, Expr)>, String> {
        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            self.expect(TokenKind::Dot)?;
            let name = self.expect_ident()?;
            self.expect(TokenKind::Assign)?;
            let value = self.parse_expr()?;
            fields.push((name, value));
            if self.check(&TokenKind::Comma) {
                self.advance();
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
        self.expect(TokenKind::RParen)?;
        self.expect(TokenKind::LBrace)?;
        let mut arms = Vec::new();
        let mut default = None;
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            if self.check(&TokenKind::Else) {
                self.advance();
                self.expect(TokenKind::FatArrow)?;
                default = Some(Box::new(self.parse_expr()?));
            } else {
                let case = if self.check(&TokenKind::Dot) {
                    self.advance();
                    SwitchCase::Variant(self.expect_ident()?)
                } else {
                    match self.peek_kind() {
                        TokenKind::Int(n) => {
                            self.advance();
                            SwitchCase::Int(n)
                        }
                        other => {
                            return Err(format!(
                                "expected integer or .variant in switch arm, found {other:?}"
                            ));
                        }
                    }
                };
                self.expect(TokenKind::FatArrow)?;
                arms.push((case, self.parse_expr()?));
            }
            if self.check(&TokenKind::Comma) {
                self.advance();
            }
        }
        self.expect(TokenKind::RBrace)?;
        Ok(Expr::Switch {
            scrutinee: Box::new(scrutinee),
            arms,
            default,
        })
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
    enums: &[EnumDecl],
    structs: &HashMap<String, Vec<(String, Type)>>,
) -> Type {
    match expr {
        Expr::Int(_) => Type::Int(IntType::U8),
        Expr::Bool(_) => Type::Bool,
        Expr::Var(_) => Type::Int(IntType::U8),
        Expr::Call { .. } => Type::Int(IntType::U8),
        Expr::BinOp { op, left, right } => match op {
            BinOp::LogicalAnd | BinOp::LogicalOr => Type::Bool,
            _ => {
                let lt = infer_expr_type(left, enums, structs);
                let rt = infer_expr_type(right, enums, structs);
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
            .map(|d| infer_expr_type(d, enums, structs))
            .unwrap_or(Type::Int(IntType::U8)),
        Expr::EnumLiteral { enum_name, .. } => {
            if enum_name.is_empty() {
                Type::Int(IntType::U8)
            } else {
                Type::Enum(enum_name.clone())
            }
        }
        Expr::IntCast { target, .. } => Type::Int(*target),
        Expr::Mod { left, right } | Expr::Rem { left, right } => {
            let lt = infer_expr_type(left, enums, structs);
            let rt = infer_expr_type(right, enums, structs);
            match (lt, rt) {
                (Type::Int(a), Type::Int(b)) => Type::Int(wider_int_type(a, b)),
                (Type::Int(a), _) => Type::Int(a),
                (_, Type::Int(b)) => Type::Int(b),
                _ => Type::Int(IntType::U8),
            }
        }
        Expr::UnaryNeg(inner) => infer_expr_type(inner, enums, structs),
        Expr::UnaryNot(_) => Type::Bool,
        Expr::ArrayLiteral { elems, annotated } => {
            if let Some((len, elem)) = annotated {
                Type::Array {
                    len: *len,
                    elem: Box::new(Type::Int(*elem)),
                }
            } else if let Some(first) = elems.first() {
                let elem = infer_expr_type(first, enums, structs)
                    .scalar_int_type()
                    .unwrap_or(IntType::U8);
                Type::Array {
                    len: elems.len(),
                    elem: Box::new(Type::Int(elem)),
                }
            } else {
                Type::Array {
                    len: 0,
                    elem: Box::new(Type::Int(IntType::U8)),
                }
            }
        }
        Expr::Index { base, index: _ } => index_result_type(base, enums, structs),
        Expr::Undefined => Type::Int(IntType::U8),
        Expr::StructLiteral { struct_name, .. } => Type::Struct(struct_name.clone()),
        Expr::FieldAccess { base, field } => infer_field_type(base, field, enums, structs),
        Expr::Null => Type::Optional {
            inner: Box::new(Type::Int(IntType::U8)),
        },
        Expr::Orelse { default, .. } => infer_expr_type(default, enums, structs),
    }
}

fn index_result_type(
    base: &Expr,
    enums: &[EnumDecl],
    structs: &HashMap<String, Vec<(String, Type)>>,
) -> Type {
    match base {
        Expr::Index { base: inner, .. } => {
            let inner_ty = index_result_type(inner, enums, structs);
            inner_ty
                .array_elem()
                .unwrap_or(Type::Int(IntType::U8))
        }
        _ => Type::Int(IntType::U8),
    }
}

fn infer_field_type(
    base: &Expr,
    field: &str,
    _enums: &[EnumDecl],
    structs: &HashMap<String, Vec<(String, Type)>>,
) -> Type {
    let struct_name = match base {
        Expr::StructLiteral { struct_name, .. } => Some(struct_name.clone()),
        Expr::FieldAccess { base, field: parent_field } => {
            infer_field_type(base, parent_field, _enums, structs)
                .struct_name()
                .map(str::to_string)
        }
        _ => None,
    };
    struct_name
        .and_then(|sn| structs.get(&sn))
        .and_then(|fields| fields.iter().find(|(n, _)| n == field).map(|(_, ty)| ty.clone()))
        .unwrap_or(Type::Int(IntType::U8))
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
