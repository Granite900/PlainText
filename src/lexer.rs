//! The lexer turns raw `.pt` source text into a flat list of [`Token`]s.
//!
//! Rust note: we work over `Vec<char>` (not raw bytes) so column counting and
//! Unicode identifiers behave sensibly. `self.pos` is an index into that vec.

use crate::diagnostics::Diagnostic;
use crate::token::{Span, StrPart, Token, TokenKind};

pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Lexer {
        // Editors on Windows often save UTF-8 with a leading byte-order mark
        // (U+FEFF). Skip it so those files lex cleanly.
        let source = source.strip_prefix('\u{feff}').unwrap_or(source);
        Lexer { chars: source.chars().collect(), pos: 0, line: 1, col: 1 }
    }

    /// Tokenize the whole input, or fail on the first lexical error.
    pub fn tokenize(mut self) -> Result<Vec<Token>, Diagnostic> {
        let mut tokens = Vec::new();
        loop {
            let tok = self.next_token()?;
            let is_eof = tok.kind == TokenKind::Eof;
            tokens.push(tok);
            if is_eof {
                break;
            }
        }
        Ok(tokens)
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek2(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    /// Consume and return the current char, advancing line/col bookkeeping.
    fn advance(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn here(&self) -> Span {
        Span::new(self.line, self.col)
    }

    fn next_token(&mut self) -> Result<Token, Diagnostic> {
        self.skip_inline_whitespace_and_comments()?;

        let span = self.here();
        let c = match self.peek() {
            None => return Ok(Token::new(TokenKind::Eof, span)),
            Some(c) => c,
        };

        // Newlines are significant: they separate statements.
        if c == '\n' {
            self.advance();
            return Ok(Token::new(TokenKind::Newline, span));
        }

        if c.is_ascii_digit() {
            return self.lex_number(span);
        }
        if c == '"' {
            return self.lex_text(span);
        }
        if is_ident_start(c) {
            return Ok(self.lex_ident_or_keyword(span));
        }

        self.lex_symbol(span)
    }

    /// Skip spaces, tabs, carriage returns and both comment styles. Newlines
    /// are NOT skipped here — they are meaningful tokens.
    fn skip_inline_whitespace_and_comments(&mut self) -> Result<(), Diagnostic> {
        loop {
            match self.peek() {
                Some(' ') | Some('\t') | Some('\r') => {
                    self.advance();
                }
                Some('/') if self.peek2() == Some('/') => {
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        self.advance();
                    }
                }
                Some('/') if self.peek2() == Some('*') => {
                    let start = self.here();
                    self.advance();
                    self.advance();
                    loop {
                        match self.peek() {
                            None => {
                                return Err(Diagnostic::new(
                                    start,
                                    "unterminated block comment (missing `*/`)",
                                ));
                            }
                            Some('*') if self.peek2() == Some('/') => {
                                self.advance();
                                self.advance();
                                break;
                            }
                            _ => {
                                self.advance();
                            }
                        }
                    }
                }
                _ => break,
            }
        }
        Ok(())
    }

    fn lex_number(&mut self, span: Span) -> Result<Token, Diagnostic> {
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                text.push(c);
                self.advance();
            } else if c == '.' && self.peek2().map_or(false, |d| d.is_ascii_digit()) {
                text.push(c);
                self.advance();
                while let Some(d) = self.peek() {
                    if d.is_ascii_digit() {
                        text.push(d);
                        self.advance();
                    } else {
                        break;
                    }
                }
                break;
            } else {
                break;
            }
        }
        let value: f64 = text.parse().map_err(|_| {
            Diagnostic::new(span, format!("`{}` is not a valid number", text))
        })?;
        Ok(Token::new(TokenKind::Number(value), span))
    }

    /// Lex a `"..."` string, splitting `{expr}` interpolations into parts and
    /// resolving escape sequences.
    fn lex_text(&mut self, span: Span) -> Result<Token, Diagnostic> {
        self.advance(); // opening quote
        let mut parts: Vec<StrPart> = Vec::new();
        let mut current = String::new();

        loop {
            let c = match self.peek() {
                None => {
                    return Err(Diagnostic::new(span, "unterminated string (missing closing `\"`)"));
                }
                Some(c) => c,
            };

            match c {
                '"' => {
                    self.advance();
                    break;
                }
                '\\' => {
                    self.advance();
                    let esc = self.advance().ok_or_else(|| {
                        Diagnostic::new(self.here(), "unfinished escape sequence")
                    })?;
                    match esc {
                        'n' => current.push('\n'),
                        't' => current.push('\t'),
                        'r' => current.push('\r'),
                        '"' => current.push('"'),
                        '\\' => current.push('\\'),
                        '{' => current.push('{'),
                        '}' => current.push('}'),
                        other => {
                            return Err(Diagnostic::new(
                                self.here(),
                                format!("unknown escape `\\{}`", other),
                            )
                            .with_hint("valid escapes are \\n \\t \\r \\\" \\\\ \\{ \\}"));
                        }
                    }
                }
                '{' => {
                    // Flush the literal chunk collected so far.
                    if !current.is_empty() {
                        parts.push(StrPart::Str(std::mem::take(&mut current)));
                    }
                    let expr_src = self.lex_interpolation(span)?;
                    parts.push(StrPart::Expr(expr_src));
                }
                '}' => {
                    return Err(Diagnostic::new(
                        self.here(),
                        "unexpected `}` in string",
                    )
                    .with_hint("write `\\}` for a literal closing brace"));
                }
                _ => {
                    current.push(c);
                    self.advance();
                }
            }
        }

        if !current.is_empty() || parts.is_empty() {
            parts.push(StrPart::Str(current));
        }
        Ok(Token::new(TokenKind::Text(parts), span))
    }

    /// Read the raw source inside a `{...}` interpolation, balancing nested
    /// braces. Returns the inner text (without the outer braces) for the
    /// parser to lex and parse on its own.
    fn lex_interpolation(&mut self, str_span: Span) -> Result<String, Diagnostic> {
        self.advance(); // opening {
        let mut depth = 1;
        let mut src = String::new();
        loop {
            let c = match self.peek() {
                None => {
                    return Err(Diagnostic::new(
                        str_span,
                        "unterminated interpolation (missing `}`)",
                    ));
                }
                Some(c) => c,
            };
            match c {
                '{' => {
                    depth += 1;
                    src.push(c);
                    self.advance();
                }
                '}' => {
                    depth -= 1;
                    self.advance();
                    if depth == 0 {
                        break;
                    }
                    src.push('}');
                }
                _ => {
                    src.push(c);
                    self.advance();
                }
            }
        }
        Ok(src)
    }

    fn lex_ident_or_keyword(&mut self, span: Span) -> Token {
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if is_ident_continue(c) {
                text.push(c);
                self.advance();
            } else {
                break;
            }
        }
        let kind = match text.as_str() {
            "make" => TokenKind::Make,
            "function" => TokenKind::Function,
            "called" => TokenKind::Called,
            "async" => TokenKind::Async,
            "class" => TokenKind::Class,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "while" => TokenKind::While,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "repeat" => TokenKind::Repeat,
            "times" => TokenKind::Times,
            "loop" => TokenKind::Loop,
            "return" => TokenKind::Return,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "and" => TokenKind::And,
            "or" => TokenKind::Or,
            "not" => TokenKind::Not,
            "is" => TokenKind::Is,
            "nothing" => TokenKind::Nothing,
            "true" => TokenKind::Bool(true),
            "false" => TokenKind::Bool(false),
            "wait" => TokenKind::Wait,
            "start" => TokenKind::Start,
            "self" => TokenKind::SelfKw,
            "list" => TokenKind::List,
            "dictionary" => TokenKind::Dictionary,
            "of" => TokenKind::Of,
            "to" => TokenKind::To,
            "second" | "seconds" => TokenKind::Seconds,
            "import" => TokenKind::Import,
            "game" => TokenKind::Game,
            "window" => TokenKind::Window,
            "on" => TokenKind::On,
            _ => TokenKind::Ident(text),
        };
        Token::new(kind, span)
    }

    fn lex_symbol(&mut self, span: Span) -> Result<Token, Diagnostic> {
        let c = self.advance().unwrap();
        let kind = match c {
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            ',' => TokenKind::Comma,
            ':' => TokenKind::Colon,
            '.' => TokenKind::Dot,
            '?' => TokenKind::Question,
            '%' => TokenKind::Percent,
            '+' => self.maybe_eq(TokenKind::PlusEq, TokenKind::Plus),
            '-' => self.maybe_eq(TokenKind::MinusEq, TokenKind::Minus),
            '*' => self.maybe_eq(TokenKind::StarEq, TokenKind::Star),
            '/' => self.maybe_eq(TokenKind::SlashEq, TokenKind::Slash),
            '=' => self.maybe_eq(TokenKind::EqEq, TokenKind::Assign),
            '<' => self.maybe_eq(TokenKind::LtEq, TokenKind::Lt),
            '>' => self.maybe_eq(TokenKind::GtEq, TokenKind::Gt),
            '!' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::NotEq
                } else {
                    return Err(Diagnostic::new(span, "unexpected `!`")
                        .with_hint("use the word `not` for logical negation, or `!=` for `is not equal`"));
                }
            }
            other => {
                return Err(Diagnostic::new(
                    span,
                    format!("unexpected character `{}`", other),
                ));
            }
        };
        Ok(Token::new(kind, span))
    }

    /// If the next char is `=`, consume it and return `two`; otherwise `one`.
    fn maybe_eq(&mut self, two: TokenKind, one: TokenKind) -> TokenKind {
        if self.peek() == Some('=') {
            self.advance();
            two
        } else {
            one
        }
    }
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

fn is_ident_continue(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}
