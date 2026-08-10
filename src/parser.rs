//! Recursive-descent parser with precedence-climbing for expressions.
//!
//! It consumes the [`Token`] stream from the lexer and produces the [`ast`]
//! tree. Newlines act as statement terminators; blank lines are ignored.

use std::rc::Rc;

use crate::ast::*;
use crate::diagnostics::Diagnostic;
use crate::lexer::Lexer;
use crate::token::{Span, StrPart, Token, TokenKind};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    /// When true, a bare `Ident {` is NOT read as a class literal. This is the
    /// classic "struct literal vs block" ambiguity: in the condition of
    /// `if x { }` or the iterable of `for every e in items { }`, the `{` must
    /// start the block, not a `Player { ... }` literal. Parentheses/brackets
    /// reset it to false so `if (Player { }).ready { }` still works.
    restrict_class_literal: bool,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Parser {
        Parser { tokens, pos: 0, restrict_class_literal: false }
    }

    pub fn parse_program(mut self) -> Result<Program, Diagnostic> {
        let mut statements = Vec::new();
        self.skip_newlines();
        while !self.at_end() {
            let stmt = self.parse_statement()?;
            statements.push(stmt);
            self.expect_terminator()?;
            self.skip_newlines();
        }
        Ok(Program { statements })
    }

    // ---- token helpers ---------------------------------------------------

    fn peek(&self) -> &TokenKind {
        &self.tokens[self.pos].kind
    }

    fn peek_span(&self) -> Span {
        self.tokens[self.pos].span
    }

    fn at_end(&self) -> bool {
        matches!(self.peek(), TokenKind::Eof)
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens[self.pos].clone();
        if !self.at_end() {
            self.pos += 1;
        }
        tok
    }

    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(self.peek()) == std::mem::discriminant(kind)
    }

    /// Consume the next token if it matches `kind`, returning whether it did.
    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: TokenKind, what: &str) -> Result<Token, Diagnostic> {
        if self.check(&kind) {
            Ok(self.advance())
        } else {
            Err(Diagnostic::new(
                self.peek_span(),
                format!("expected {}, found {}", what, describe(self.peek())),
            ))
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), TokenKind::Newline) {
            self.advance();
        }
    }

    /// After a statement we require a newline, a closing brace, or EOF.
    fn expect_terminator(&mut self) -> Result<(), Diagnostic> {
        match self.peek() {
            TokenKind::Newline | TokenKind::RBrace | TokenKind::Eof => Ok(()),
            other => Err(Diagnostic::new(
                self.peek_span(),
                format!("expected end of line, found {}", describe(other)),
            )),
        }
    }

    // ---- statements ------------------------------------------------------

    fn parse_statement(&mut self) -> Result<Stmt, Diagnostic> {
        match self.peek() {
            TokenKind::Make => self.parse_function_decl().map(Stmt::Function),
            TokenKind::Class => self.parse_class_decl(),
            TokenKind::If => self.parse_if(),
            TokenKind::While => self.parse_while(),
            TokenKind::For => self.parse_for(),
            TokenKind::Repeat => self.parse_repeat(),
            TokenKind::Loop => self.parse_loop(),
            TokenKind::Import => self.parse_import(),
            TokenKind::Game => self.parse_game(),
            TokenKind::Window => self.parse_window(),
            TokenKind::Return => self.parse_return(),
            TokenKind::Break => {
                let span = self.advance().span;
                Ok(Stmt::Break(span))
            }
            TokenKind::Continue => {
                let span = self.advance().span;
                Ok(Stmt::Continue(span))
            }
            TokenKind::Start => {
                // `start <call>` launches a background task. The async runtime
                // is a later milestone, so this parses but the interpreter
                // reports it as not-yet-supported.
                let span = self.advance().span;
                let expr = self.parse_expr()?;
                Ok(Stmt::Expr(Expr::Wait { expr: Box::new(expr), span }))
            }
            _ if self.is_change_start() => self.parse_change(),
            _ => self.parse_assign_or_expr(),
        }
    }

    /// `increase`/`decrease` are contextual words, not reserved: this only reads
    /// them as the change statement when a target name follows (so `increase = 5`
    /// or `increase(x)` still treat `increase` as an ordinary identifier).
    fn is_change_start(&self) -> bool {
        let lead = matches!(self.peek(), TokenKind::Ident(w) if w == "increase" || w == "decrease");
        lead && matches!(
            self.tokens.get(self.pos + 1).map(|t| &t.kind),
            Some(TokenKind::Ident(_)) | Some(TokenKind::SelfKw)
        )
    }

    /// `increase <place> by <amount>` / `decrease <place> by <amount>` — sugar
    /// for `<place> = <place> + <amount>` (or `-`).
    fn parse_change(&mut self) -> Result<Stmt, Diagnostic> {
        let tok = self.advance();
        let span = tok.span;
        let decrease = matches!(&tok.kind, TokenKind::Ident(w) if w == "decrease");
        let verb = if decrease { "decrease" } else { "increase" };

        let target = self.parse_postfix()?;
        if !self.eat_word("by") {
            return Err(Diagnostic::new(
                self.peek_span(),
                format!("expected `by` (write `{} {} by <amount>`)", verb, "<name>"),
            ));
        }
        let amount = self.parse_expr()?;
        let op = if decrease { BinaryOp::Sub } else { BinaryOp::Add };
        let value = Expr::Binary {
            op,
            left: Box::new(target.clone()),
            right: Box::new(amount),
            span,
        };
        Ok(Stmt::Assign { target, ty: None, value, span })
    }

    /// Consume the next token if it's the identifier `w`.
    fn eat_word(&mut self, w: &str) -> bool {
        if self.word_at(0, w) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, Diagnostic> {
        self.expect(TokenKind::LBrace, "`{`")?;
        let mut stmts = Vec::new();
        self.skip_newlines();
        while !self.check(&TokenKind::RBrace) && !self.at_end() {
            let stmt = self.parse_statement()?;
            stmts.push(stmt);
            self.expect_terminator()?;
            self.skip_newlines();
        }
        self.expect(TokenKind::RBrace, "`}` to close the block")?;
        Ok(stmts)
    }

    fn parse_function_decl(&mut self) -> Result<FunctionDecl, Diagnostic> {
        let span = self.expect(TokenKind::Make, "`make`")?.span;
        let is_async = self.eat(&TokenKind::Async);
        self.expect(TokenKind::Function, "`function`")?;
        self.expect(TokenKind::Called, "`called`")?;
        let name = self.parse_ident("a function name")?;
        self.expect(TokenKind::LParen, "`(` before parameters")?;
        let params = self.parse_params()?;
        self.expect(TokenKind::RParen, "`)` after parameters")?;
        let body = self.parse_block()?;
        Ok(FunctionDecl { name, is_async, params, body, span })
    }

    /// An anonymous function used as a value: `make function (params) { body }`
    /// — the same shape as a named function, minus `called <name>`. Handy for
    /// passing a short function to `transformed_by`, `on_click`, `after`, etc.
    fn parse_anon_function(&mut self, span: Span) -> Result<Expr, Diagnostic> {
        self.expect(TokenKind::Make, "`make`")?;
        let is_async = self.eat(&TokenKind::Async);
        self.expect(TokenKind::Function, "`function`")?;
        // Anonymous: no `called <name>` — the parentheses come next.
        self.expect(TokenKind::LParen, "`(` before parameters")?;
        let params = self.parse_params()?;
        self.expect(TokenKind::RParen, "`)` after parameters")?;
        let body = self.parse_block()?;
        let decl = FunctionDecl { name: "anonymous".to_string(), is_async, params, body, span };
        Ok(Expr::Function { decl: Rc::new(decl), span })
    }

    fn parse_params(&mut self) -> Result<Vec<Param>, Diagnostic> {
        let mut params = Vec::new();
        if self.check(&TokenKind::RParen) {
            return Ok(params);
        }
        loop {
            let span = self.peek_span();
            let name = self.parse_ident("a parameter name")?;
            let ty = if self.eat(&TokenKind::Colon) {
                Some(self.parse_type()?)
            } else {
                None
            };
            let default = if self.eat(&TokenKind::Assign) {
                Some(self.parse_expr()?)
            } else {
                None
            };
            params.push(Param { name, ty, default, span });
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        Ok(params)
    }

    fn parse_class_decl(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.expect(TokenKind::Class, "`class`")?.span;
        let name = self.parse_ident("a name for the class")?;
        self.expect(TokenKind::LBrace, "`{` to open the definition")?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        self.skip_newlines();
        while !self.check(&TokenKind::RBrace) && !self.at_end() {
            if self.check(&TokenKind::Make) {
                methods.push(self.parse_function_decl()?);
            } else {
                let fspan = self.peek_span();
                let fname = self.parse_ident("a field name")?;
                let ty = if self.eat(&TokenKind::Colon) {
                    Some(self.parse_type()?)
                } else {
                    None
                };
                let default = if self.eat(&TokenKind::Assign) {
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                if ty.is_none() && default.is_none() {
                    return Err(Diagnostic::new(
                        fspan,
                        format!("field `{}` needs a type or a default value", fname),
                    )
                    .with_hint("write `name: Text` or `name = <default>`"));
                }
                fields.push(Field { name: fname, ty, default, span: fspan });
            }
            self.expect_terminator()?;
            self.skip_newlines();
        }
        self.expect(TokenKind::RBrace, "`}` to close the definition")?;
        Ok(Stmt::Class(ClassDecl { name, fields, methods, span }))
    }

    fn parse_if(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.expect(TokenKind::If, "`if`")?.span;
        let mut branches = Vec::new();
        let cond = self.parse_condition()?;
        let body = self.parse_block()?;
        branches.push((cond, body));

        let mut else_body = None;
        while self.check(&TokenKind::Else) {
            self.advance();
            if self.eat(&TokenKind::If) {
                let cond = self.parse_condition()?;
                let body = self.parse_block()?;
                branches.push((cond, body));
            } else {
                else_body = Some(self.parse_block()?);
                break;
            }
        }
        Ok(Stmt::If { branches, else_body, span })
    }

    fn parse_while(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.expect(TokenKind::While, "`while`")?.span;
        let cond = self.parse_condition()?;
        let body = self.parse_block()?;
        Ok(Stmt::While { cond, body, span })
    }

    fn parse_for(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.expect(TokenKind::For, "`for`")?.span;
        // `every` is a plain word here (not a reserved keyword), so it stays
        // free for use as an identifier like the `every(...)` timer function.
        match self.peek().clone() {
            TokenKind::Ident(w) if w == "every" => {
                self.advance();
            }
            other => {
                return Err(Diagnostic::new(
                    self.peek_span(),
                    format!("expected `every` (loops read `for every x in ...`), found {}", describe(&other)),
                ));
            }
        }
        let var = self.parse_ident("a loop variable name")?;
        self.expect(TokenKind::In, "`in`")?;
        let iterable = self.parse_condition()?;
        let body = self.parse_block()?;
        Ok(Stmt::ForEvery { var, iterable, body, span })
    }

    fn parse_repeat(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.expect(TokenKind::Repeat, "`repeat`")?.span;
        let count = self.parse_condition()?;
        self.expect(TokenKind::Times, "`times`")?;
        let body = self.parse_block()?;
        Ok(Stmt::Repeat { count, body, span })
    }

    fn parse_loop(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.expect(TokenKind::Loop, "`loop`")?.span;
        let body = self.parse_block()?;
        Ok(Stmt::Loop { body, span })
    }

    fn parse_game(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.expect(TokenKind::Game, "`game`")?.span;
        let title = self.parse_string_literal("a game title in quotes")?;
        let props = self.parse_prop_list()?;
        self.expect(TokenKind::LBrace, "`{` to open the game block")?;

        let mut init = Vec::new();
        let mut hooks = Vec::new();
        self.skip_newlines();
        while !self.check(&TokenKind::RBrace) && !self.at_end() {
            if self.check(&TokenKind::On) {
                hooks.push(self.parse_hook()?);
            } else {
                init.push(self.parse_statement()?);
            }
            self.expect_terminator()?;
            self.skip_newlines();
        }
        self.expect(TokenKind::RBrace, "`}` to close the game block")?;
        Ok(Stmt::Game(GameDecl { title, props, init, hooks, span }))
    }

    fn parse_window(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.expect(TokenKind::Window, "`window`")?.span;
        let title = self.parse_string_literal("a window title in quotes")?;
        let props = self.parse_prop_list()?;
        self.expect(TokenKind::LBrace, "`{` to open the window block")?;
        let root = self.parse_widget_list()?;
        self.expect(TokenKind::RBrace, "`}` to close the window block")?;
        Ok(Stmt::Window(WindowDecl { title, props, root, span }))
    }

    fn parse_widget_list(&mut self) -> Result<Vec<Widget>, Diagnostic> {
        let mut widgets = Vec::new();
        self.skip_newlines();
        while !self.check(&TokenKind::RBrace) && !self.at_end() {
            widgets.push(self.parse_widget()?);
            self.expect_terminator()?;
            self.skip_newlines();
        }
        Ok(widgets)
    }

    /// `name [ "label" ] [ (props) ] [ { children } ]`
    fn parse_widget(&mut self) -> Result<Widget, Diagnostic> {
        let span = self.peek_span();
        // Widget names are usually identifiers; `list` is also a keyword, so
        // accept keyword tokens here too (same idea as `.list` member names).
        let name = if let TokenKind::Ident(n) = self.peek().clone() {
            self.advance();
            n
        } else if let Some(word) = keyword_word(self.peek()) {
            self.advance();
            word.to_string()
        } else {
            return Err(Diagnostic::new(
                span,
                format!(
                    "expected a widget name like `column`, `text`, or `list`, found {}",
                    describe(self.peek())
                ),
            ));
        };

        let label = if let TokenKind::Text(parts) = self.peek().clone() {
            self.advance();
            Some(Expr::Text(self.build_string(parts, span)?, span))
        } else {
            None
        };

        let props = self.parse_prop_list()?;

        let children = if self.check(&TokenKind::LBrace) {
            self.advance();
            let kids = self.parse_widget_list()?;
            self.expect(TokenKind::RBrace, "`}` to close the widget's children")?;
            kids
        } else {
            Vec::new()
        };

        Ok(Widget { name, label, props, children, span })
    }

    fn parse_hook(&mut self) -> Result<Hook, Diagnostic> {
        let span = self.expect(TokenKind::On, "`on`")?.span;
        let name = self.parse_ident("a hook name like `start`, `update`, or `draw`")?;
        self.expect(TokenKind::LParen, "`(` after the hook name")?;
        let mut params = Vec::new();
        if !self.check(&TokenKind::RParen) {
            loop {
                params.push(self.parse_ident("a parameter name")?);
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen, "`)` after parameters")?;
        let body = self.parse_block()?;
        Ok(Hook { name, params, body, span })
    }

    /// Parse an optional `(name: value, ...)` property list. Returns an empty
    /// vec if there are no parentheses. Newlines are allowed between props.
    fn parse_prop_list(&mut self) -> Result<Vec<(String, Expr)>, Diagnostic> {
        let mut props = Vec::new();
        if !self.eat(&TokenKind::LParen) {
            return Ok(props);
        }
        self.skip_newlines();
        if !self.check(&TokenKind::RParen) {
            let prev = self.restrict_class_literal;
            self.restrict_class_literal = false;
            let res = (|| {
                loop {
                    self.skip_newlines();
                    let name = self.parse_ident("a property name")?;
                    self.expect(TokenKind::Colon, "`:` after the property name")?;
                    let value = self.parse_expr()?;
                    props.push((name, value));
                    self.skip_newlines();
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
                Ok(())
            })();
            self.restrict_class_literal = prev;
            res?;
        }
        self.skip_newlines();
        self.expect(TokenKind::RParen, "`)` after properties")?;
        Ok(props)
    }

    fn parse_string_literal(&mut self, what: &str) -> Result<String, Diagnostic> {
        match self.peek().clone() {
            TokenKind::Text(parts) => {
                self.advance();
                // A title is a plain string; join literal parts, ignore any
                // interpolation (titles are static).
                let mut s = String::new();
                for p in parts {
                    if let StrPart::Str(chunk) = p {
                        s.push_str(&chunk);
                    }
                }
                Ok(s)
            }
            other => Err(Diagnostic::new(
                self.peek_span(),
                format!("expected {}, found {}", what, describe(&other)),
            )),
        }
    }

    fn parse_import(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.expect(TokenKind::Import, "`import`")?.span;
        // A quoted path imports another file; a bare name imports a module.
        if matches!(self.peek(), TokenKind::Text(_)) {
            let path = self.parse_string_literal("a file path in quotes")?;
            return Ok(Stmt::ImportFile { path, span });
        }
        let module = self.parse_ident("a module name like `math`, or a \"path\" in quotes")?;
        Ok(Stmt::Import { module, span })
    }

    fn parse_return(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.expect(TokenKind::Return, "`return`")?.span;
        // A bare `return` with nothing after it (end of line / block).
        let value = if matches!(self.peek(), TokenKind::Newline | TokenKind::RBrace | TokenKind::Eof) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        Ok(Stmt::Return { value, span })
    }

    /// Parse a condition/iterable expression with class-literals disabled so a
    /// trailing `{` is read as the block, not a struct literal.
    fn parse_condition(&mut self) -> Result<Expr, Diagnostic> {
        let prev = self.restrict_class_literal;
        self.restrict_class_literal = true;
        let expr = self.parse_expr();
        self.restrict_class_literal = prev;
        expr
    }

    fn parse_assign_or_expr(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.peek_span();
        let target = self.parse_expr()?;

        // Typed assignment: `name: Type = value`.
        if self.check(&TokenKind::Colon) {
            self.advance();
            let ty = self.parse_type()?;
            self.expect(TokenKind::Assign, "`=` after the type")?;
            let value = self.parse_expr()?;
            return Ok(Stmt::Assign { target, ty: Some(ty), value, span });
        }

        // Plain or compound assignment.
        let compound = match self.peek() {
            TokenKind::Assign => Some(None),
            TokenKind::PlusEq => Some(Some(BinaryOp::Add)),
            TokenKind::MinusEq => Some(Some(BinaryOp::Sub)),
            TokenKind::StarEq => Some(Some(BinaryOp::Mul)),
            TokenKind::SlashEq => Some(Some(BinaryOp::Div)),
            _ => None,
        };
        if let Some(op) = compound {
            self.advance();
            let rhs = self.parse_expr()?;
            let value = match op {
                None => rhs,
                Some(binop) => Expr::Binary {
                    op: binop,
                    left: Box::new(target.clone()),
                    right: Box::new(rhs),
                    span,
                },
            };
            return Ok(Stmt::Assign { target, ty: None, value, span });
        }

        Ok(Stmt::Expr(target))
    }

    // ---- types -----------------------------------------------------------

    fn parse_type(&mut self) -> Result<TypeAnn, Diagnostic> {
        let mut base = if self.check(&TokenKind::Dictionary) {
            self.advance();
            self.expect(TokenKind::Of, "`of` (dictionary types read `dictionary of K to V`)")?;
            let key = self.parse_type()?;
            self.expect(TokenKind::To, "`to`")?;
            let value = self.parse_type()?;
            TypeAnn::Dictionary(Box::new(key), Box::new(value))
        } else {
            let name = self.parse_ident("a type name")?;
            let optional = self.eat(&TokenKind::Question);
            TypeAnn::Named { name, optional }
        };
        // Postfix `list`, allowing `Number list list`.
        while self.check(&TokenKind::List) {
            self.advance();
            base = TypeAnn::List(Box::new(base));
        }
        Ok(base)
    }

    // ---- expressions (precedence climbing) -------------------------------

    fn parse_expr(&mut self) -> Result<Expr, Diagnostic> {
        self.parse_otherwise()
    }

    // Lowest precedence: `value otherwise fallback`, so it wraps everything else.
    fn parse_otherwise(&mut self) -> Result<Expr, Diagnostic> {
        let mut left = self.parse_or()?;
        while self.check(&TokenKind::Otherwise) {
            let span = self.advance().span;
            let right = self.parse_or()?;
            left = Expr::Otherwise { value: Box::new(left), fallback: Box::new(right), span };
        }
        Ok(left)
    }

    fn parse_or(&mut self) -> Result<Expr, Diagnostic> {
        let mut left = self.parse_and()?;
        while self.check(&TokenKind::Or) {
            let span = self.advance().span;
            let right = self.parse_and()?;
            left = Expr::Binary { op: BinaryOp::Or, left: Box::new(left), right: Box::new(right), span };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, Diagnostic> {
        let mut left = self.parse_equality()?;
        while self.check(&TokenKind::And) {
            let span = self.advance().span;
            let right = self.parse_equality()?;
            left = Expr::Binary { op: BinaryOp::And, left: Box::new(left), right: Box::new(right), span };
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr, Diagnostic> {
        let mut left = self.parse_comparison()?;
        loop {
            let op = match self.peek() {
                TokenKind::EqEq => BinaryOp::Eq,
                TokenKind::NotEq => BinaryOp::NotEq,
                TokenKind::Is => {
                    // The word forms of comparison, all built on `is`:
                    //   is / is not              →  ==  /  !=
                    //   is [not] nothing         →  the nothing check
                    //   is at least / at most    →  >=  /  <=
                    //   is more/greater than     →  >
                    //   is less/fewer than       →  <
                    let span = self.advance().span;
                    let negated = self.eat(&TokenKind::Not);
                    if self.check(&TokenKind::Nothing) {
                        self.advance();
                        left = Expr::IsNothing { expr: Box::new(left), negated, span };
                        continue;
                    }
                    let op = if negated {
                        BinaryOp::NotEq
                    } else {
                        // A word comparison (`at least`, `more than`, …) or, if
                        // none of those follow, plain equality.
                        self.word_comparison().unwrap_or(BinaryOp::Eq)
                    };
                    let right = self.parse_comparison()?;
                    left = Expr::Binary { op, left: Box::new(left), right: Box::new(right), span };
                    continue;
                }
                _ => break,
            };
            let span = self.advance().span;
            let right = self.parse_comparison()?;
            left = Expr::Binary { op, left: Box::new(left), right: Box::new(right), span };
        }
        Ok(left)
    }

    /// After `is`, try to read a word comparison operator, consuming its words.
    /// Returns `None` (consuming nothing) if what follows is a plain value, so
    /// `x is 5` still means equality. Only fires on the exact two-word phrases,
    /// so a variable named `more`/`at`/… keeps working unless `than`/`least`/
    /// `most` follows it.
    fn word_comparison(&mut self) -> Option<BinaryOp> {
        if self.word_at(0, "at") {
            if self.word_at(1, "least") {
                self.advance();
                self.advance();
                return Some(BinaryOp::GtEq);
            }
            if self.word_at(1, "most") {
                self.advance();
                self.advance();
                return Some(BinaryOp::LtEq);
            }
            return None;
        }
        let more = self.word_at(0, "more") || self.word_at(0, "greater");
        let less = self.word_at(0, "less") || self.word_at(0, "fewer");
        if (more || less) && self.word_at(1, "than") {
            self.advance();
            self.advance();
            return Some(if more { BinaryOp::Gt } else { BinaryOp::Lt });
        }
        None
    }

    /// Whether the token `offset` ahead is the identifier `w`.
    fn word_at(&self, offset: usize, w: &str) -> bool {
        match self.tokens.get(self.pos + offset) {
            Some(tok) => matches!(&tok.kind, TokenKind::Ident(name) if name == w),
            None => false,
        }
    }

    fn parse_comparison(&mut self) -> Result<Expr, Diagnostic> {
        let mut left = self.parse_term()?;
        loop {
            let op = match self.peek() {
                TokenKind::Lt => BinaryOp::Lt,
                TokenKind::LtEq => BinaryOp::LtEq,
                TokenKind::Gt => BinaryOp::Gt,
                TokenKind::GtEq => BinaryOp::GtEq,
                _ => break,
            };
            let span = self.advance().span;
            let right = self.parse_term()?;
            left = Expr::Binary { op, left: Box::new(left), right: Box::new(right), span };
        }
        Ok(left)
    }

    fn parse_term(&mut self) -> Result<Expr, Diagnostic> {
        let mut left = self.parse_factor()?;
        loop {
            let op = match self.peek() {
                TokenKind::Plus => BinaryOp::Add,
                TokenKind::Minus => BinaryOp::Sub,
                _ => break,
            };
            let span = self.advance().span;
            let right = self.parse_factor()?;
            left = Expr::Binary { op, left: Box::new(left), right: Box::new(right), span };
        }
        Ok(left)
    }

    fn parse_factor(&mut self) -> Result<Expr, Diagnostic> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                TokenKind::Star => BinaryOp::Mul,
                TokenKind::Slash => BinaryOp::Div,
                TokenKind::Percent => BinaryOp::Mod,
                _ => break,
            };
            let span = self.advance().span;
            let right = self.parse_unary()?;
            left = Expr::Binary { op, left: Box::new(left), right: Box::new(right), span };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, Diagnostic> {
        match self.peek() {
            TokenKind::Minus => {
                let span = self.advance().span;
                let expr = self.parse_unary()?;
                Ok(Expr::Unary { op: UnaryOp::Neg, expr: Box::new(expr), span })
            }
            TokenKind::Not => {
                let span = self.advance().span;
                let expr = self.parse_unary()?;
                Ok(Expr::Unary { op: UnaryOp::Not, expr: Box::new(expr), span })
            }
            TokenKind::Wait => {
                let span = self.advance().span;
                let expr = self.parse_unary()?;
                self.eat(&TokenKind::Seconds); // optional duration unit
                Ok(Expr::Wait { expr: Box::new(expr), span })
            }
            TokenKind::Try => {
                let span = self.advance().span;
                let expr = self.parse_unary()?;
                Ok(Expr::Try { expr: Box::new(expr), span })
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.peek() {
                TokenKind::Dot => {
                    let span = self.advance().span;
                    let name = self.parse_member_name()?;
                    expr = Expr::Field { object: Box::new(expr), name, span };
                }
                TokenKind::LParen => {
                    let span = self.advance().span;
                    let args = self.parse_args()?;
                    self.expect(TokenKind::RParen, "`)` after arguments")?;
                    expr = Expr::Call { callee: Box::new(expr), args, span };
                }
                TokenKind::LBracket => {
                    let span = self.advance().span;
                    let index = self.parse_bracketed(|p| p.parse_expr())?;
                    self.expect(TokenKind::RBracket, "`]` after the index")?;
                    expr = Expr::Index { object: Box::new(expr), index: Box::new(index), span };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_args(&mut self) -> Result<Vec<Expr>, Diagnostic> {
        let mut args = Vec::new();
        if self.check(&TokenKind::RParen) {
            return Ok(args);
        }
        // Inside parentheses, class literals are allowed again.
        let prev = self.restrict_class_literal;
        self.restrict_class_literal = false;
        let result = (|| {
            // Trailing `name: value` arguments are collected into one options
            // dictionary passed as the final argument, so calls can read like
            // `train(examples, answers, epochs: 5000, optimizer: adam)`.
            let mut options: Vec<(Expr, Expr)> = Vec::new();
            let mut options_span = Span::new(0, 0);
            loop {
                if let Some((name, span)) = self.peek_keyword_arg() {
                    self.advance(); // name
                    self.advance(); // colon
                    let value = self.parse_expr()?;
                    options.push((Expr::Text(vec![StrChunk::Lit(name)], span), value));
                    if options_span.line == 0 {
                        options_span = span;
                    }
                } else if options.is_empty() {
                    args.push(self.parse_expr()?);
                } else {
                    return Err(Diagnostic::new(
                        self.peek_span(),
                        "a plain argument can't come after a `name: value` argument",
                    )
                    .with_hint("put the `name: value` options last"));
                }
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            if !options.is_empty() {
                args.push(Expr::DictionaryLit { entries: options, span: options_span });
            }
            Ok(args)
        })();
        self.restrict_class_literal = prev;
        result
    }

    /// A `name:` at the current position (an argument label), without consuming.
    /// Keywords are allowed as labels too (`play_sound(beep, loop: true)`),
    /// matching how they can appear as member names after `.`.
    fn peek_keyword_arg(&self) -> Option<(String, Span)> {
        let colon_next =
            matches!(self.tokens.get(self.pos + 1).map(|t| &t.kind), Some(TokenKind::Colon));
        if !colon_next {
            return None;
        }
        match self.peek() {
            TokenKind::Ident(name) => Some((name.clone(), self.peek_span())),
            other => keyword_word(other).map(|word| (word.to_string(), self.peek_span())),
        }
    }

    /// Run `f` with class-literals re-enabled (used inside `(...)` and `[...]`).
    fn parse_bracketed<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, Diagnostic>,
    ) -> Result<T, Diagnostic> {
        let prev = self.restrict_class_literal;
        self.restrict_class_literal = false;
        let result = f(self);
        self.restrict_class_literal = prev;
        result
    }

    fn parse_primary(&mut self) -> Result<Expr, Diagnostic> {
        let span = self.peek_span();
        match self.peek().clone() {
            TokenKind::Number(n) => {
                self.advance();
                Ok(Expr::Number(n, span))
            }
            TokenKind::Bool(b) => {
                self.advance();
                Ok(Expr::Bool(b, span))
            }
            TokenKind::Nothing => {
                self.advance();
                Ok(Expr::Nothing(span))
            }
            TokenKind::SelfKw => {
                self.advance();
                Ok(Expr::SelfRef(span))
            }
            TokenKind::Text(parts) => {
                self.advance();
                let chunks = self.build_string(parts, span)?;
                Ok(Expr::Text(chunks, span))
            }
            TokenKind::Ident(name) => {
                self.advance();
                // Class literal `Name { field: value, ... }`, unless restricted.
                if self.check(&TokenKind::LBrace) && !self.restrict_class_literal {
                    return self.parse_class_literal(name, span);
                }
                Ok(Expr::Ident(name, span))
            }
            TokenKind::LParen => {
                self.advance();
                let expr = self.parse_bracketed(|p| p.parse_expr())?;
                self.expect(TokenKind::RParen, "`)`")?;
                Ok(expr)
            }
            TokenKind::LBracket => self.parse_list_literal(span),
            TokenKind::Dictionary => self.parse_dictionary_literal(span),
            TokenKind::Make => self.parse_anon_function(span),
            other => Err(Diagnostic::new(
                span,
                format!("expected a value, found {}", describe(&other)),
            )),
        }
    }

    fn parse_class_literal(&mut self, name: String, span: Span) -> Result<Expr, Diagnostic> {
        self.expect(TokenKind::LBrace, "`{`")?;
        let mut fields = Vec::new();
        self.skip_newlines();
        if !self.check(&TokenKind::RBrace) {
            let prev = self.restrict_class_literal;
            self.restrict_class_literal = false;
            let res = (|| {
                loop {
                    self.skip_newlines();
                    let fname = self.parse_ident("a field name")?;
                    self.expect(TokenKind::Colon, "`:` after the field name")?;
                    let value = self.parse_expr()?;
                    fields.push((fname, value));
                    self.skip_newlines();
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
                Ok(())
            })();
            self.restrict_class_literal = prev;
            res?;
        }
        self.skip_newlines();
        self.expect(TokenKind::RBrace, "`}` to close the value")?;
        Ok(Expr::ClassLit { name, fields, span })
    }

    fn parse_list_literal(&mut self, span: Span) -> Result<Expr, Diagnostic> {
        self.expect(TokenKind::LBracket, "`[`")?;
        let mut items = Vec::new();
        let prev = self.restrict_class_literal;
        self.restrict_class_literal = false;
        let res = (|| {
            self.skip_newlines();
            while !self.check(&TokenKind::RBracket) {
                items.push(self.parse_expr()?);
                self.skip_newlines();
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
                self.skip_newlines();
            }
            Ok(())
        })();
        self.restrict_class_literal = prev;
        res?;
        self.skip_newlines();
        self.expect(TokenKind::RBracket, "`]` to close the list")?;
        Ok(Expr::ListLit { items, span })
    }

    /// `dictionary of ... { key: value, ... }` — the type prefix is optional at the
    /// value level; we accept `dictionary { "a": 1 }` too.
    fn parse_dictionary_literal(&mut self, span: Span) -> Result<Expr, Diagnostic> {
        self.expect(TokenKind::Dictionary, "`dictionary`")?;
        // Optional `of K to V` type hint on a literal — skip it if present.
        if self.eat(&TokenKind::Of) {
            let _ = self.parse_type()?;
            self.expect(TokenKind::To, "`to`")?;
            let _ = self.parse_type()?;
        }
        self.expect(TokenKind::LBrace, "`{` to open the dictionary")?;
        let mut entries = Vec::new();
        let prev = self.restrict_class_literal;
        self.restrict_class_literal = false;
        let res = (|| {
            self.skip_newlines();
            while !self.check(&TokenKind::RBrace) {
                let key = self.parse_expr()?;
                self.expect(TokenKind::Colon, "`:` between key and value")?;
                let value = self.parse_expr()?;
                entries.push((key, value));
                self.skip_newlines();
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
                self.skip_newlines();
            }
            Ok(())
        })();
        self.restrict_class_literal = prev;
        res?;
        self.skip_newlines();
        self.expect(TokenKind::RBrace, "`}` to close the dictionary")?;
        Ok(Expr::DictionaryLit { entries, span })
    }

    /// Convert lexer string parts (with raw interpolation source) into parsed
    /// [`StrChunk`]s.
    fn build_string(&self, parts: Vec<StrPart>, span: Span) -> Result<Vec<StrChunk>, Diagnostic> {
        let mut chunks = Vec::new();
        for part in parts {
            match part {
                StrPart::Str(s) => chunks.push(StrChunk::Lit(s)),
                StrPart::Expr(raw) => {
                    let expr = parse_expr_from_str(&raw, span)?;
                    chunks.push(StrChunk::Expr(expr));
                }
            }
        }
        Ok(chunks)
    }

    fn parse_ident(&mut self, what: &str) -> Result<String, Diagnostic> {
        match self.peek().clone() {
            TokenKind::Ident(name) => {
                self.advance();
                Ok(name)
            }
            other => Err(Diagnostic::new(
                self.peek_span(),
                format!("expected {}, found {}", what, describe(&other)),
            )),
        }
    }

    /// A field or method name after `.`. Regular identifiers work, and so do
    /// keywords (`items.repeat(...)`, `x.to(...)`) — in member position a
    /// keyword is just a name, which keeps the keyword list from stealing
    /// useful method names.
    fn parse_member_name(&mut self) -> Result<String, Diagnostic> {
        if let TokenKind::Ident(name) = self.peek().clone() {
            self.advance();
            return Ok(name);
        }
        if let Some(word) = keyword_word(self.peek()) {
            self.advance();
            return Ok(word.to_string());
        }
        Err(Diagnostic::new(
            self.peek_span(),
            format!("expected a field or method name, found {}", describe(self.peek())),
        ))
    }
}

/// The source spelling of a keyword token, so keywords can be reused as member
/// names after `.`. Returns `None` for non-keyword tokens.
fn keyword_word(kind: &TokenKind) -> Option<&'static str> {
    Some(match kind {
        TokenKind::Make => "make",
        TokenKind::Function => "function",
        TokenKind::Called => "called",
        TokenKind::Async => "async",
        TokenKind::Class => "class",
        TokenKind::If => "if",
        TokenKind::Else => "else",
        TokenKind::While => "while",
        TokenKind::For => "for",
        TokenKind::In => "in",
        TokenKind::Repeat => "repeat",
        TokenKind::Times => "times",
        TokenKind::Loop => "loop",
        TokenKind::Return => "return",
        TokenKind::Break => "break",
        TokenKind::Continue => "continue",
        TokenKind::And => "and",
        TokenKind::Or => "or",
        TokenKind::Not => "not",
        TokenKind::Is => "is",
        TokenKind::Nothing => "nothing",
        TokenKind::Wait => "wait",
        TokenKind::Start => "start",
        TokenKind::List => "list",
        TokenKind::Dictionary => "dictionary",
        TokenKind::Of => "of",
        TokenKind::To => "to",
        TokenKind::Seconds => "seconds",
        TokenKind::Import => "import",
        TokenKind::Game => "game",
        TokenKind::Window => "window",
        TokenKind::On => "on",
        _ => return None,
    })
}

/// Parse a single expression out of a raw source fragment — used for the
/// `{...}` pieces of an interpolated string. The whole fragment must be one
/// expression.
fn parse_expr_from_str(src: &str, str_span: Span) -> Result<Expr, Diagnostic> {
    let tokens = Lexer::with_file(src, str_span.file).tokenize().map_err(|mut d| {
        // Point interpolation errors at the string they came from.
        d.span = str_span;
        d
    })?;
    let mut parser = Parser::new(tokens);
    parser.skip_newlines();
    if parser.at_end() {
        return Err(Diagnostic::new(str_span, "empty `{}` interpolation")
            .with_hint("put an expression inside, e.g. \"{name}\""));
    }
    let expr = parser.parse_expr().map_err(|mut d| {
        d.span = str_span;
        d
    })?;
    parser.skip_newlines();
    if !parser.at_end() {
        return Err(Diagnostic::new(str_span, "interpolation must contain a single expression"));
    }
    Ok(expr)
}

/// A short, friendly name for a token kind, for error messages.
fn describe(kind: &TokenKind) -> String {
    match kind {
        TokenKind::Number(_) => "a number".into(),
        TokenKind::Text(_) => "text".into(),
        TokenKind::Bool(_) => "a true/false value".into(),
        TokenKind::Ident(n) => format!("`{}`", n),
        TokenKind::Newline => "end of line".into(),
        TokenKind::Eof => "end of file".into(),
        TokenKind::LBrace => "`{`".into(),
        TokenKind::RBrace => "`}`".into(),
        TokenKind::LParen => "`(`".into(),
        TokenKind::RParen => "`)`".into(),
        other => format!("{:?}", other),
    }
}
