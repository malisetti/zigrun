use crate::ast::{BinOp, Expr, MainFn, Program, Stmt};
use crate::lexer::{Token, TokenKind};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    pub fn parse_program(mut self) -> Result<Program, String> {
        self.expect(TokenKind::Pub)?;
        self.expect(TokenKind::Fn)?;
        self.expect(TokenKind::Main)?;
        self.expect(TokenKind::LParen)?;
        self.expect(TokenKind::RParen)?;
        self.expect(TokenKind::U8)?;
        self.expect(TokenKind::LBrace)?;
        let body = self.parse_stmt()?;
        self.expect(TokenKind::RBrace)?;
        self.expect(TokenKind::Eof)?;
        Ok(Program {
            main: MainFn { body },
        })
    }

    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        self.expect(TokenKind::Return)?;
        let expr = self.parse_expr()?;
        self.expect(TokenKind::Semicolon)?;
        Ok(Stmt::Return(expr))
    }

    fn parse_expr(&mut self) -> Result<Expr, String> {
        let left = self.parse_primary()?;
        if self.check(&TokenKind::Plus) {
            self.advance();
            let right = self.parse_primary()?;
            return Ok(Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(left),
                right: Box::new(right),
            });
        }
        Ok(left)
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.peek_kind() {
            TokenKind::Int(n) => {
                self.advance();
                Ok(Expr::Int(n))
            }
            other => Err(format!("expected integer literal, found {other:?}")),
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Result<(), String> {
        if self.check(&kind) {
            self.advance();
            Ok(())
        } else {
            Err(format!(
                "expected {:?}, found {:?}",
                kind,
                self.peek_kind()
            ))
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
    use crate::ast::Expr;
    use crate::lexer::Lexer;

    #[test]
    fn parses_add_program() {
        let src = "pub fn main() u8 { return 3 + 4; }";
        let tokens = Lexer::new(src).tokenize().unwrap();
        let program = Parser::new(tokens).parse_program().unwrap();
        match program.main.body {
            Stmt::Return(Expr::BinOp { op: _, left, right }) => {
                assert_eq!(*left, Expr::Int(3));
                assert_eq!(*right, Expr::Int(4));
            }
            other => panic!("unexpected stmt: {other:?}"),
        }
    }
}
