#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Pub,
    Fn,
    Main,
    U8,
    Return,
    Int(u8),
    Plus,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Semicolon,
    Eof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
}

pub struct Lexer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    pub fn tokenize(mut self) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();
        loop {
            let kind = self.next_token()?;
            let is_eof = kind == TokenKind::Eof;
            tokens.push(Token { kind });
            if is_eof {
                break;
            }
        }
        Ok(tokens)
    }

    fn next_token(&mut self) -> Result<TokenKind, String> {
        self.skip_whitespace();
        if self.pos >= self.input.len() {
            return Ok(TokenKind::Eof);
        }

        let ch = self.peek_char().unwrap();

        if ch.is_ascii_digit() {
            return self.read_int();
        }

        match ch {
            '+' => {
                self.pos += 1;
                Ok(TokenKind::Plus)
            }
            '(' => {
                self.pos += 1;
                Ok(TokenKind::LParen)
            }
            ')' => {
                self.pos += 1;
                Ok(TokenKind::RParen)
            }
            '{' => {
                self.pos += 1;
                Ok(TokenKind::LBrace)
            }
            '}' => {
                self.pos += 1;
                Ok(TokenKind::RBrace)
            }
            ';' => {
                self.pos += 1;
                Ok(TokenKind::Semicolon)
            }
            _ if ch.is_ascii_alphabetic() => self.read_ident(),
            _ => Err(format!("unexpected character {:?}", ch)),
        }
    }

    fn read_int(&mut self) -> Result<TokenKind, String> {
        let start = self.pos;
        while self.pos < self.input.len() && self.peek_char().unwrap().is_ascii_digit() {
            self.pos += 1;
        }
        let text = &self.input[start..self.pos];
        let value: u8 = text
            .parse()
            .map_err(|_| format!("integer literal out of range: {text}"))?;
        Ok(TokenKind::Int(value))
    }

    fn read_ident(&mut self) -> Result<TokenKind, String> {
        let start = self.pos;
        while self.pos < self.input.len() {
            let ch = self.peek_char().unwrap();
            if ch.is_ascii_alphanumeric() || ch == '_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let word = &self.input[start..self.pos];
        Ok(match word {
            "pub" => TokenKind::Pub,
            "fn" => TokenKind::Fn,
            "main" => TokenKind::Main,
            "u8" => TokenKind::U8,
            "return" => TokenKind::Return,
            other => return Err(format!("unknown identifier: {other}")),
        })
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() {
            let ch = self.peek_char().unwrap();
            if ch.is_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_add_fixture() {
        let src = "pub fn main() u8 { return 3 + 4; }";
        let tokens = Lexer::new(src).tokenize().unwrap();
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Int(3)));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Plus));
    }
}
