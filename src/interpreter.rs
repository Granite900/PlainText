//! A tree-walking interpreter for the core synchronous language.
//!
//! It executes the AST directly. Values live behind the `Rc<RefCell<_>>`
//! bootstrap heap from `value.rs`; a real garbage collector replaces that in a
//! later milestone without changing this file's logic.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::ast::*;
use crate::diagnostics::Diagnostic;
use crate::gfx::{Color, DrawCmd, GfxBridge};
use crate::token::Span;
use crate::ui::{Align, UiKind, UiNode};
use crate::value::*;

/// Result of executing a statement: either fall through, or a jump.
enum Flow {
    Normal,
    Break,
    Continue,
    Return(Value),
}

pub struct Interpreter {
    globals: Env,
    classes: HashMap<String, Rc<ClassDef>>,
    rng_state: Cell<u64>,
    start: Instant,
    /// Present only while a `game` block is running; graphics/input builtins
    /// use it. `None` for plain console programs.
    gfx: Option<Rc<RefCell<GfxBridge>>>,
    /// Pending timers scheduled with `after`/`every`.
    timers: Vec<Timer>,
}

/// A scheduled callback. `interval` is `Some` for `every` (repeats) and `None`
/// for `after` (fires once).
struct Timer {
    remaining: f64,
    interval: Option<f64>,
    callback: Value,
}

type EvalResult = Result<Value, Diagnostic>;
type ExecResult = Result<Flow, Diagnostic>;

