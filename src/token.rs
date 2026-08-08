//! Tokens produced by the lexer.
//!
//! A `Span` records where a token came from in the source so that every later
//! stage (parser, interpreter) can point at the exact line/column in an error.

/// A location in a source file. Lines and columns are 1-based, the way editors
/// count them, so a diagnostic can say "line 14, column 5" and match what the
/// user sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub col: usize,
}

impl Span {
    pub fn new(line: usize, col: usize) -> Span {
        Span { line, col }
    }
}

/// The kind of a token, plus any payload it carries (the actual number, the
/// text of an identifier, etc).
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    Number(f64),
    /// A string literal, already split into interpolation parts. Even a string
    /// with no `{...}` is one `Text` part. `"Hi {name}"` becomes
    /// `[Str("Hi "), Expr("name")]`.
    Text(Vec<StrPart>),
    Bool(bool),

    // Identifier (variable / function / class name)
    Ident(String),

    // Keywords
    Make,
    Function,
    Called,
    Async,
    Class,
    If,
    Else,
    While,
    For,
    In,
    Repeat,
    Times,
    Loop,
    Return,
    Break,
    Continue,
    And,
    Or,
    Not,
    Is,
    Nothing,
    Wait,
    Start,
    SelfKw,
    List,
    Dictionary,
    Of,
    To,
    Seconds,
    Import,
    Game,
    Window,
    On,

    // Punctuation & operators
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Dot,
    Question,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Assign,     // =
    PlusEq,     // +=
    MinusEq,    // -=
    StarEq,     // *=
    SlashEq,    // /=
    EqEq,       // ==
    NotEq,      // !=
    Lt,
    LtEq,
    Gt,
    GtEq,

    /// End of a logical line (statement separator).
    Newline,
    /// End of file.
    Eof,
}

/// One piece of a (possibly interpolated) string literal.
#[derive(Debug, Clone, PartialEq)]
pub enum StrPart {
    /// A literal chunk of text.
    Str(String),
    /// The raw source text of a `{...}` expression, to be parsed later.
    Expr(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Token {
        Token { kind, span }
    }
}
