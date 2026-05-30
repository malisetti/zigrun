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
    expr_type_hint: Option<Type>,
    struct_methods: HashMap<String, Vec<String>>,
    packed_structs: std::collections::HashSet<String>,
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
            expr_type_hint: None,
            struct_methods: HashMap::new(),
            packed_structs: std::collections::HashSet::new(),
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
                if self.try_skip_import_decl() {
                    continue;
                }
                let decl = self.parse_const_decl()?;
                match decl {
                    TopLevelDecl::Enum(e) => enum_defs.push(e),
                    TopLevelDecl::ErrorSet(e) => error_set_defs.push(e),
                    TopLevelDecl::Struct(s) => {
                        for m in &s.methods {
                            functions.push(m.clone());
                        }
                        struct_defs.push(s);
                    }
                    TopLevelDecl::Union(u) => union_defs.push(u),
                }
            } else {
                let f = self.parse_function()?;
                self.functions.insert(f.name.clone(), f.return_type.clone());
                functions.push(f);
            }
        }
        if functions.is_empty() {
            return Err("empty program: no functions".to_string());
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
        if self.check(&TokenKind::Packed) {
            self.advance();
            Ok(TopLevelDecl::Struct(self.parse_struct_body(name, true)?))
        } else if self.check(&TokenKind::Struct) {
            Ok(TopLevelDecl::Struct(self.parse_struct_body(name, false)?))
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

    fn parse_struct_body(&mut self, name: String, packed: bool) -> Result<StructDef, String> {
        self.expect(TokenKind::Struct)?;
        self.expect(TokenKind::LBrace)?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        // Register the struct name early so method bodies can reference the type.
        self.structs.insert(name.clone(), Vec::new());
        if packed {
            self.packed_structs.insert(name.clone());
        }
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            if self.check(&TokenKind::Fn) {
                let method = self.parse_struct_method(&name)?;
                let short_name = method.name
                    .strip_prefix(&format!("{}_", name))
                    .unwrap_or(&method.name)
                    .to_string();
                self.struct_methods.entry(name.clone()).or_default().push(short_name);
                self.functions.insert(method.name.clone(), method.return_type.clone());
                methods.push(method);
            } else {
                let field = self.expect_ident()?;
                self.expect(TokenKind::Colon)?;
                let ty = self.parse_type()?;
                fields.push((field, ty));
                if self.check(&TokenKind::Comma) {
                    self.advance();
                }
            }
        }
        self.expect(TokenKind::RBrace)?;
        self.expect(TokenKind::Semicolon)?;
        if fields.is_empty() && methods.is_empty() {
            return Err(format!("struct {name} has no fields"));
        }
        self.structs.insert(name.clone(), fields.clone());
        Ok(StructDef { name, packed, fields, methods })
    }

    fn parse_struct_method(&mut self, struct_name: &str) -> Result<crate::ast::Function, String> {
        self.expect(TokenKind::Fn)?;
        let short_name = self.expect_ident()?;
        let mangled_name = format!("{}_{}", struct_name, short_name);
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
        Ok(crate::ast::Function {
            name: mangled_name,
            params,
            return_type,
            body,
        })
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
        if self.check(&TokenKind::Star) {
            self.advance();
            return Ok(Type::Pointer(Box::new(self.parse_type()?)));
        }
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
            if self.check(&TokenKind::RBracket) {
                self.advance();
                let const_ = if self.check(&TokenKind::Const) {
                    self.advance();
                    true
                } else {
                    false
                };
                let elem = Box::new(self.parse_type()?);
                return Ok(Type::Slice { const_, elem });
            }
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
                self.expr_type_hint = ty.clone();
                let value = self.parse_expr()?;
                self.expr_enum_hint = None;
                self.expr_union_hint = None;
                self.expr_type_hint = None;
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

        // Check for `&array` (pointer iteration)
        let ptr_iter = if self.check(&TokenKind::Amp) {
            self.advance();
            true
        } else {
            false
        };

        let first = self.parse_expr()?;

        // ForRange: for (start..end) |i|  (only when not ptr_iter)
        if !ptr_iter && self.check(&TokenKind::DotDot) {
            self.advance();
            let end = self.parse_expr()?;
            self.expect(TokenKind::RParen)?;
            let capture = self.parse_for_capture()?;
            let body = self.parse_for_body()?;
            return Ok(Stmt::ForRange {
                label,
                capture,
                start: first,
                end,
                body,
            });
        }

        // Check for `, 0..` (index capture secondary iterable)
        let has_idx = if self.check(&TokenKind::Comma) {
            self.advance();
            match self.peek_kind() {
                TokenKind::Int(0) => { self.advance(); }
                other => return Err(format!("expected 0 after comma in for-loop, found {other:?}")),
            }
            self.expect(TokenKind::DotDot)?;
            true
        } else {
            false
        };

        self.expect(TokenKind::RParen)?;

        let array = match first {
            Expr::Var(name) => name,
            other => {
                return Err(format!(
                    "expected array identifier in for-loop, found {other:?}"
                ))
            }
        };

        let (capture, ptr_capture, idx_capture) = self.parse_for_captures_extended(has_idx)?;
        let body = self.parse_for_body()?;

        Ok(Stmt::ForArray {
            label,
            capture,
            ptr_capture,
            idx_capture,
            array,
            ptr_iter,
            body,
        })
    }

    /// Parse `|cap|`, `|*cap|`, `|*cap, idx|`, `|cap, idx|`, etc.
    fn parse_for_captures_extended(
        &mut self,
        expect_idx: bool,
    ) -> Result<(Option<String>, bool, Option<String>), String> {
        self.expect(TokenKind::Pipe)?;

        // First capture: `*name`, `name`, or `_`
        let (capture, ptr_capture) = if self.check(&TokenKind::Star) {
            self.advance();
            let name = self.expect_ident()?;
            (Some(name), true)
        } else {
            match self.peek_kind() {
                TokenKind::Ident(name) if name == "_" => {
                    self.advance();
                    (None, false)
                }
                TokenKind::Ident(name) => {
                    self.advance();
                    (Some(name), false)
                }
                other => return Err(format!("expected capture identifier, found {other:?}")),
            }
        };

        // Optional second capture (index)
        let idx_capture = if expect_idx {
            self.expect(TokenKind::Comma)?;
            match self.peek_kind() {
                TokenKind::Ident(name) if name == "_" => {
                    self.advance();
                    None
                }
                TokenKind::Ident(name) => {
                    self.advance();
                    Some(name)
                }
                other => return Err(format!("expected index capture, found {other:?}")),
            }
        } else {
            None
        };

        self.expect(TokenKind::Pipe)?;
        Ok((capture, ptr_capture, idx_capture))
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
        while self.check(&TokenKind::LBracket) || self.check(&TokenKind::Dot) {
            if self.check(&TokenKind::Dot) {
                self.advance();
                // `ptr.*` — pointer dereference lvalue
                if self.check(&TokenKind::Star) {
                    self.advance();
                    target_expr = Expr::Deref(Box::new(target_expr));
                    break; // dereference is always the final target
                }
                let field = self.expect_ident()?;
                target_expr = Expr::FieldAccess {
                    base: Box::new(target_expr),
                    field,
                };
            } else {
                self.advance();
                let index = self.parse_expr()?;
                self.expect(TokenKind::RBracket)?;
                target_expr = Expr::Index {
                    base: Box::new(target_expr),
                    index: Box::new(index),
                };
            }
        }
        if let Expr::FieldAccess { base, field } = &target_expr {
            if self.check(&TokenKind::LParen) {
                let struct_name = match base.as_ref() {
                    Expr::Var(v) => self
                        .locals
                        .get(v)
                        .and_then(|t| t.struct_name().map(str::to_string)),
                    _ => None,
                };
                if let Some(sn) = struct_name {
                    if self
                        .struct_methods
                        .get(&sn)
                        .map_or(false, |ms| ms.contains(field))
                    {
                        self.advance();
                        let mut args = vec![*base.clone()];
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
                        self.expect(TokenKind::Semicolon)?;
                        return Ok(Stmt::Assign {
                            target: AssignTarget::Name("_".to_string()),
                            value: Expr::Call {
                                name: format!("{}_{}", sn, field),
                                args,
                            },
                        });
                    }
                }
            }
        }
        if self.check(&TokenKind::LParen) {
            if let Some(stmt) = self.try_parse_debug_print_stmt(&target_expr)? {
                return Ok(stmt);
            }
        }
        let target = match target_expr {
            Expr::Var(name) => AssignTarget::Name(name),
            Expr::Index { base, index } => AssignTarget::Index { base, index },
            Expr::FieldAccess { base, field } => AssignTarget::Field { base, field },
            Expr::Deref(inner) => AssignTarget::Deref(inner),
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
                AssignTarget::Field { base, field } => Expr::FieldAccess {
                    base: base.clone(),
                    field: field.clone(),
                },
                AssignTarget::Deref(inner) => Expr::Deref(inner.clone()),
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

        let ok_capture = if self.check(&TokenKind::Pipe) {
            self.advance();
            let name = self.expect_ident()?;
            self.expect(TokenKind::Pipe)?;
            Some(name)
        } else {
            None
        };

        let err_union = match self.infer_expr_type_local(&cond) {
            Type::ErrorUnion { err_set, payload } => Some((err_set, (*payload).clone())),
            _ => None,
        };

        let saved_ok = if let (Some(cap), Some((_, payload))) = (&ok_capture, &err_union) {
            Some((cap.clone(), self.locals.insert(cap.clone(), payload.clone())))
        } else {
            None
        };

        let then_branch = self.parse_if_branch()?;

        if let Some((cap, prev)) = saved_ok {
            match prev {
                Some(t) => {
                    self.locals.insert(cap, t);
                }
                None => {
                    self.locals.remove(&cap);
                }
            }
        }

        let (err_capture, else_branch) = if self.check(&TokenKind::Else) {
            self.advance();

            let err_cap = if self.check(&TokenKind::Pipe) {
                self.advance();
                let name = self.expect_ident()?;
                self.expect(TokenKind::Pipe)?;
                Some(name)
            } else {
                None
            };

            let saved_err = if let (Some(cap), Some((err_set, _))) = (&err_cap, &err_union) {
                let tag_ty = Type::Enum(format!("{err_set}_err"));
                Some((cap.clone(), self.locals.insert(cap.clone(), tag_ty)))
            } else {
                None
            };

            let else_br = if self.check(&TokenKind::If) {
                self.advance();
                Some(vec![self.parse_if_stmt()?])
            } else {
                Some(self.parse_if_branch()?)
            };

            if let Some((cap, prev)) = saved_err {
                match prev {
                    Some(t) => {
                        self.locals.insert(cap, t);
                    }
                    None => {
                        self.locals.remove(&cap);
                    }
                }
            }

            (err_cap, else_br)
        } else {
            (None, None)
        };

        Ok(Stmt::If {
            cond,
            ok_capture,
            then_branch,
            err_capture,
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
        if self.check(&TokenKind::Amp) {
            self.advance();
            let operand = self.parse_unary()?;
            return Ok(Expr::AddrOf(Box::new(operand)));
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.check(&TokenKind::Dot) {
                self.advance();
                // `expr.*` — pointer dereference
                if self.check(&TokenKind::Star) {
                    self.advance();
                    expr = Expr::Deref(Box::new(expr));
                    continue;
                }
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
                // Check for method call: expr.method(args)
                if self.check(&TokenKind::LParen) {
                    let struct_name = match &expr {
                        Expr::Var(v) => self.locals.get(v).and_then(|t| t.struct_name().map(str::to_string)),
                        _ => None,
                    };
                    if let Some(sn) = struct_name {
                        if self.struct_methods.get(&sn).map_or(false, |ms| ms.contains(&field)) {
                            self.advance(); // consume (
                            let mut args = vec![expr];
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
                            expr = Expr::Call { name: format!("{}_{}", sn, field), args };
                            continue;
                        }
                    }
                }
                expr = Expr::FieldAccess {
                    base: Box::new(expr),
                    field,
                };
            } else if self.check(&TokenKind::LBracket) {
                self.advance();
                let first = self.parse_expr()?;
                if self.check(&TokenKind::DotDot) {
                    self.advance();
                    let end = self.parse_expr()?;
                    self.expect(TokenKind::RBracket)?;
                    expr = Expr::SliceRange {
                        base: Box::new(expr),
                        start: Box::new(first),
                        end: Box::new(end),
                    };
                    continue;
                }
                self.expect(TokenKind::RBracket)?;
                expr = Expr::Index {
                    base: Box::new(expr),
                    index: Box::new(first),
                };
            } else {
                break;
            }
        }
        if self.check(&TokenKind::Catch) {
            self.advance();
            let capture = self.parse_switch_capture()?;
            let err_tag = self
                .infer_expr_type_local(&expr)
                .error_union_err_set()
                .map(|err_set| Type::Enum(format!("{err_set}_err")));
            let saved_capture = capture.as_ref().zip(err_tag).map(|(cap, tag_ty)| {
                (
                    cap.clone(),
                    self.locals.insert(cap.clone(), tag_ty),
                )
            });
            if self.check(&TokenKind::Return) {
                self.advance();
                let ret_val = self.parse_unary()?;
                expr = Expr::CatchReturn {
                    expr: Box::new(expr),
                    capture,
                    ret_val: Box::new(ret_val),
                };
            } else {
                let fallback = self.parse_unary()?;
                expr = Expr::Catch {
                    expr: Box::new(expr),
                    capture,
                    fallback: Box::new(fallback),
                };
            }
            if let Some((cap, prev)) = saved_capture {
                match prev {
                    Some(t) => {
                        self.locals.insert(cap, t);
                    }
                    None => {
                        self.locals.remove(&cap);
                    }
                }
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

    fn try_skip_import_decl(&mut self) -> bool {
        let save = self.pos;
        if !self.check(&TokenKind::Const) {
            return false;
        }
        self.advance();
        if self.expect_ident().is_err() {
            self.pos = save;
            return false;
        }
        if !self.check(&TokenKind::Assign) {
            self.pos = save;
            return false;
        }
        self.advance();
        if !self.check(&TokenKind::At) {
            self.pos = save;
            return false;
        }
        self.advance();
        if self.expect_ident().ok().as_deref() != Some("import") {
            self.pos = save;
            return false;
        }
        if self.expect(TokenKind::LParen).is_err() {
            self.pos = save;
            return false;
        }
        while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
            self.advance();
        }
        if self.expect(TokenKind::RParen).is_err() || self.expect(TokenKind::Semicolon).is_err() {
            self.pos = save;
            return false;
        }
        true
    }

    fn field_path(expr: &Expr) -> Option<Vec<String>> {
        match expr {
            Expr::Var(name) => Some(vec![name.clone()]),
            Expr::FieldAccess { base, field } => {
                let mut p = Self::field_path(base)?;
                p.push(field.clone());
                Some(p)
            }
            _ => None,
        }
    }

    fn try_parse_debug_print_stmt(&mut self, target: &Expr) -> Result<Option<Stmt>, String> {
        let path = match Self::field_path(target) {
            Some(p) if p == ["std", "debug", "print"] => p,
            _ => return Ok(None),
        };
        let _ = path;
        self.expect(TokenKind::LParen)?;
        let format = match self.peek_kind() {
            TokenKind::StringLit(s) => {
                self.advance();
                s
            }
            other => return Err(format!("std.debug.print expects string literal, got {other:?}")),
        };
        self.expect(TokenKind::Comma)?;
        self.expect(TokenKind::Dot)?;
        self.expect(TokenKind::LBrace)?;
        self.expect(TokenKind::RBrace)?;
        self.expect(TokenKind::RParen)?;
        self.expect(TokenKind::Semicolon)?;
        Ok(Some(Stmt::Expr(Expr::DebugPrint { format })))
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.peek_kind() {
            TokenKind::StringLit(s) => {
                self.advance();
                Ok(Expr::StringLit(s))
            }
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
                    "as" => {
                        let ty = self.parse_type()?;
                        self.expect(TokenKind::Comma)?;
                        let expr = self.parse_expr()?;
                        self.expect(TokenKind::RParen)?;
                        if let Type::Int(target) = ty {
                            Ok(Expr::IntCast {
                                expr: Box::new(expr),
                                target,
                            })
                        } else {
                            Err(format!("@as only supports integer types, got {:?}", ty))
                        }
                    }
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
                    "bitCast" => {
                        let expr = self.parse_expr()?;
                        self.expect(TokenKind::RParen)?;
                        let target = self
                            .expr_type_hint
                            .clone()
                            .or_else(|| {
                                if self.return_type.int_type().is_some()
                                    || self.return_type.struct_name().is_some()
                                {
                                    Some(self.return_type.clone())
                                } else {
                                    None
                                }
                            })
                            .ok_or_else(|| {
                                "@bitCast requires a type context (integer or packed struct)"
                                    .to_string()
                            })?;
                        match &target {
                            Type::Int(_) | Type::Struct(_) => {}
                            _ => {
                                return Err(
                                    "@bitCast target must be an integer or struct type".to_string(),
                                );
                            }
                        }
                        Ok(Expr::BitCast {
                            expr: Box::new(expr),
                            target,
                        })
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
            TokenKind::Null => {
                self.advance();
                Ok(Expr::Null)
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
                    if !self.enum_has_variant(&enum_name, &variant) {
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
            } else if let TokenKind::Ident(potential_enum) = self.peek_kind() {
                if !self.check_next(&TokenKind::Dot) {
                    if scrutinee_enum.is_some() || scrutinee_union.is_some() {
                        return Err("enum/union switch requires `.variant =>` arms".to_string());
                    }
                    let tags = self.parse_int_switch_tags()?;
                    self.expect(TokenKind::FatArrow)?;
                    let capture = self.parse_switch_capture()?;
                    let expr = self.parse_expr()?;
                    for tag in tags {
                        let tag = match tag {
                            SwitchTag::IntRange { lo, hi, .. } => SwitchTag::IntRange {
                                lo,
                                hi,
                                capture: capture.clone(),
                            },
                            other => other,
                        };
                        arms.push(SwitchArm { tag, expr: expr.clone() });
                    }
                } else {
                    let enum_name = scrutinee_enum.clone().ok_or_else(|| {
                        "qualified switch arm requires enum or error-tag scrutinee".to_string()
                    })?;
                    self.advance();
                    self.advance();
                    let variant = self.expect_ident()?;
                    if enum_name == potential_enum {
                        if !self.enum_has_variant(&enum_name, &variant) {
                            return Err(format!(
                                "unknown variant {variant:?} for enum {enum_name:?}"
                            ));
                        }
                    } else if enum_name.ends_with("_err")
                        && format!("{potential_enum}_err") == enum_name
                    {
                        if !self.enum_has_variant(&enum_name, &variant) {
                            return Err(format!(
                                "unknown variant {variant:?} for error set {potential_enum:?}"
                            ));
                        }
                    } else {
                        return Err(format!(
                            "switch arm enum {potential_enum:?} does not match scrutinee enum {enum_name:?}"
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
                let tags = self.parse_int_switch_tags()?;
                self.expect(TokenKind::FatArrow)?;
                let capture = self.parse_switch_capture()?;
                let expr = self.parse_expr()?;
                for tag in tags {
                    let tag = match tag {
                        SwitchTag::IntRange { lo, hi, .. } => SwitchTag::IntRange {
                            lo,
                            hi,
                            capture: capture.clone(),
                        },
                        other => other,
                    };
                    arms.push(SwitchArm { tag, expr: expr.clone() });
                }
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
                    if !self.enum_has_variant(enum_name, &variant) {
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
            } else if let TokenKind::Ident(enum_or_name) = self.peek_kind() {
                let potential_enum = enum_or_name.clone();
                let next_pos = self.pos + 1;
                let next_is_dot = next_pos < self.tokens.len() && self.tokens[next_pos].kind == TokenKind::Dot;

                if next_is_dot {
                    self.advance();
                    self.advance();
                    let variant = self.expect_ident()?;
                    if let Some(ref enum_name) = scrutinee_enum {
                        if enum_name == &potential_enum {
                            if !self.enum_has_variant(enum_name, &variant) {
                                return Err(format!(
                                    "unknown variant {variant:?} for enum {enum_name:?}"
                                ));
                            }
                            SwitchTag::EnumVariant {
                                enum_name: enum_name.clone(),
                                variant,
                            }
                        } else if enum_name.ends_with("_err") && &format!("{potential_enum}_err") == enum_name {
                            if !self.enum_has_variant(enum_name, &variant) {
                                return Err(format!(
                                    "unknown variant {variant:?} for error set {potential_enum:?}"
                                ));
                            }
                            SwitchTag::EnumVariant {
                                enum_name: enum_name.clone(),
                                variant,
                            }
                        } else {
                            return Err(format!(
                                "switch arm enum {potential_enum:?} does not match scrutinee enum {enum_name:?}"
                            ));
                        }
                    } else {
                        return Err(format!(
                            "enum variant arm {potential_enum}.{variant} requires enum scrutinee"
                        ));
                    }
                } else {
                    return Err(format!(
                        "expected `.variant` or integer switch arm, found {:?}",
                        self.peek_kind()
                    ));
                }
            } else if self.check(&TokenKind::Else) {
                return Err("switch statement does not support `else` arm".to_string());
            } else {
                let tags = self.parse_int_switch_tags()?;
                self.expect(TokenKind::FatArrow)?;
                let capture = self.parse_switch_capture()?;
                let body = if self.check(&TokenKind::LBrace) {
                    self.parse_block()?
                } else {
                    vec![self.parse_switch_arm_stmt()?]
                };
                for tag in tags {
                    let tag = match tag {
                        SwitchTag::IntRange { lo, hi, .. } => SwitchTag::IntRange {
                            lo,
                            hi,
                            capture: capture.clone(),
                        },
                        other => other,
                    };
                    arms.push(SwitchStmtArm {
                        tag,
                        body: body.clone(),
                    });
                }
                if self.check(&TokenKind::Comma) {
                    self.advance();
                }
                continue;
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

    fn infer_expr_type_local(&self, expr: &Expr) -> Type {
        infer_expr_type(
            expr,
            &self.enums,
            &self.structs,
            &self.unions,
            &self.locals,
            &self.functions,
        )
    }

    fn enum_has_variant(&self, enum_name: &str, variant: &str) -> bool {
        if self
            .enums
            .get(enum_name)
            .is_some_and(|vs| vs.iter().any(|v| v == variant))
        {
            return true;
        }
        if let Some(err_set) = enum_name.strip_suffix("_err") {
            return self
                .error_sets
                .get(err_set)
                .is_some_and(|vs| vs.iter().any(|v| v == variant));
        }
        false
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

    fn infer_error_set_type(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Var(name) => self
                .locals
                .get(name)
                .and_then(|t| t.error_union_err_set().map(str::to_string)),
            _ => None,
        }
    }

    fn parse_int_switch_tags(&mut self) -> Result<Vec<SwitchTag>, String> {
        let mut tags = Vec::new();
        loop {
            let lo = match self.peek_kind() {
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
            if self.check(&TokenKind::Ellipsis) {
                self.advance();
                let hi = match self.peek_kind() {
                    TokenKind::Int(n) => {
                        self.advance();
                        n
                    }
                    other => {
                        return Err(format!(
                            "expected integer literal after `...`, found {other:?}"
                        ));
                    }
                };
                tags.push(SwitchTag::IntRange {
                    lo,
                    hi,
                    capture: None,
                });
                break;
            }
            tags.push(SwitchTag::Int(lo));
            if self.check(&TokenKind::Comma) && matches!(self.tokens.get(self.pos + 1).map(|t| &t.kind), Some(TokenKind::Int(_))) {
                self.advance();
                continue;
            }
            break;
        }
        Ok(tags)
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
        Expr::Null => Type::Int(IntType::U8),
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
        Expr::BitCast { target, .. } => target.clone(),
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
        Expr::AddrOf(inner) => {
            let inner_ty = infer_expr_type(inner, enums, structs, unions, locals, functions);
            Type::Pointer(Box::new(inner_ty))
        }
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
        Expr::SliceRange { base, .. } => {
            let base_ty = infer_expr_type(base, enums, structs, unions, locals, functions);
            match base_ty {
                Type::Slice { .. } => base_ty,
                Type::Array { elem, .. } => Type::Slice {
                    const_: false,
                    elem,
                },
                _ => Type::Slice {
                    const_: false,
                    elem: Box::new(Type::Int(IntType::U8)),
                },
            }
        }
        Expr::StructLiteral { struct_name, .. } => Type::Struct(struct_name.clone()),
        Expr::UnionLiteral { union_name, .. } => Type::Union(
            union_name
                .clone()
                .expect("union literal requires type context"),
        ),
        Expr::EmptyInit => Type::Void,
        Expr::FieldAccess { base, field } => field_type(base, field, enums, structs, unions, locals),
        Expr::Deref(inner) => infer_expr_type(inner, enums, structs, unions, locals, functions)
            .pointee()
            .unwrap_or(Type::Int(IntType::U8)),
        Expr::Orelse { right, .. } => infer_expr_type(right, enums, structs, unions, locals, functions),
        Expr::StringLit(_) | Expr::DebugPrint { .. } => Type::Void,
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
        .and_then(|t| t.pointee().or(Some(t)))
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
