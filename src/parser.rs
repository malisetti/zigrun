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
// multiplicative := primary ((*|/|%) primary)*
// primary     := int | ident ("(" args? ")")? | "(" expr ")"
//              | "switch" "(" expr ")" "{" ( int "=>" expr "," )* "else" "=>" expr "}"
//              | "@intCast" "(" expr ")"

use crate::ast::{BinOp, Expr, Function, IntType, Program, Stmt};
use crate::lexer::{Token, TokenKind};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    return_type: IntType,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            return_type: IntType::U8,
        }
    }

    pub fn parse_program(mut self) -> Result<Program, String> {
        let mut functions = Vec::new();
        while !self.check(&TokenKind::Eof) {
            functions.push(self.parse_function()?);
        }
        if functions.is_empty() {
            return Err("empty program: no functions".to_string());
        }
        Ok(Program { functions })
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
        self.return_type = return_type;
        let body = self.parse_block()?;
        Ok(Function {
            name,
            params,
            return_type,
            body,
        })
    }

    fn parse_type(&mut self) -> Result<IntType, String> {
        let name = self.expect_ident()?;
        IntType::from_name(&name)
            .ok_or_else(|| format!("unsupported type {name:?} (expected u8, u16, or u32)"))
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
                self.expect(TokenKind::Colon)?;
                let ty = self.parse_type()?;
                self.expect(TokenKind::Assign)?;
                let value = self.parse_expr()?;
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
                let start = self.parse_expr()?;
                self.expect(TokenKind::DotDot)?;
                let end = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
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
                let body = self.parse_block()?;
                Ok(Stmt::For {
                    capture,
                    start,
                    end,
                    body,
                })
            }
            TokenKind::Ident(name) => {
                self.advance();
                self.expect(TokenKind::Assign)?;
                let value = self.parse_expr()?;
                self.expect(TokenKind::Semicolon)?;
                Ok(Stmt::Assign { name, value })
            }
            other => Err(format!("unexpected token at start of statement: {other:?}")),
        }
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
        let mut left = self.parse_primary()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_primary()?;
            left = Expr::BinOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.peek_kind() {
            TokenKind::At => {
                self.advance();
                let builtin = self.expect_ident()?;
                if builtin != "intCast" {
                    return Err(format!("unsupported builtin @{builtin}"));
                }
                self.expect(TokenKind::LParen)?;
                let expr = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                Ok(Expr::IntCast {
                    expr: Box::new(expr),
                    target: self.return_type,
                })
            }
            TokenKind::Int(n) => {
                self.advance();
                Ok(Expr::Int(n))
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
                } else {
                    Ok(Expr::Var(name))
                }
            }
            other => Err(format!("expected an expression, found {other:?}")),
        }
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
                default = Some(self.parse_expr()?);
            } else {
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
                arms.push((val, self.parse_expr()?));
            }
            if self.check(&TokenKind::Comma) {
                self.advance();
            }
        }
        self.expect(TokenKind::RBrace)?;
        let default = default.ok_or_else(|| "switch missing `else =>` arm".to_string())?;
        Ok(Expr::Switch {
            scrutinee: Box::new(scrutinee),
            arms,
            default: Box::new(default),
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
        assert_eq!(prog.functions[0].params, vec![("n".to_string(), IntType::U8)]);
    }
}
