//! The Abstract Syntax Tree: the parser's output, and what the interpreter
//! walks. Every node carries a `Span` so runtime errors can point at source.

// Several fields here (type annotations, `is_async`, spans on some nodes) are
// parsed and stored now but only consumed once the type checker milestone
// lands. Silence the "never read" warnings until then.
#![allow(dead_code)]

use crate::token::Span;

/// A whole `.pt` file: a flat list of top-level statements.
#[derive(Debug, Clone)]
pub struct Program {
    pub statements: Vec<Stmt>,
}

/// One piece of an interpolated string, with any `{...}` already parsed into a
/// real expression.
#[derive(Debug, Clone)]
pub enum StrChunk {
    Lit(String),
    Expr(Expr),
}

/// A written-out type annotation, e.g. `Number`, `Text?`, `Enemy list`,
/// `dictionary of Text to Number`. In this milestone the interpreter mostly ignores
/// these (checking happens at runtime); they are kept so the future type
/// checker can use them without re-parsing.
#[derive(Debug, Clone)]
pub enum TypeAnn {
    Named { name: String, optional: bool },
    List(Box<TypeAnn>),
    Dictionary(Box<TypeAnn>, Box<TypeAnn>),
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Option<TypeAnn>,
    pub default: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FunctionDecl {
    pub name: String,
    pub is_async: bool,
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub ty: Option<TypeAnn>,
    pub default: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ClassDecl {
    pub name: String,
    pub fields: Vec<Field>,
    pub methods: Vec<FunctionDecl>,
    pub span: Span,
}

/// A lifecycle callback inside a `game`/`window` block, e.g. `on update(delta)`.
#[derive(Debug, Clone)]
pub struct Hook {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

/// A top-level `game "Title" (props) { init...; on ...() {} }` block.
#[derive(Debug, Clone)]
pub struct GameDecl {
    pub title: String,
    pub props: Vec<(String, Expr)>,
    /// Statements in the block body that aren't hooks — run once at start to
    /// set up game state.
    pub init: Vec<Stmt>,
    pub hooks: Vec<Hook>,
    pub span: Span,
}

/// One node of a declarative UI tree, e.g.
/// `button "Click me" (on_click: handle_click)`.
#[derive(Debug, Clone)]
pub struct Widget {
    pub name: String,
    /// The optional quoted label; may interpolate, re-evaluated each frame.
    pub label: Option<Expr>,
    pub props: Vec<(String, Expr)>,
    pub children: Vec<Widget>,
    pub span: Span,
}

/// A top-level `window "Title" (props) { <widgets> }` block.
#[derive(Debug, Clone)]
pub struct WindowDecl {
    pub title: String,
    pub props: Vec<(String, Expr)>,
    pub root: Vec<Widget>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    /// `name = value` or `name: Type = value` or `target.field = value`.
    Assign {
        target: Expr,
        ty: Option<TypeAnn>,
        value: Expr,
        span: Span,
    },
    Function(FunctionDecl),
    Class(ClassDecl),
    If {
        branches: Vec<(Expr, Vec<Stmt>)>,
        else_body: Option<Vec<Stmt>>,
        span: Span,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    /// `for every <name> in <iterable> { ... }`
    ForEvery {
        var: String,
        iterable: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    /// `repeat <count> times { ... }`
    Repeat {
        count: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    Loop {
        body: Vec<Stmt>,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
    Break(Span),
    Continue(Span),
    Game(GameDecl),
    Window(WindowDecl),
    /// A bare expression evaluated for its side effects (e.g. a function call).
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub enum Expr {
    Number(f64, Span),
    /// Interpolated string parts. Non-interpolated strings are a single part.
    Text(Vec<StrChunk>, Span),
    Bool(bool, Span),
    Nothing(Span),
    /// A reference to `self` inside a method.
    SelfRef(Span),
    Ident(String, Span),

    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
        span: Span,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    /// `expr is nothing` / `expr is not nothing`.
    IsNothing {
        expr: Box<Expr>,
        negated: bool,
        span: Span,
    },

    /// A function call: `callee(args)`.
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    /// Field or method access: `object.name`.
    Field {
        object: Box<Expr>,
        name: String,
        span: Span,
    },
    /// Index: `object[index]`.
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },

    ListLit {
        items: Vec<Expr>,
        span: Span,
    },
    DictionaryLit {
        entries: Vec<(Expr, Expr)>,
        span: Span,
    },
    /// `ClassName { field: value, ... }`
    ClassLit {
        name: String,
        fields: Vec<(String, Expr)>,
        span: Span,
    },

    /// `wait <expr>` — evaluates in this milestone as a no-op placeholder
    /// (the async runtime arrives in a later milestone).
    Wait {
        expr: Box<Expr>,
        span: Span,
    },
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Number(_, s)
            | Expr::Text(_, s)
            | Expr::Bool(_, s)
            | Expr::Nothing(s)
            | Expr::SelfRef(s)
            | Expr::Ident(_, s)
            | Expr::Unary { span: s, .. }
            | Expr::Binary { span: s, .. }
            | Expr::IsNothing { span: s, .. }
            | Expr::Call { span: s, .. }
            | Expr::Field { span: s, .. }
            | Expr::Index { span: s, .. }
            | Expr::ListLit { span: s, .. }
            | Expr::DictionaryLit { span: s, .. }
            | Expr::ClassLit { span: s, .. }
            | Expr::Wait { span: s, .. } => *s,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOp {
    Neg, // -x
    Not, // not x
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    And,
    Or,
}
