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
    Null,
    Undefined,
    Packed,
    And,
    Or,
    Bang,
    Question,
    Orelse,
    FatArrow,
    Dot,
    DotDot,
    Ellipsis,
    Ident(String),
    StringLit(String),
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
    AmpAssign,
    Pipe,
    PipeAssign,
    Caret,
    CaretAssign,
    Shl,
    ShlAssign,
    Shr,
    ShrAssign,
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
        if ch == '"' {
            return self.read_string();
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
            '&' => {
                if self.eat('=') {
                    TokenKind::AmpAssign
                } else {
                    TokenKind::Amp
                }
            }
            '|' => {
                if self.eat('=') {
                    TokenKind::PipeAssign
                } else {
                    TokenKind::Pipe
                }
            }
            '^' => {
                if self.eat('=') {
                    TokenKind::CaretAssign
                } else {
                    TokenKind::Caret
                }
            }
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
                    if self.eat('.') {
                        TokenKind::Ellipsis
                    } else {
                        TokenKind::DotDot
                    }
                } else {
                    TokenKind::Dot
                }
            }
            '<' => {
                if self.eat('<') {
                    if self.eat('=') {
                        TokenKind::ShlAssign
                    } else {
                        TokenKind::Shl
                    }
                } else if self.eat('=') {
                    TokenKind::Le
                } else {
                    TokenKind::Lt
                }
            }
            '>' => {
                if self.eat('>') {
                    if self.eat('=') {
                        TokenKind::ShrAssign
                    } else {
                        TokenKind::Shr
                    }
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

    fn read_string(&mut self) -> Result<TokenKind, String> {
        self.pos += 1; // opening "
        let mut out = String::new();
        while self.pos < self.input.len() {
            let ch = self.input[self.pos] as char;
            if ch == '"' {
                self.pos += 1;
                return Ok(TokenKind::StringLit(out));
            }
            if ch == '\\' && self.pos + 1 < self.input.len() {
                self.pos += 1;
                let esc = self.input[self.pos] as char;
                out.push(match esc {
                    'n' => '\n',
                    't' => '\t',
                    '\\' => '\\',
                    '"' => '"',
                    other => other,
                });
                self.pos += 1;
                continue;
            }
            out.push(ch);
            self.pos += 1;
        }
        Err("unterminated string literal".to_string())
    }

    fn read_int(&mut self) -> Result<TokenKind, String> {
        let start = self.pos;
        // Check for 0b (binary) or 0x (hex) prefix
        if self.input[self.pos] == b'0'
            && self.pos + 1 < self.input.len()
        {
            let next = self.input[self.pos + 1] as char;
            if next == 'b' || next == 'B' {
                self.pos += 2;
                let mut value: u64 = 0;
                let mut has_digit = false;
                while self.pos < self.input.len() {
                    match self.input[self.pos] as char {
                        '0' | '1' => {
                            value = value * 2 + (self.input[self.pos] - b'0') as u64;
                            has_digit = true;
                            self.pos += 1;
                        }
                        '_' => { self.pos += 1; }
                        _ => break,
                    }
                }
                if !has_digit {
                    return Err("empty binary literal".to_string());
                }
                return Ok(TokenKind::Int(value));
            } else if next == 'x' || next == 'X' {
                self.pos += 2;
                let mut value: u64 = 0;
                let mut has_digit = false;
                while self.pos < self.input.len() {
                    match self.input[self.pos] as char {
                        c @ '0'..='9' => {
                            value = value * 16 + (c as u64 - '0' as u64);
                            has_digit = true;
                            self.pos += 1;
                        }
                        c @ 'a'..='f' => {
                            value = value * 16 + (c as u64 - 'a' as u64 + 10);
                            has_digit = true;
                            self.pos += 1;
                        }
                        c @ 'A'..='F' => {
                            value = value * 16 + (c as u64 - 'A' as u64 + 10);
                            has_digit = true;
                            self.pos += 1;
                        }
                        '_' => { self.pos += 1; }
                        _ => break,
                    }
                }
                if !has_digit {
                    return Err("empty hex literal".to_string());
                }
                return Ok(TokenKind::Int(value));
            }
        }
        while self.pos < self.input.len()
            && ((self.input[self.pos] as char).is_ascii_digit()
                || self.input[self.pos] == b'_')
        {
            self.pos += 1;
        }
        let text: String = std::str::from_utf8(&self.input[start..self.pos])
            .unwrap()
            .chars()
            .filter(|c| *c != '_')
            .collect();
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
            "null" => TokenKind::Null,
            "undefined" => TokenKind::Undefined,
            "packed" => TokenKind::Packed,
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
