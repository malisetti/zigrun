// Lexer for the zigrun Zig subset. ASCII source; identifiers are generic
// (keywords recognized by table), and operators include the two-char forms
// `<=`, `>=`, `==`, `!=`. `//` line comments are skipped.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Pub,
    Fn,
    Return,
    Const,
    Enum,
    Error,
    Union,
    Try,
    Catch,
    Struct,
    Void,
    Var,
    If,
    Else,
    While,
    For,
    Break,
    Continue,
    Switch,
    Bool,
    True,
    False,
    Undefined,
    And,
    Or,
    Bang,
    Question,
    Orelse,
    FatArrow,
    Dot,
    DotDot,
    Ident(String),
    Int(u64),
    At,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Lt,
    Gt,
    Le,
    Ge,
    EqEq,
    Ne,
    Assign,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    PercentAssign,
    Amp,
    Pipe,
    Caret,
    Shl,
    Shr,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Semicolon,
    Eof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
}

pub struct Lexer<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            pos: 0,
        }
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
        self.skip_trivia();
        if self.pos >= self.input.len() {
            return Ok(TokenKind::Eof);
        }
        let ch = self.input[self.pos] as char;

        if ch.is_ascii_digit() {
            return self.read_int();
        }
        if ch.is_ascii_alphabetic() || ch == '_' {
            return Ok(self.read_ident());
        }

        self.pos += 1; // consume the operator/punctuation char
        let kind = match ch {
            '=' => {
                if self.eat('=') {
                    TokenKind::EqEq
                } else if self.eat('>') {
                    TokenKind::FatArrow
                } else {
                    TokenKind::Assign
                }
            }
            '+' => {
                if self.eat('=') {
                    TokenKind::PlusAssign
                } else {
                    TokenKind::Plus
                }
            }
            '-' => {
                if self.eat('=') {
                    TokenKind::MinusAssign
                } else {
                    TokenKind::Minus
                }
            }
            '*' => {
                if self.eat('=') {
                    TokenKind::StarAssign
                } else {
                    TokenKind::Star
                }
            }
            '/' => {
                if self.eat('=') {
                    TokenKind::SlashAssign
                } else {
                    TokenKind::Slash
                }
            }
            '%' => {
                if self.eat('=') {
                    TokenKind::PercentAssign
                } else {
                    TokenKind::Percent
                }
            }
            '&' => TokenKind::Amp,
            '|' => TokenKind::Pipe,
            '^' => TokenKind::Caret,
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            ',' => TokenKind::Comma,
            ':' => TokenKind::Colon,
            ';' => TokenKind::Semicolon,
            '@' => TokenKind::At,
            '.' => {
                if self.eat('.') {
                    TokenKind::DotDot
                } else {
                    TokenKind::Dot
                }
            }
            '<' => {
                if self.eat('<') {
                    TokenKind::Shl
                } else if self.eat('=') {
                    TokenKind::Le
                } else {
                    TokenKind::Lt
                }
            }
            '>' => {
                if self.eat('>') {
                    TokenKind::Shr
                } else if self.eat('=') {
                    TokenKind::Ge
                } else {
                    TokenKind::Gt
                }
            }
            '!' => {
                if self.eat('=') {
                    TokenKind::Ne
                } else {
                    TokenKind::Bang
                }
            }
            '?' => TokenKind::Question,
            other => return Err(format!("unexpected character {other:?}")),
        };
        Ok(kind)
    }

    fn read_int(&mut self) -> Result<TokenKind, String> {
        let start = self.pos;
        while self.pos < self.input.len() && (self.input[self.pos] as char).is_ascii_digit() {
            self.pos += 1;
        }
        let text = std::str::from_utf8(&self.input[start..self.pos]).unwrap();
        let value: u64 = text
            .parse()
            .map_err(|_| format!("integer literal out of u64 range: {text}"))?;
        Ok(TokenKind::Int(value))
    }

    fn read_ident(&mut self) -> TokenKind {
        let start = self.pos;
        while self.pos < self.input.len() {
            let ch = self.input[self.pos] as char;
            if ch.is_ascii_alphanumeric() || ch == '_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let word = std::str::from_utf8(&self.input[start..self.pos]).unwrap();
        match word {
            "pub" => TokenKind::Pub,
            "fn" => TokenKind::Fn,
            "return" => TokenKind::Return,
            "const" => TokenKind::Const,
            "enum" => TokenKind::Enum,
            "error" => TokenKind::Error,
            "union" => TokenKind::Union,
            "try" => TokenKind::Try,
            "catch" => TokenKind::Catch,
            "struct" => TokenKind::Struct,
            "void" => TokenKind::Void,
            "var" => TokenKind::Var,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "while" => TokenKind::While,
            "for" => TokenKind::For,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "switch" => TokenKind::Switch,
            "bool" => TokenKind::Bool,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "undefined" => TokenKind::Undefined,
            "and" => TokenKind::And,
            "or" => TokenKind::Or,
            "orelse" => TokenKind::Orelse,
            other => TokenKind::Ident(other.to_string()),
        }
    }

    /// Consume the next byte iff it equals `c`.
    fn eat(&mut self, c: char) -> bool {
        if self.pos < self.input.len() && self.input[self.pos] as char == c {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn skip_trivia(&mut self) {
        loop {
            while self.pos < self.input.len() && (self.input[self.pos] as char).is_whitespace() {
                self.pos += 1;
            }
            // `//` line comment
            if self.pos + 1 < self.input.len()
                && self.input[self.pos] == b'/'
                && self.input[self.pos + 1] == b'/'
            {
                while self.pos < self.input.len() && self.input[self.pos] != b'\n' {
                    self.pos += 1;
                }
                continue;
            }
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_keywords_and_ops() {
        let toks = Lexer::new("fn fib(n: u8) u8 { if (n < 2) return n; }")
            .tokenize()
            .unwrap();
        assert!(toks.iter().any(|t| t.kind == TokenKind::Fn));
        assert!(toks.iter().any(|t| t.kind == TokenKind::Lt));
        assert!(toks.iter().any(|t| t.kind == TokenKind::Ident("fib".into())));
    }
}