impl Interpreter {
    pub fn new() -> Interpreter {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E3779B9)
            | 1;
        let globals = Scope::new_global();
        // Math constants available as ordinary global values.
        env_declare(&globals, "pi", Value::Number(std::f64::consts::PI));
        env_declare(&globals, "e", Value::Number(std::f64::consts::E));
        // Named colors as [r, g, b] lists.
        for (name, (r, g, b)) in crate::gfx::named_colors() {
            let list = vec![
                Value::Number(*r as f64),
                Value::Number(*g as f64),
                Value::Number(*b as f64),
            ];
            env_declare(&globals, name, Value::List(Rc::new(RefCell::new(list))));
        }
        // Alignment words (`center`, `left`, …) as Text, for `align: center`.
        for word in crate::ui::align_words() {
            env_declare(&globals, word, Value::text(*word));
        }
        Interpreter {
            globals,
            classes: HashMap::new(),
            rng_state: Cell::new(seed),
            start: Instant::now(),
            gfx: None,
            timers: Vec::new(),
        }
    }

    /// Advance all timers by `dt` seconds, firing any that come due. Called once
    /// per frame by the game runner (and by the console driver). Callbacks may
    /// schedule new timers, which are preserved.
    pub fn tick_timers(&mut self, dt: f64) -> Result<(), Diagnostic> {
        let due = std::mem::take(&mut self.timers);
        let mut keep = Vec::new();
        for mut t in due {
            t.remaining -= dt;
            if t.remaining <= 0.0 {
                self.call_value(t.callback.clone(), Vec::new(), Span::new(0, 0))?;
                if let Some(iv) = t.interval {
                    t.remaining += iv; // reschedule (accumulate to limit drift)
                    keep.push(t);
                }
                // one-shot timers simply drop
            } else {
                keep.push(t);
            }
        }
        // Timers scheduled by the callbacks above landed in self.timers.
        keep.append(&mut self.timers);
        self.timers = keep;
        Ok(())
    }

    pub fn has_timers(&self) -> bool {
        !self.timers.is_empty()
    }

    // ---- game runner support (used by game.rs) ---------------------------

    pub fn set_gfx(&mut self, bridge: Rc<RefCell<GfxBridge>>) {
        self.gfx = Some(bridge);
    }

    /// Hoist declarations, run top-level statements and the game block's init
    /// statements, and return the scope holding the game's state.
    pub fn prepare_game(&mut self, program: &Program, game: &GameDecl) -> Result<Env, Diagnostic> {
        self.hoist(&program.statements)?;
        for stmt in &program.statements {
            if matches!(stmt, Stmt::Function(_) | Stmt::Class(_) | Stmt::Game(_)) {
                continue;
            }
            self.exec_stmt(stmt, &self.globals.clone())?;
        }
        let scope = Scope::new_child(&self.globals);
        for stmt in &game.init {
            self.exec_stmt(stmt, &scope)?;
        }
        Ok(scope)
    }

    /// Hoist declarations and run top-level statements, returning the global
    /// scope. Used by the window runner (window state lives at top level).
    pub fn prepare(&mut self, program: &Program) -> Result<Env, Diagnostic> {
        self.hoist(&program.statements)?;
        for stmt in &program.statements {
            if matches!(
                stmt,
                Stmt::Function(_) | Stmt::Class(_) | Stmt::Game(_) | Stmt::Window(_)
            ) {
                continue;
            }
            self.exec_stmt(stmt, &self.globals.clone())?;
        }
        Ok(self.globals.clone())
    }

    /// Evaluate a list of widgets into laid-out-ready UI nodes (labels, props
    /// and click handlers resolved against `scope`). Called once per frame.
    pub fn build_widgets(&mut self, widgets: &[Widget], scope: &Env) -> Result<Vec<UiNode>, Diagnostic> {
        let mut out = Vec::with_capacity(widgets.len());
        for w in widgets {
            out.push(self.build_widget(w, scope)?);
        }
        Ok(out)
    }

    fn build_widget(&mut self, w: &Widget, scope: &Env) -> Result<UiNode, Diagnostic> {
        let kind = match w.name.as_str() {
            "column" => UiKind::Column,
            "row" => UiKind::Row,
            "text" => UiKind::Text,
            "button" => UiKind::Button,
            "spacer" => UiKind::Spacer,
            other => {
                return Err(Diagnostic::new(w.span, format!("unknown widget `{}`", other))
                    .with_hint("widgets are column, row, text, button, spacer"));
            }
        };
        let mut node = UiNode::new(kind);
        if let Some(label) = &w.label {
            node.text = Some(self.eval(label, scope)?.display());
        }
        for (name, expr) in &w.props {
            let v = self.eval(expr, scope)?;
            self.apply_widget_prop(&mut node, name, v, expr.span())?;
        }
        for child in &w.children {
            let c = self.build_widget(child, scope)?;
            node.children.push(c);
        }
        Ok(node)
    }

    fn apply_widget_prop(&self, node: &mut UiNode, name: &str, v: Value, span: Span) -> Result<(), Diagnostic> {
        match name {
            "padding" => node.props.padding = self.as_number(&v, span)? as f32,
            "spacing" => node.props.spacing = self.as_number(&v, span)? as f32,
            "size" => node.props.font_size = self.as_number(&v, span)? as i32,
            "width" => node.props.width = Some(self.as_number(&v, span)? as f32),
            "height" => node.props.height = Some(self.as_number(&v, span)? as f32),
            "color" => node.props.color = Some(self.as_color(&v, span)?),
            "bg" | "background" => node.props.bg = Some(self.as_color(&v, span)?),
            "sprite" => {
                let id = self.as_number(&v, span)?;
                if id < 0.0 || id.fract() != 0.0 {
                    return Err(Diagnostic::new(span, "sprite needs a sprite from load_sprite(...)"));
                }
                node.sprite = Some(id as usize);
            }
            "font" => {
                let id = self.as_number(&v, span)?;
                if id < 0.0 || id.fract() != 0.0 {
                    return Err(Diagnostic::new(span, "font needs a font from load_font(...)"));
                }
                node.font = Some(id as usize);
            }
            "align" => {
                let s = self.as_text(&v, span)?;
                node.props.align = match s.as_str() {
                    "center" | "middle" => Align::Center,
                    "left" | "start" | "top" => Align::Start,
                    "right" | "end" | "bottom" => Align::End,
                    other => {
                        return Err(Diagnostic::new(span, format!("unknown align `{}`", other))
                            .with_hint("align is center, left/start, or right/end"));
                    }
                };
            }
            "on_click" => {
                if !is_callable(&v) {
                    return Err(Diagnostic::new(span, "on_click needs a function")
                        .with_hint("pass a function by name, e.g. on_click: handle_click"));
                }
                node.callback = Some(v);
            }
            other => {
                return Err(Diagnostic::new(span, format!("unknown property `{}`", other)));
            }
        }
        Ok(())
    }

    /// Evaluate an expression in an existing scope (used for window props).
    pub fn eval_in(&mut self, expr: &Expr, env: &Env) -> Result<Value, Diagnostic> {
        self.eval(expr, env)
    }

    /// Interpret a value as a color (for window chrome props).
    pub fn value_as_color(&self, v: &Value, span: Span) -> Result<Color, Diagnostic> {
        self.as_color(v, span)
    }

    /// Call a UI callback (button click) from the window runner.
    pub fn call_callback(&mut self, callback: &Value) -> Result<(), Diagnostic> {
        self.call_value(callback.clone(), Vec::new(), Span::new(0, 0))?;
        Ok(())
    }

    /// Run one lifecycle hook (start/update/draw), binding its parameters.
    pub fn run_hook(&mut self, scope: &Env, hook: &Hook, args: Vec<Value>) -> Result<(), Diagnostic> {
        let child = Scope::new_child(scope);
        for (p, v) in hook.params.iter().zip(args) {
            env_declare(&child, p, v);
        }
        self.exec_block(&hook.body, &child)?;
        Ok(())
    }

    /// Run a whole program: hoist declarations, execute top-level statements,
    /// then call `main` if one is defined.
    pub fn run(&mut self, program: &Program) -> Result<(), Diagnostic> {
        self.hoist(&program.statements)?;

        for stmt in &program.statements {
            // Declarations were already hoisted; skip re-processing them so
            // top-level ordering of real statements is preserved.
            if matches!(stmt, Stmt::Function(_) | Stmt::Class(_)) {
                continue;
            }
            match self.exec_stmt(stmt, &self.globals.clone())? {
                Flow::Normal => {}
                Flow::Return(_) => break,
                Flow::Break | Flow::Continue => {
                    return Err(Diagnostic::new(
                        stmt_span(stmt),
                        "`break`/`continue` can only be used inside a loop",
                    ));
                }
            }
        }

        if let Some(Value::Function(f)) = env_get(&self.globals, "main") {
            self.call_function(&f, Vec::new(), None, Span::new(0, 0))?;
        }

        // Console programs that scheduled timers keep running in real time until
        // the timers are done. (`every` timers run forever — Ctrl+C to stop.)
        while self.has_timers() {
            std::thread::sleep(std::time::Duration::from_millis(16));
            self.tick_timers(0.016)?;
        }
        Ok(())
    }

    /// Register every top-level function and class so they can be referenced
    /// before their textual position (forward references).
    fn hoist(&mut self, statements: &[Stmt]) -> Result<(), Diagnostic> {
        // Classes first, so methods can construct any class.
        for stmt in statements {
            if let Stmt::Class(decl) = stmt {
                let def = self.build_class_def(decl);
                self.classes.insert(decl.name.clone(), def);
            }
        }
        for stmt in statements {
            if let Stmt::Function(decl) = stmt {
                let func = Rc::new(FunctionObj {
                    decl: Rc::new(decl.clone()),
                    closure: self.globals.clone(),
                });
                env_declare(&self.globals, &decl.name, Value::Function(func));
            }
        }
        Ok(())
    }

    fn build_class_def(&self, decl: &ClassDecl) -> Rc<ClassDef> {
        let mut methods = HashMap::new();
        for m in &decl.methods {
            let func = Rc::new(FunctionObj {
                decl: Rc::new(m.clone()),
                closure: self.globals.clone(),
            });
            methods.insert(m.name.clone(), func);
        }
        Rc::new(ClassDef { name: decl.name.clone(), fields: decl.fields.clone(), methods })
    }

    // ---- statements ------------------------------------------------------

    fn exec_block(&mut self, stmts: &[Stmt], env: &Env) -> ExecResult {
        for stmt in stmts {
            match self.exec_stmt(stmt, env)? {
                Flow::Normal => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Normal)
    }

    fn exec_stmt(&mut self, stmt: &Stmt, env: &Env) -> ExecResult {
        match stmt {
            Stmt::Assign { target, value, .. } => {
                let v = self.eval(value, env)?;
                self.assign(target, v, env)?;
                Ok(Flow::Normal)
            }
            Stmt::Function(decl) => {
                let func = Rc::new(FunctionObj {
                    decl: Rc::new(decl.clone()),
                    closure: env.clone(),
                });
                env_declare(env, &decl.name, Value::Function(func));
                Ok(Flow::Normal)
            }
            Stmt::Class(decl) => {
                let def = self.build_class_def(decl);
                self.classes.insert(decl.name.clone(), def);
                Ok(Flow::Normal)
            }
            Stmt::If { branches, else_body, .. } => {
                for (cond, body) in branches {
                    if self.eval_bool(cond, env)? {
                        return self.exec_block(body, env);
                    }
                }
                if let Some(body) = else_body {
                    return self.exec_block(body, env);
                }
                Ok(Flow::Normal)
            }
            Stmt::While { cond, body, .. } => {
                while self.eval_bool(cond, env)? {
                    match self.exec_block(body, env)? {
                        Flow::Break => break,
                        Flow::Continue | Flow::Normal => {}
                        ret @ Flow::Return(_) => return Ok(ret),
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::ForEvery { var, iterable, body, span } => {
                let seq = self.eval(iterable, env)?;
                let items = self.iterate(seq, *span)?;
                for item in items {
                    env_declare(env, var, item);
                    match self.exec_block(body, env)? {
                        Flow::Break => break,
                        Flow::Continue | Flow::Normal => {}
                        ret @ Flow::Return(_) => return Ok(ret),
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::Repeat { count, body, span } => {
                let n = self.eval_number(count, env)?;
                if n < 0.0 {
                    return Err(Diagnostic::new(*span, "`repeat` count can't be negative"));
                }
                let times = n.floor() as i64;
                for _ in 0..times {
                    match self.exec_block(body, env)? {
                        Flow::Break => break,
                        Flow::Continue | Flow::Normal => {}
                        ret @ Flow::Return(_) => return Ok(ret),
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::Loop { body, .. } => {
                loop {
                    match self.exec_block(body, env)? {
                        Flow::Break => break,
                        Flow::Continue | Flow::Normal => {}
                        ret @ Flow::Return(_) => return Ok(ret),
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::Return { value, .. } => {
                let v = match value {
                    Some(e) => self.eval(e, env)?,
                    None => Value::Nothing,
                };
                Ok(Flow::Return(v))
            }
            Stmt::Break(_) => Ok(Flow::Break),
            Stmt::Continue(_) => Ok(Flow::Continue),
            Stmt::Game(_) | Stmt::Window(_) => Ok(Flow::Normal), // handled by the runner, not here
            Stmt::Expr(e) => {
                self.eval(e, env)?;
                Ok(Flow::Normal)
            }
        }
    }

    /// Assign a value to an lvalue: a variable, a class field, or an element.
    fn assign(&mut self, target: &Expr, value: Value, env: &Env) -> Result<(), Diagnostic> {
        match target {
            Expr::Ident(name, _) => {
                env_set(env, name, value);
                Ok(())
            }
            Expr::SelfRef(span) => {
                Err(Diagnostic::new(*span, "can't assign to `self`"))
            }
            Expr::Field { object, name, span } => {
                let obj = self.eval(object, env)?;
                match obj {
                    Value::Class(inst) => {
                        let mut inst = inst.borrow_mut();
                        if !inst.def.fields.iter().any(|f| &f.name == name) {
                            return Err(Diagnostic::new(
                                *span,
                                format!("`{}` has no field `{}`", inst.def.name, name),
                            ));
                        }
                        inst.fields.insert(name.clone(), value);
                        Ok(())
                    }
                    other => Err(Diagnostic::new(
                        *span,
                        format!("can't set field `{}` on a {}", name, other.type_name()),
                    )),
                }
            }
            Expr::Index { object, index, span } => {
                let obj = self.eval(object, env)?;
                let idx = self.eval(index, env)?;
                match obj {
                    Value::List(list) => {
                        let i = self.as_index(&idx, *span)?;
                        let mut list = list.borrow_mut();
                        if i >= list.len() {
                            return Err(Diagnostic::new(
                                *span,
                                format!("index {} is out of range (list has {} items)", i, list.len()),
                            ));
                        }
                        list[i] = value;
                        Ok(())
                    }
                    Value::Dictionary(map) => {
                        let key = self.as_map_key(&idx, *span)?;
                        map.borrow_mut().set(key, value);
                        Ok(())
                    }
                    other => Err(Diagnostic::new(
                        *span,
                        format!("can't index into a {}", other.type_name()),
                    )),
                }
            }
            other => Err(Diagnostic::new(
                other.span(),
                "the left side of `=` must be a variable, field, or element",
            )),
        }
    }

    // ---- expressions -----------------------------------------------------

    fn eval(&mut self, expr: &Expr, env: &Env) -> EvalResult {
        match expr {
            Expr::Number(n, _) => Ok(Value::Number(*n)),
            Expr::Bool(b, _) => Ok(Value::Bool(*b)),
            Expr::Nothing(_) => Ok(Value::Nothing),
            Expr::Text(chunks, _) => {
                let mut out = String::new();
                for chunk in chunks {
                    match chunk {
                        StrChunk::Lit(s) => out.push_str(s),
                        StrChunk::Expr(e) => out.push_str(&self.eval(e, env)?.display()),
                    }
                }
                Ok(Value::text(out))
            }
            Expr::SelfRef(span) => env_get(env, "self").ok_or_else(|| {
                Diagnostic::new(*span, "`self` is only available inside a method")
            }),
            Expr::Ident(name, span) => self.lookup(name, env, *span),
            Expr::Unary { op, expr, span } => {
                let v = self.eval(expr, env)?;
                match op {
                    UnaryOp::Neg => match v {
                        Value::Number(n) => Ok(Value::Number(-n)),
                        other => Err(Diagnostic::new(
                            *span,
                            format!("can't negate a {}", other.type_name()),
                        )),
                    },
                    UnaryOp::Not => match v {
                        Value::Bool(b) => Ok(Value::Bool(!b)),
                        other => Err(Diagnostic::new(
                            *span,
                            format!("`not` needs a true/false value, got a {}", other.type_name()),
                        )),
                    },
                }
            }
            Expr::Binary { op, left, right, span } => self.eval_binary(*op, left, right, env, *span),
            Expr::IsNothing { expr, negated, .. } => {
                let v = self.eval(expr, env)?;
                let is_nothing = matches!(v, Value::Nothing);
                Ok(Value::Bool(is_nothing != *negated))
            }
            Expr::ListLit { items, .. } => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.eval(item, env)?);
                }
                Ok(Value::List(Rc::new(std::cell::RefCell::new(values))))
            }
            Expr::DictionaryLit { entries, span } => {
                let mut map = PtMap::new();
                for (k, v) in entries {
                    let key_v = self.eval(k, env)?;
                    let key = self.as_map_key(&key_v, *span)?;
                    let val = self.eval(v, env)?;
                    map.set(key, val);
                }
                Ok(Value::Dictionary(Rc::new(std::cell::RefCell::new(map))))
            }
            Expr::ClassLit { name, fields, span } => self.eval_class_literal(name, fields, env, *span),
            Expr::Field { object, name, span } => {
                let obj = self.eval(object, env)?;
                self.eval_field(obj, name, *span)
            }
            Expr::Index { object, index, span } => {
                let obj = self.eval(object, env)?;
                let idx = self.eval(index, env)?;
                self.eval_index(obj, idx, *span)
            }
            Expr::Call { callee, args, span } => self.eval_call(callee, args, env, *span),
            Expr::Wait { span, .. } => Err(Diagnostic::new(
                *span,
                "`wait`/`start` aren't part of PlainText",
            )
            .with_hint("for timed actions use after(seconds, action) or every(seconds, action)")),
        }
    }

    fn lookup(&self, name: &str, env: &Env, span: Span) -> EvalResult {
        if let Some(v) = env_get(env, name) {
            return Ok(v);
        }
        if let Some(b) = Builtin::from_name(name) {
            return Ok(Value::Builtin(b));
        }
        Err(Diagnostic::new(span, format!("unknown name `{}`", name))
            .with_hint("check the spelling, or make sure it's defined before this point"))
    }

    fn eval_binary(&mut self, op: BinaryOp, left: &Expr, right: &Expr, env: &Env, span: Span) -> EvalResult {
        // Logical operators short-circuit and need booleans.
        match op {
            BinaryOp::And => {
                let l = self.eval_bool(left, env)?;
                if !l {
                    return Ok(Value::Bool(false));
                }
                return Ok(Value::Bool(self.eval_bool(right, env)?));
            }
            BinaryOp::Or => {
                let l = self.eval_bool(left, env)?;
                if l {
                    return Ok(Value::Bool(true));
                }
                return Ok(Value::Bool(self.eval_bool(right, env)?));
            }
            _ => {}
        }

        let l = self.eval(left, env)?;
        let r = self.eval(right, env)?;

        match op {
            BinaryOp::Eq => Ok(Value::Bool(values_equal(&l, &r))),
            BinaryOp::NotEq => Ok(Value::Bool(!values_equal(&l, &r))),
            BinaryOp::Add => match (&l, &r) {
                (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a + b)),
                (Value::Text(a), Value::Text(b)) => Ok(Value::text(format!("{}{}", a, b))),
                _ => Err(self.binop_err("add", &l, &r, span)),
            },
            BinaryOp::Sub => self.arith(&l, &r, span, "subtract", |a, b| a - b),
            BinaryOp::Mul => self.arith(&l, &r, span, "multiply", |a, b| a * b),
            BinaryOp::Div => {
                if let (Value::Number(_), Value::Number(b)) = (&l, &r) {
                    if *b == 0.0 {
                        return Err(Diagnostic::new(span, "can't divide by zero"));
                    }
                }
                self.arith(&l, &r, span, "divide", |a, b| a / b)
            }
            BinaryOp::Mod => {
                if let (Value::Number(_), Value::Number(b)) = (&l, &r) {
                    if *b == 0.0 {
                        return Err(Diagnostic::new(span, "can't take remainder with zero"));
                    }
                }
                self.arith(&l, &r, span, "take the remainder of", |a, b| a % b)
            }
            BinaryOp::Lt => self.compare(&l, &r, span, |o| o.is_lt()),
            BinaryOp::LtEq => self.compare(&l, &r, span, |o| o.is_le()),
            BinaryOp::Gt => self.compare(&l, &r, span, |o| o.is_gt()),
            BinaryOp::GtEq => self.compare(&l, &r, span, |o| o.is_ge()),
            BinaryOp::And | BinaryOp::Or => unreachable!(),
        }
    }

    fn arith(
        &self,
        l: &Value,
        r: &Value,
        span: Span,
        verb: &str,
        f: impl Fn(f64, f64) -> f64,
    ) -> EvalResult {
        match (l, r) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(f(*a, *b))),
            _ => Err(self.binop_err(verb, l, r, span)),
        }
    }

    fn compare(
        &self,
        l: &Value,
        r: &Value,
        span: Span,
        f: impl Fn(std::cmp::Ordering) -> bool,
    ) -> EvalResult {
        use std::cmp::Ordering;
        let ord = match (l, r) {
            (Value::Number(a), Value::Number(b)) => {
                a.partial_cmp(b).unwrap_or(Ordering::Equal)
            }
            (Value::Text(a), Value::Text(b)) => a.cmp(b),
            _ => {
                return Err(Diagnostic::new(
                    span,
                    format!(
                        "can't compare a {} with a {}",
                        l.type_name(),
                        r.type_name()
                    ),
                )
                .with_hint("comparisons like `<` work on two numbers or two texts"));
            }
        };
        Ok(Value::Bool(f(ord)))
    }

    fn binop_err(&self, verb: &str, l: &Value, r: &Value, span: Span) -> Diagnostic {
        let d = Diagnostic::new(
            span,
            format!("can't {} a {} and a {}", verb, l.type_name(), r.type_name()),
        );
        if verb == "add" && matches!((l, r), (Value::Number(_), Value::Text(_)) | (Value::Text(_), Value::Number(_)))
        {
            d.with_hint("to join text and a number, use interpolation like \"{x}\" or to_text(x)")
        } else {
            d
        }
    }

    fn eval_class_literal(
        &mut self,
        name: &str,
        fields: &[(String, Expr)],
        env: &Env,
        span: Span,
    ) -> EvalResult {
        let def = self.classes.get(name).cloned().ok_or_else(|| {
            Diagnostic::new(span, format!("unknown class `{}`", name))
                .with_hint("declare it with `class {} {{ ... }}`".replace("{}", name).as_str())
        })?;

        let mut field_values: HashMap<String, Value> = HashMap::new();
        for (fname, fexpr) in fields {
            if !def.fields.iter().any(|f| &f.name == fname) {
                return Err(Diagnostic::new(
                    span,
                    format!("`{}` has no field `{}`", name, fname),
                ));
            }
            field_values.insert(fname.clone(), self.eval(fexpr, env)?);
        }

        // Fill in defaults / check required fields.
        for field in &def.fields {
            if field_values.contains_key(&field.name) {
                continue;
            }
            if let Some(default) = &field.default {
                let v = self.eval(default, env)?;
                field_values.insert(field.name.clone(), v);
            } else if is_optional_ann(field.ty.as_ref()) {
                // An optional field (`x: Text?`) left out defaults to nothing.
                field_values.insert(field.name.clone(), Value::Nothing);
            } else {
                return Err(Diagnostic::new(
                    span,
                    format!("`{}` is missing a value for field `{}`", name, field.name),
                ));
            }
        }

        Ok(Value::Class(Rc::new(std::cell::RefCell::new(ClassInstance {
            def,
            fields: field_values,
        }))))
    }

    /// Read `object.name`: a class field, a bound method, or nothing valid.
    fn eval_field(&mut self, obj: Value, name: &str, span: Span) -> EvalResult {
        match &obj {
            Value::Class(inst) => {
                let inst_ref = inst.borrow();
                if let Some(v) = inst_ref.fields.get(name) {
                    return Ok(v.clone());
                }
                if let Some(func) = inst_ref.def.methods.get(name).cloned() {
                    return Ok(Value::BoundMethod {
                        receiver: Box::new(obj.clone()),
                        func,
                    });
                }
                Err(Diagnostic::new(
                    span,
                    format!("`{}` has no field or method `{}`", inst_ref.def.name, name),
                ))
            }
            _ => Err(Diagnostic::new(
                span,
                format!("a {} has no field `{}`", obj.type_name(), name),
            )
            .with_hint("methods like `.length()` must be called; field access works on classes")),
        }
    }

    fn eval_index(&mut self, obj: Value, idx: Value, span: Span) -> EvalResult {
        match &obj {
            Value::List(list) => {
                let i = self.as_index(&idx, span)?;
                let list = list.borrow();
                list.get(i).cloned().ok_or_else(|| {
                    Diagnostic::new(
                        span,
                        format!("index {} is out of range (list has {} items)", i, list.len()),
                    )
                })
            }
            Value::Text(s) => {
                let i = self.as_index(&idx, span)?;
                s.chars().nth(i).map(|c| Value::text(c.to_string())).ok_or_else(|| {
                    Diagnostic::new(span, format!("index {} is out of range for this text", i))
                })
            }
            Value::Dictionary(map) => {
                let key = self.as_map_key(&idx, span)?;
                map.borrow().get(&key).ok_or_else(|| {
                    Diagnostic::new(span, format!("dictionary has no key {}", idx.display()))
                        .with_hint("use `.has(key)` to check first, or `.get(key)` for nothing-on-miss")
                })
            }
            other => Err(Diagnostic::new(
                span,
                format!("can't index into a {}", other.type_name()),
            )),
        }
    }

    // ---- calls -----------------------------------------------------------

    fn eval_call(&mut self, callee: &Expr, args: &[Expr], env: &Env, span: Span) -> EvalResult {
        // Method call: evaluate receiver, then dispatch by name so builtin
        // methods on lists/text/maps work without a stored function value.
        if let Expr::Field { object, name, span: fspan } = callee {
            let receiver = self.eval(object, env)?;
            let argv = self.eval_args(args, env)?;
            return self.call_method(receiver, name, argv, *fspan);
        }

        let callee_v = self.eval(callee, env)?;
        let argv = self.eval_args(args, env)?;
        self.call_value(callee_v, argv, span)
    }

    fn eval_args(&mut self, args: &[Expr], env: &Env) -> Result<Vec<Value>, Diagnostic> {
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            out.push(self.eval(a, env)?);
        }
        Ok(out)
    }

    fn call_value(&mut self, callee: Value, args: Vec<Value>, span: Span) -> EvalResult {
        match callee {
            Value::Function(f) => self.call_function(&f, args, None, span),
            Value::BoundMethod { receiver, func } => {
                self.call_function(&func, args, Some(*receiver), span)
            }
            Value::Builtin(b) => self.call_builtin(b, args, span),
            other => Err(Diagnostic::new(
                span,
                format!("a {} isn't something you can call", other.type_name()),
            )),
        }
    }

    fn call_function(
        &mut self,
        func: &Rc<FunctionObj>,
        args: Vec<Value>,
        self_val: Option<Value>,
        span: Span,
    ) -> EvalResult {
        let decl = &func.decl;
        let scope = Scope::new_child(&func.closure);
        if let Some(sv) = self_val {
            env_declare(&scope, "self", sv);
        }

        let required = decl.params.iter().filter(|p| p.default.is_none()).count();
        if args.len() > decl.params.len() || args.len() < required {
            return Err(Diagnostic::new(
                span,
                format!(
                    "`{}` takes {} argument(s), but got {}",
                    decl.name,
                    decl.params.len(),
                    args.len()
                ),
            ));
        }

        let mut args = args.into_iter();
        for param in &decl.params {
            let value = match args.next() {
                Some(v) => v,
                None => {
                    // Must have a default (checked by arity above).
                    let default = param.default.as_ref().unwrap();
                    self.eval(default, &scope)?
                }
            };
            env_declare(&scope, &param.name, value);
        }

        match self.exec_block(&decl.body, &scope)? {
            Flow::Return(v) => Ok(v),
            Flow::Normal => Ok(Value::Nothing),
            Flow::Break | Flow::Continue => Err(Diagnostic::new(
                decl.span,
                "`break`/`continue` can only be used inside a loop",
            )),
        }
    }

    fn call_method(&mut self, receiver: Value, name: &str, args: Vec<Value>, span: Span) -> EvalResult {
        match &receiver {
            Value::Class(inst) => {
                let method = inst.borrow().def.methods.get(name).cloned();
                if let Some(func) = method {
                    return self.call_function(&func, args, Some(receiver.clone()), span);
                }
                // A field holding a function value can also be called.
                let field = inst.borrow().fields.get(name).cloned();
                if let Some(f) = field {
                    return self.call_value(f, args, span);
                }
                Err(Diagnostic::new(
                    span,
                    format!("`{}` has no method `{}`", inst.borrow().def.name, name),
                ))
            }
            Value::List(list) => self.list_method(list, name, args, span),
            Value::Text(s) => self.text_method(s, name, args, span),
            Value::Dictionary(map) => self.map_method(map, name, args, span),
            other => Err(Diagnostic::new(
                span,
                format!("a {} has no method `{}`", other.type_name(), name),
            )),
        }
    }

    fn list_method(
        &mut self,
        list: &Rc<std::cell::RefCell<Vec<Value>>>,
        name: &str,
        args: Vec<Value>,
        span: Span,
    ) -> EvalResult {
        match name {
            "length" => {
                self.expect_arity(name, &args, 0, span)?;
                Ok(Value::Number(list.borrow().len() as f64))
            }
            "is_empty" => {
                self.expect_arity(name, &args, 0, span)?;
                Ok(Value::Bool(list.borrow().is_empty()))
            }
            "append" | "add" => {
                self.expect_arity(name, &args, 1, span)?;
                list.borrow_mut().push(args.into_iter().next().unwrap());
                Ok(Value::Nothing)
            }
            "pop" => {
                self.expect_arity(name, &args, 0, span)?;
                list.borrow_mut().pop().ok_or_else(|| {
                    Diagnostic::new(span, "can't pop from an empty list")
                })
            }
            "get" => {
                self.expect_arity(name, &args, 1, span)?;
                let i = self.as_index(&args[0], span)?;
                list.borrow().get(i).cloned().ok_or_else(|| {
                    Diagnostic::new(span, format!("index {} is out of range", i))
                })
            }
            "contains" => {
                self.expect_arity(name, &args, 1, span)?;
                let found = list.borrow().iter().any(|v| values_equal(v, &args[0]));
                Ok(Value::Bool(found))
            }
            "first" | "last" => {
                self.expect_arity(name, &args, 0, span)?;
                let list = list.borrow();
                let item = if name == "first" { list.first() } else { list.last() };
                item.cloned().ok_or_else(|| {
                    Diagnostic::new(span, format!("can't take the {} item of an empty list", name))
                })
            }
            "index_of" => {
                self.expect_arity(name, &args, 1, span)?;
                let pos = list.borrow().iter().position(|v| values_equal(v, &args[0]));
                Ok(Value::Number(pos.map(|i| i as f64).unwrap_or(-1.0)))
            }
            "remove_at" => {
                self.expect_arity(name, &args, 1, span)?;
                let i = self.as_index(&args[0], span)?;
                let mut list = list.borrow_mut();
                if i >= list.len() {
                    return Err(Diagnostic::new(span, format!("index {} is out of range", i)));
                }
                Ok(list.remove(i))
            }
            "reversed" => {
                self.expect_arity(name, &args, 0, span)?;
                let mut items = list.borrow().clone();
                items.reverse();
                Ok(Value::List(Rc::new(std::cell::RefCell::new(items))))
            }
            "join" => {
                self.expect_arity(name, &args, 1, span)?;
                let sep = self.as_text(&args[0], span)?;
                let parts: Vec<String> = list.borrow().iter().map(|v| v.display()).collect();
                Ok(Value::text(parts.join(&sep)))
            }
            _ => Err(Diagnostic::new(span, format!("a list has no method `{}`", name)).with_hint(
                "lists have length, is_empty, append, pop, get, contains, first, last, index_of, remove_at, reversed, join",
            )),
        }
    }

    fn text_method(&mut self, s: &Rc<String>, name: &str, args: Vec<Value>, span: Span) -> EvalResult {
        match name {
            "length" => {
                self.expect_arity(name, &args, 0, span)?;
                Ok(Value::Number(s.chars().count() as f64))
            }
            "upper" => {
                self.expect_arity(name, &args, 0, span)?;
                Ok(Value::text(s.to_uppercase()))
            }
            "lower" => {
                self.expect_arity(name, &args, 0, span)?;
                Ok(Value::text(s.to_lowercase()))
            }
            "contains" => {
                self.expect_arity(name, &args, 1, span)?;
                let needle = self.as_text(&args[0], span)?;
                Ok(Value::Bool(s.contains(&needle)))
            }
            "trim" => {
                self.expect_arity(name, &args, 0, span)?;
                Ok(Value::text(s.trim().to_string()))
            }
            "starts_with" => {
                self.expect_arity(name, &args, 1, span)?;
                let p = self.as_text(&args[0], span)?;
                Ok(Value::Bool(s.starts_with(&p)))
            }
            "ends_with" => {
                self.expect_arity(name, &args, 1, span)?;
                let p = self.as_text(&args[0], span)?;
                Ok(Value::Bool(s.ends_with(&p)))
            }
            "replace" => {
                self.expect_arity(name, &args, 2, span)?;
                let from = self.as_text(&args[0], span)?;
                let to = self.as_text(&args[1], span)?;
                Ok(Value::text(s.replace(&from, &to)))
            }
            "repeat" => {
                self.expect_arity(name, &args, 1, span)?;
                let n = self.as_index(&args[0], span)?;
                Ok(Value::text(s.repeat(n)))
            }
            "split" => {
                self.expect_arity(name, &args, 1, span)?;
                let sep = self.as_text(&args[0], span)?;
                let parts: Vec<Value> = if sep.is_empty() {
                    s.chars().map(|c| Value::text(c.to_string())).collect()
                } else {
                    s.split(&sep).map(Value::text).collect()
                };
                Ok(Value::List(Rc::new(std::cell::RefCell::new(parts))))
            }
            "substring" => {
                self.expect_arity(name, &args, 2, span)?;
                let start = self.as_index(&args[0], span)?;
                let end = self.as_index(&args[1], span)?;
                let chars: Vec<char> = s.chars().collect();
                if start > end || end > chars.len() {
                    return Err(Diagnostic::new(
                        span,
                        format!("substring range {}..{} is out of range (text has {} characters)", start, end, chars.len()),
                    ));
                }
                Ok(Value::text(chars[start..end].iter().collect::<String>()))
            }
            _ => Err(Diagnostic::new(span, format!("text has no method `{}`", name)).with_hint(
                "text has length, upper, lower, contains, trim, starts_with, ends_with, replace, repeat, split, substring",
            )),
        }
    }

    fn map_method(&mut self, map: &Rc<std::cell::RefCell<PtMap>>, name: &str, args: Vec<Value>, span: Span) -> EvalResult {
        match name {
            "length" => {
                self.expect_arity(name, &args, 0, span)?;
                Ok(Value::Number(map.borrow().entries.len() as f64))
            }
            "has" => {
                self.expect_arity(name, &args, 1, span)?;
                let key = self.as_map_key(&args[0], span)?;
                Ok(Value::Bool(map.borrow().has(&key)))
            }
            "get" => {
                self.expect_arity(name, &args, 1, span)?;
                let key = self.as_map_key(&args[0], span)?;
                Ok(map.borrow().get(&key).unwrap_or(Value::Nothing))
            }
            "keys" => {
                self.expect_arity(name, &args, 0, span)?;
                let keys: Vec<Value> = map.borrow().entries.iter().map(|(k, _)| k.to_value()).collect();
                Ok(Value::List(Rc::new(std::cell::RefCell::new(keys))))
            }
            "values" => {
                self.expect_arity(name, &args, 0, span)?;
                let vals: Vec<Value> = map.borrow().entries.iter().map(|(_, v)| v.clone()).collect();
                Ok(Value::List(Rc::new(std::cell::RefCell::new(vals))))
            }
            "is_empty" => {
                self.expect_arity(name, &args, 0, span)?;
                Ok(Value::Bool(map.borrow().entries.is_empty()))
            }
            "remove" => {
                self.expect_arity(name, &args, 1, span)?;
                let key = self.as_map_key(&args[0], span)?;
                Ok(map.borrow_mut().remove(&key).unwrap_or(Value::Nothing))
            }
            _ => Err(Diagnostic::new(span, format!("a dictionary has no method `{}`", name))
                .with_hint("maps have length, has, get, keys, values, is_empty, remove")),
        }
    }

    fn call_builtin(&mut self, b: Builtin, args: Vec<Value>, span: Span) -> EvalResult {
        match b {
            Builtin::Print => {
                let line: Vec<String> = args.iter().map(|v| v.display()).collect();
                println!("{}", line.join(" "));
                // Flush so output appears immediately, even when stdout is a
                // pipe/file (important for timers and long-running programs).
                use std::io::Write;
                let _ = std::io::stdout().flush();
                Ok(Value::Nothing)
            }
            Builtin::ToText => {
                self.expect_arity("to_text", &args, 1, span)?;
                Ok(Value::text(args[0].display()))
            }
            Builtin::ToNumber => {
                self.expect_arity("to_number", &args, 1, span)?;
                match &args[0] {
                    Value::Number(n) => Ok(Value::Number(*n)),
                    Value::Text(s) => s.trim().parse::<f64>().map(Value::Number).map_err(|_| {
                        Diagnostic::new(span, format!("can't read \"{}\" as a number", s))
                    }),
                    other => Err(Diagnostic::new(
                        span,
                        format!("can't turn a {} into a number", other.type_name()),
                    )),
                }
            }
            Builtin::Length => {
                self.expect_arity("length", &args, 1, span)?;
                match &args[0] {
                    Value::List(l) => Ok(Value::Number(l.borrow().len() as f64)),
                    Value::Text(s) => Ok(Value::Number(s.chars().count() as f64)),
                    Value::Dictionary(m) => Ok(Value::Number(m.borrow().entries.len() as f64)),
                    other => Err(Diagnostic::new(
                        span,
                        format!("length() needs a list, text, or dictionary, got a {}", other.type_name()),
                    )),
                }
            }
            Builtin::Min | Builtin::Greatest => {
                if args.len() < 2 {
                    return Err(Diagnostic::new(span, format!("{}() needs at least 2 numbers", b.name())));
                }
                let mut acc = self.as_number(&args[0], span)?;
                for a in &args[1..] {
                    let n = self.as_number(a, span)?;
                    acc = if b == Builtin::Min { acc.min(n) } else { acc.max(n) };
                }
                Ok(Value::Number(acc))
            }
            Builtin::Abs | Builtin::Sqrt | Builtin::Floor | Builtin::Ceil | Builtin::Round => {
                self.expect_arity(b.name(), &args, 1, span)?;
                let n = self.as_number(&args[0], span)?;
                let r = match b {
                    Builtin::Abs => n.abs(),
                    Builtin::Sqrt => {
                        if n < 0.0 {
                            return Err(Diagnostic::new(span, "can't take the square root of a negative number"));
                        }
                        n.sqrt()
                    }
                    Builtin::Floor => n.floor(),
                    Builtin::Ceil => n.ceil(),
                    Builtin::Round => n.round(),
                    _ => unreachable!(),
                };
                Ok(Value::Number(r))
            }
            Builtin::RandomBetween => {
                self.expect_arity("random_between", &args, 2, span)?;
                let lo = self.as_number(&args[0], span)?;
                let hi = self.as_number(&args[1], span)?;
                Ok(Value::Number(lo + self.next_rand() * (hi - lo)))
            }
            Builtin::Pow => {
                self.expect_arity("pow", &args, 2, span)?;
                let base = self.as_number(&args[0], span)?;
                let exp = self.as_number(&args[1], span)?;
                Ok(Value::Number(base.powf(exp)))
            }
            Builtin::Clamp => {
                self.expect_arity("clamp", &args, 3, span)?;
                let x = self.as_number(&args[0], span)?;
                let lo = self.as_number(&args[1], span)?;
                let hi = self.as_number(&args[2], span)?;
                Ok(Value::Number(x.max(lo).min(hi)))
            }
            Builtin::Sin | Builtin::Cos | Builtin::Tan => {
                self.expect_arity(b.name(), &args, 1, span)?;
                let n = self.as_number(&args[0], span)?;
                Ok(Value::Number(match b {
                    Builtin::Sin => n.sin(),
                    Builtin::Cos => n.cos(),
                    Builtin::Tan => n.tan(),
                    _ => unreachable!(),
                }))
            }
            Builtin::ReadFile => {
                self.expect_arity("read_file", &args, 1, span)?;
                let path = self.as_text(&args[0], span)?;
                std::fs::read_to_string(&path).map(Value::text).map_err(|e| {
                    Diagnostic::new(span, format!("couldn't read file \"{}\": {}", path, e))
                })
            }
            Builtin::WriteFile => {
                self.expect_arity("write_file", &args, 2, span)?;
                let path = self.as_text(&args[0], span)?;
                let contents = self.as_text(&args[1], span)?;
                std::fs::write(&path, contents).map(|_| Value::Nothing).map_err(|e| {
                    Diagnostic::new(span, format!("couldn't write file \"{}\": {}", path, e))
                })
            }
            Builtin::AppendFile => {
                self.expect_arity("append_file", &args, 2, span)?;
                let path = self.as_text(&args[0], span)?;
                let contents = self.as_text(&args[1], span)?;
                use std::io::Write;
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .and_then(|mut f| f.write_all(contents.as_bytes()))
                    .map(|_| Value::Nothing)
                    .map_err(|e| {
                        Diagnostic::new(span, format!("couldn't append to file \"{}\": {}", path, e))
                    })
            }
            Builtin::FileExists => {
                self.expect_arity("file_exists", &args, 1, span)?;
                let path = self.as_text(&args[0], span)?;
                Ok(Value::Bool(std::path::Path::new(&path).exists()))
            }
            Builtin::Now => {
                self.expect_arity("now", &args, 0, span)?;
                let secs = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0);
                Ok(Value::Number(secs))
            }
            Builtin::Clock => {
                self.expect_arity("clock", &args, 0, span)?;
                // Seconds since the interpreter started — handy for timing.
                let secs = self.start.elapsed().as_secs_f64();
                Ok(Value::Number(secs))
            }
            Builtin::Rgb => {
                self.expect_arity("rgb", &args, 3, span)?;
                let r = self.as_number(&args[0], span)?;
                let g = self.as_number(&args[1], span)?;
                let b = self.as_number(&args[2], span)?;
                Ok(Value::List(Rc::new(RefCell::new(vec![
                    Value::Number(r),
                    Value::Number(g),
                    Value::Number(b),
                ]))))
            }
            Builtin::Rgba => {
                self.expect_arity("rgba", &args, 4, span)?;
                let vals: Vec<Value> = args.iter().map(|a| self.as_number(a, span).map(Value::Number)).collect::<Result<_, _>>()?;
                Ok(Value::List(Rc::new(RefCell::new(vals))))
            }
            Builtin::ClearScreen => {
                self.expect_arity("clear_screen", &args, 1, span)?;
                let color = self.as_color(&args[0], span)?;
                self.push_draw(DrawCmd::Clear(color), span)
            }
            Builtin::DrawCircle => {
                self.expect_arity("draw_circle", &args, 4, span)?;
                let x = self.as_number(&args[0], span)? as f32;
                let y = self.as_number(&args[1], span)? as f32;
                let r = self.as_number(&args[2], span)? as f32;
                let color = self.as_color(&args[3], span)?;
                self.push_draw(DrawCmd::Circle { x, y, r, color }, span)
            }
            Builtin::DrawRectangle => {
                self.expect_arity("draw_rectangle", &args, 5, span)?;
                let x = self.as_number(&args[0], span)? as f32;
                let y = self.as_number(&args[1], span)? as f32;
                let w = self.as_number(&args[2], span)? as f32;
                let h = self.as_number(&args[3], span)? as f32;
                let color = self.as_color(&args[4], span)?;
                self.push_draw(DrawCmd::Rect { x, y, w, h, color }, span)
            }
            Builtin::DrawLine => {
                self.expect_arity("draw_line", &args, 5, span)?;
                let x1 = self.as_number(&args[0], span)? as f32;
                let y1 = self.as_number(&args[1], span)? as f32;
                let x2 = self.as_number(&args[2], span)? as f32;
                let y2 = self.as_number(&args[3], span)? as f32;
                let color = self.as_color(&args[4], span)?;
                self.push_draw(DrawCmd::Line { x1, y1, x2, y2, thick: 1.0, color }, span)
            }
            Builtin::DrawText => {
                self.expect_arity("draw_text", &args, 5, span)?;
                let text = args[0].display();
                let x = self.as_number(&args[1], span)? as f32;
                let y = self.as_number(&args[2], span)? as f32;
                let size = self.as_number(&args[3], span)? as i32;
                let color = self.as_color(&args[4], span)?;
                self.push_draw(DrawCmd::Text { text, x, y, size, color, font: None }, span)
            }
            Builtin::ScreenWidth | Builtin::ScreenHeight => {
                self.expect_arity(b.name(), &args, 0, span)?;
                let g = self.gfx_or_err(span)?;
                let g = g.borrow();
                let n = if b == Builtin::ScreenWidth { g.screen_w } else { g.screen_h };
                Ok(Value::Number(n as f64))
            }
            Builtin::KeyDown | Builtin::KeyPressed => {
                self.expect_arity(b.name(), &args, 1, span)?;
                let key = self.as_text(&args[0], span)?.to_lowercase();
                let g = self.gfx_or_err(span)?;
                let g = g.borrow();
                let set = if b == Builtin::KeyDown { &g.keys_down } else { &g.keys_pressed };
                Ok(Value::Bool(set.contains(&key)))
            }
            Builtin::MouseX | Builtin::MouseY => {
                self.expect_arity(b.name(), &args, 0, span)?;
                let g = self.gfx_or_err(span)?;
                let g = g.borrow();
                Ok(Value::Number(if b == Builtin::MouseX { g.mouse_x as f64 } else { g.mouse_y as f64 }))
            }
            Builtin::MouseDown | Builtin::MousePressed => {
                self.expect_arity(b.name(), &args, 0, span)?;
                let g = self.gfx_or_err(span)?;
                let g = g.borrow();
                Ok(Value::Bool(if b == Builtin::MouseDown { g.mouse_down } else { g.mouse_pressed }))
            }
            Builtin::LoadSprite => {
                self.expect_arity("load_sprite", &args, 1, span)?;
                let path = self.as_text(&args[0], span)?;
                if !std::path::Path::new(&path).exists() {
                    return Err(Diagnostic::new(span, format!("couldn't find sprite file \"{}\"", path)));
                }
                let g = self.gfx_or_err(span)?;
                let id = g.borrow_mut().queue_sprite(path);
                Ok(Value::Number(id as f64))
            }
            Builtin::DrawSprite => {
                self.expect_arity("draw_sprite", &args, 3, span)?;
                let id = self.as_index(&args[0], span)?;
                let x = self.as_number(&args[1], span)? as f32;
                let y = self.as_number(&args[2], span)? as f32;
                self.push_draw(DrawCmd::Sprite { id, x, y, scale: 1.0, rotation: 0.0 }, span)
            }
            Builtin::DrawSpriteScaled => {
                self.expect_arity("draw_sprite_scaled", &args, 4, span)?;
                let id = self.as_index(&args[0], span)?;
                let x = self.as_number(&args[1], span)? as f32;
                let y = self.as_number(&args[2], span)? as f32;
                let scale = self.as_number(&args[3], span)? as f32;
                self.push_draw(DrawCmd::Sprite { id, x, y, scale, rotation: 0.0 }, span)
            }
            Builtin::DrawSpriteRotated => {
                self.expect_arity("draw_sprite_rotated", &args, 4, span)?;
                let id = self.as_index(&args[0], span)?;
                let x = self.as_number(&args[1], span)? as f32;
                let y = self.as_number(&args[2], span)? as f32;
                let rotation = self.as_number(&args[3], span)? as f32;
                self.push_draw(DrawCmd::Sprite { id, x, y, scale: 1.0, rotation }, span)
            }
            Builtin::SpriteWidth | Builtin::SpriteHeight => {
                self.expect_arity(b.name(), &args, 1, span)?;
                let id = self.as_index(&args[0], span)?;
                let g = self.gfx_or_err(span)?;
                let size = g.borrow().sprite_sizes.get(&id).copied();
                let n = match size {
                    Some((w, h)) => if b == Builtin::SpriteWidth { w } else { h },
                    None => 0, // not loaded yet (e.g. read during init, before the window opens)
                };
                Ok(Value::Number(n as f64))
            }
            Builtin::LoadSound => {
                self.expect_arity("load_sound", &args, 1, span)?;
                let path = self.as_text(&args[0], span)?;
                if !std::path::Path::new(&path).exists() {
                    return Err(Diagnostic::new(span, format!("couldn't find sound file \"{}\"", path)));
                }
                let g = self.gfx_or_err(span)?;
                let id = g.borrow_mut().queue_sound(path);
                Ok(Value::Number(id as f64))
            }
            Builtin::PlaySound => {
                self.expect_arity("play_sound", &args, 1, span)?;
                let id = self.as_index(&args[0], span)?;
                let g = self.gfx_or_err(span)?;
                g.borrow_mut().sound_plays.push(id);
                Ok(Value::Nothing)
            }
            Builtin::LoadFont => {
                self.expect_arity("load_font", &args, 1, span)?;
                let path = self.as_text(&args[0], span)?;
                if !std::path::Path::new(&path).exists() {
                    return Err(Diagnostic::new(span, format!("couldn't find font file \"{}\"", path)));
                }
                let g = self.gfx_or_err(span)?;
                let id = g.borrow_mut().queue_font(path);
                Ok(Value::Number(id as f64))
            }
            Builtin::After | Builtin::Every => {
                self.expect_arity(b.name(), &args, 2, span)?;
                let secs = self.as_number(&args[0], span)?;
                if secs < 0.0 {
                    return Err(Diagnostic::new(span, format!("{}() delay can't be negative", b.name())));
                }
                let callback = args[1].clone();
                if !is_callable(&callback) {
                    return Err(Diagnostic::new(
                        span,
                        format!("{}() needs a function to run, got a {}", b.name(), callback.type_name()),
                    )
                    .with_hint("pass a function by name, e.g. after(2, spawn_enemy)"));
                }
                let interval = if b == Builtin::Every { Some(secs) } else { None };
                self.timers.push(Timer { remaining: secs, interval, callback });
                Ok(Value::Nothing)
            }
        }
    }

    fn gfx_or_err(&self, span: Span) -> Result<Rc<RefCell<GfxBridge>>, Diagnostic> {
        self.gfx.clone().ok_or_else(|| {
            Diagnostic::new(span, "this drawing/input function only works inside a `game` block")
        })
    }

    fn push_draw(&self, cmd: DrawCmd, span: Span) -> EvalResult {
        let g = self.gfx_or_err(span)?;
        g.borrow_mut().draw.push(cmd);
        Ok(Value::Nothing)
    }

    /// Read a color value: a list of 3 (`rgb`) or 4 (`rgba`) numbers, each 0–255.
    fn as_color(&self, v: &Value, span: Span) -> Result<Color, Diagnostic> {
        let list = match v {
            Value::List(l) => l.borrow(),
            other => {
                return Err(Diagnostic::new(
                    span,
                    format!("expected a color, got a {}", other.type_name()),
                )
                .with_hint("use a named color like `red`, or rgb(r, g, b)"));
            }
        };
        if list.len() != 3 && list.len() != 4 {
            return Err(Diagnostic::new(span, "a color needs 3 numbers (rgb) or 4 (rgba)"));
        }
        let comp = |i: usize| -> u8 {
            if let Some(Value::Number(n)) = list.get(i) {
                n.round().clamp(0.0, 255.0) as u8
            } else {
                0
            }
        };
        let a = if list.len() == 4 { comp(3) } else { 255 };
        Ok(Color(comp(0), comp(1), comp(2), a))
    }

    // ---- small helpers ---------------------------------------------------

    fn eval_bool(&mut self, expr: &Expr, env: &Env) -> Result<bool, Diagnostic> {
        match self.eval(expr, env)? {
            Value::Bool(b) => Ok(b),
            other => Err(Diagnostic::new(
                expr.span(),
                format!("this needs to be true or false, but it's a {}", other.type_name()),
            )
            .with_hint("PlainText has no truthy/falsy values — use a comparison like `x > 0`")),
        }
    }

    fn eval_number(&mut self, expr: &Expr, env: &Env) -> Result<f64, Diagnostic> {
        let v = self.eval(expr, env)?;
        self.as_number(&v, expr.span())
    }

    fn as_number(&self, v: &Value, span: Span) -> Result<f64, Diagnostic> {
        match v {
            Value::Number(n) => Ok(*n),
            other => Err(Diagnostic::new(
                span,
                format!("expected a number, got a {}", other.type_name()),
            )),
        }
    }

    fn as_text(&self, v: &Value, span: Span) -> Result<String, Diagnostic> {
        match v {
            Value::Text(s) => Ok((**s).clone()),
            other => Err(Diagnostic::new(
                span,
                format!("expected text, got a {}", other.type_name()),
            )),
        }
    }

    fn as_index(&self, v: &Value, span: Span) -> Result<usize, Diagnostic> {
        let n = self.as_number(v, span)?;
        if n < 0.0 || n.fract() != 0.0 {
            return Err(Diagnostic::new(
                span,
                format!("index must be a whole number 0 or greater, got {}", format_number(n)),
            ));
        }
        Ok(n as usize)
    }

    fn as_map_key(&self, v: &Value, span: Span) -> Result<MapKey, Diagnostic> {
        match v {
            Value::Text(s) => Ok(MapKey::Text((**s).clone())),
            Value::Number(n) => Ok(MapKey::Number(*n)),
            Value::Bool(b) => Ok(MapKey::Bool(*b)),
            other => Err(Diagnostic::new(
                span,
                format!("dictionary keys must be text, a number, or true/false, got a {}", other.type_name()),
            )),
        }
    }

    fn iterate(&self, seq: Value, span: Span) -> Result<Vec<Value>, Diagnostic> {
        match seq {
            Value::List(l) => Ok(l.borrow().clone()),
            Value::Text(s) => Ok(s.chars().map(|c| Value::text(c.to_string())).collect()),
            Value::Dictionary(m) => Ok(m.borrow().entries.iter().map(|(k, _)| k.to_value()).collect()),
            other => Err(Diagnostic::new(
                span,
                format!("can't loop over a {}", other.type_name()),
            )
            .with_hint("`for every` works on a list, text, or dictionary")),
        }
    }

    fn expect_arity(&self, name: &str, args: &[Value], n: usize, span: Span) -> Result<(), Diagnostic> {
        if args.len() != n {
            Err(Diagnostic::new(
                span,
                format!("{}() takes {} argument(s), but got {}", name, n, args.len()),
            ))
        } else {
            Ok(())
        }
    }

    /// A tiny xorshift RNG returning a float in [0, 1). Good enough for game
    /// randomness; not cryptographic.
    fn next_rand(&self) -> f64 {
        let mut x = self.rng_state.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng_state.set(x);
        // Take the top 53 bits for a uniform double.
        (x >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn is_callable(v: &Value) -> bool {
    matches!(
        v,
        Value::Function(_) | Value::BoundMethod { .. } | Value::Builtin(_)
    )
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x == y,
        (Value::Text(x), Value::Text(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Nothing, Value::Nothing) => true,
        (Value::List(x), Value::List(y)) => Rc::ptr_eq(x, y),
        (Value::Dictionary(x), Value::Dictionary(y)) => Rc::ptr_eq(x, y),
        (Value::Class(x), Value::Class(y)) => Rc::ptr_eq(x, y),
        _ => false,
    }
}

/// Whether a type annotation is an optional (`T?`) type.
fn is_optional_ann(ann: Option<&TypeAnn>) -> bool {
    matches!(ann, Some(TypeAnn::Named { optional: true, .. }))
}

fn stmt_span(stmt: &Stmt) -> Span {
    match stmt {
        Stmt::Assign { span, .. }
        | Stmt::If { span, .. }
        | Stmt::While { span, .. }
        | Stmt::ForEvery { span, .. }
        | Stmt::Repeat { span, .. }
        | Stmt::Loop { span, .. }
        | Stmt::Return { span, .. } => *span,
        Stmt::Function(f) => f.span,
        Stmt::Class(t) => t.span,
        Stmt::Break(s) | Stmt::Continue(s) => *s,
        Stmt::Game(g) => g.span,
        Stmt::Window(w) => w.span,
        Stmt::Expr(e) => e.span(),
    }
}
