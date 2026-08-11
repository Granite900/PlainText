//! A tree-walking interpreter for the core synchronous language.
//!
//! It executes the AST directly. Values live behind the `Rc<RefCell<_>>`
//! bootstrap heap from `value.rs`; a real garbage collector replaces that in a
//! later milestone without changing this file's logic.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::ast::*;
use crate::diagnostics::Diagnostic;
use crate::gc::Heap;
use crate::gfx::{Color, DrawCmd, GfxBridge, MusicCmd, SoundCmd};
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
    /// Standard-library modules brought in with `import` (e.g. `math`).
    imports: HashSet<String>,
    /// The cycle-collecting garbage collector's object registry.
    heap: Heap,
    /// The scope of each currently-executing function/hook call. These are GC
    /// roots: collection can happen while they're on the stack.
    frames: Vec<Env>,
    /// Scopes that must stay rooted for a whole run (e.g. a game block's state).
    persistent_roots: Vec<Env>,
    /// Values held on the Rust side across a collection point (loop snapshots).
    temp_roots: Vec<Value>,
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
            imports: HashSet::new(),
            heap: Heap::new(),
            frames: Vec::new(),
            persistent_roots: Vec::new(),
            temp_roots: Vec::new(),
        }
    }

    // ---- garbage collection ----------------------------------------------

    /// Allocate a list on the collected heap.
    fn new_list(&mut self, items: Vec<Value>) -> Value {
        let rc = Rc::new(RefCell::new(items));
        self.heap.track_list(&rc);
        Value::List(rc)
    }

    /// Allocate a dictionary on the collected heap.
    fn new_dict(&mut self, map: PtMap) -> Value {
        let rc = Rc::new(RefCell::new(map));
        self.heap.track_dict(&rc);
        Value::Dictionary(rc)
    }

    /// Allocate a class instance on the collected heap.
    fn new_instance(&mut self, inst: ClassInstance) -> Value {
        let rc = Rc::new(RefCell::new(inst));
        self.heap.track_instance(&rc);
        Value::Class(rc)
    }

    /// Run one garbage collection: mark from all roots, sweep cyclic garbage.
    fn collect(&mut self) {
        let roots: Vec<Value> = self
            .temp_roots
            .iter()
            .cloned()
            .chain(self.timers.iter().map(|t| t.callback.clone()))
            .collect();
        let mut envs: Vec<Env> = Vec::with_capacity(1 + self.persistent_roots.len() + self.frames.len());
        envs.push(self.globals.clone());
        envs.extend(self.persistent_roots.iter().cloned());
        envs.extend(self.frames.iter().cloned());
        let swept = self.heap.collect(&roots, &envs);
        if std::env::var("PT_GC_TRACE").is_ok() {
            eprintln!("[gc] swept {} cyclic object(s)", swept);
        }
    }

    /// Scan for `import` statements and enable their modules. `import math`
    /// makes the math functions resolvable and declares `pi`/`e`.
    fn process_imports(&mut self, statements: &[Stmt]) {
        for stmt in statements {
            if let Stmt::Import { module, .. } = stmt {
                self.imports.insert(module.clone());
            }
        }
        if self.imports.contains("math") {
            env_declare(&self.globals, "pi", Value::Number(std::f64::consts::PI));
            env_declare(&self.globals, "e", Value::Number(std::f64::consts::E));
        }
        if self.imports.contains("ai") {
            // Optimizer and device names as bare words, so `optimizer: adam` and
            // `device: rocm` need no quotes.
            for word in ["sgd", "adam", "momentum", "rmsprop", "cpu", "gpu", "auto", "cuda", "rocm", "mps", "vulkan", "dx12"] {
                env_declare(&self.globals, word, Value::text(word));
            }
        }
        if self.imports.contains("web") {
            crate::web::install(&self.globals);
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
        self.process_imports(&program.statements);
        self.hoist(&program.statements)?;
        for stmt in &program.statements {
            if matches!(stmt, Stmt::Function(_) | Stmt::Class(_) | Stmt::Game(_)) {
                continue;
            }
            self.exec_stmt(stmt, &self.globals.clone())?;
        }
        let scope = Scope::new_child(&self.globals);
        // The game's state scope stays rooted for the whole run — hooks and
        // timers can trigger collection at any time.
        self.persistent_roots.push(scope.clone());
        for stmt in &game.init {
            self.exec_stmt(stmt, &scope)?;
        }
        Ok(scope)
    }

    /// Hoist declarations and run top-level statements, returning the global
    /// scope. Used by the window runner (window state lives at top level).
    pub fn prepare(&mut self, program: &Program) -> Result<Env, Diagnostic> {
        self.process_imports(&program.statements);
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
            "text_field" => UiKind::TextField,
            "checkbox" => UiKind::Checkbox,
            "slider" => UiKind::Slider,
            "image" => UiKind::Image,
            "scroll" => UiKind::Scroll,
            "list" => UiKind::List,
            "dropdown" => UiKind::Dropdown,
            other => {
                return Err(Diagnostic::new(w.span, format!("unknown widget `{}`", other))
                    .with_hint("widgets are column, row, text, button, spacer, text_field, checkbox, slider, image, scroll, list, dropdown"));
            }
        };
        let mut node = UiNode::new(kind);
        if let Some(label) = &w.label {
            node.text = Some(self.eval(label, scope)?.display());
        }
        for (name, expr) in &w.props {
            // `bind:` names a variable to read from and write back to; it needs
            // the identifier itself, not just its current value.
            if name == "bind" {
                let var = match expr {
                    Expr::Ident(n, _) => n.clone(),
                    _ => {
                        return Err(Diagnostic::new(expr.span(), "bind needs a variable name")
                            .with_hint("write bind: myVariable"));
                    }
                };
                let v = self.eval(expr, scope)?;
                node.bind = Some(var);
                self.set_widget_value(&mut node, v, expr.span())?;
                continue;
            }
            let v = self.eval(expr, scope)?;
            self.apply_widget_prop(&mut node, name, v, expr.span())?;
        }
        // An image with no explicit size takes the sprite's natural size.
        if node.kind == UiKind::Image {
            if let (Some(id), Some(gfx)) = (node.sprite, &self.gfx) {
                if let Some((sw, sh)) = gfx.borrow().sprite_sizes.get(&id).copied() {
                    node.props.width.get_or_insert(sw as f32);
                    node.props.height.get_or_insert(sh as f32);
                }
            }
        }
        for child in &w.children {
            let c = self.build_widget(child, scope)?;
            node.children.push(c);
        }
        Ok(node)
    }

    /// Set an interactive widget's current value from a bound/`value:` prop,
    /// routed by the widget's kind.
    fn set_widget_value(&self, node: &mut UiNode, v: Value, span: Span) -> Result<(), Diagnostic> {
        match node.kind {
            UiKind::Checkbox => {
                node.checked = match v {
                    Value::Bool(b) => b,
                    other => {
                        return Err(Diagnostic::new(
                            span,
                            format!("a checkbox holds a true/false value, got a {}", other.type_name()),
                        ));
                    }
                };
            }
            UiKind::Slider => node.number = self.as_number(&v, span)? as f32,
            UiKind::TextField => node.text = Some(v.display()),
            UiKind::List | UiKind::Dropdown => {
                node.selected = self.as_number(&v, span)? as i32;
            }
            _ => {}
        }
        Ok(())
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
            "value" | "checked" => self.set_widget_value(node, v, span)?,
            "min" => node.min = self.as_number(&v, span)? as f32,
            "max" => node.max = self.as_number(&v, span)? as f32,
            "step" => node.step = self.as_number(&v, span)? as f32,
            "multiline" => {
                node.multiline = match v {
                    Value::Bool(b) => b,
                    other => {
                        return Err(Diagnostic::new(
                            span,
                            format!("multiline needs true or false, got a {}", other.type_name()),
                        ));
                    }
                };
            }
            "items" => {
                match v {
                    Value::List(items) => {
                        let mut out = Vec::new();
                        for item in items.borrow().iter() {
                            out.push(self.as_text(item, span)?);
                        }
                        node.items = out;
                    }
                    other => {
                        return Err(Diagnostic::new(
                            span,
                            format!("items needs a list of text, got a {}", other.type_name()),
                        ));
                    }
                }
            }
            "on_change" => {
                if !is_callable(&v) {
                    return Err(Diagnostic::new(span, "on_change needs a function")
                        .with_hint("pass a function, e.g. on_change: make function (new) { name = new }"));
                }
                node.on_change = Some(v);
            }
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

    /// Call an `on_change` handler with the widget's new value.
    pub fn call_on_change(&mut self, callback: &Value, arg: Value) -> Result<(), Diagnostic> {
        self.call_value(callback.clone(), vec![arg], Span::new(0, 0))?;
        Ok(())
    }

    /// Write a widget's new value back to the variable named by its `bind:`.
    pub fn assign_var(&self, scope: &Env, name: &str, value: Value) {
        env_set(scope, name, value);
    }

    /// Run one lifecycle hook (start/update/draw), binding its parameters.
    pub fn run_hook(&mut self, scope: &Env, hook: &Hook, args: Vec<Value>) -> Result<(), Diagnostic> {
        let child = Scope::new_child(scope);
        for (p, v) in hook.params.iter().zip(args) {
            env_declare(&child, p, v);
        }
        self.frames.push(child.clone());
        let result = self.exec_block(&hook.body, &child);
        self.frames.pop();
        result?;
        Ok(())
    }

    /// Run a whole program: hoist declarations, execute top-level statements,
    /// then call `main` if one is defined.
    pub fn run(&mut self, program: &Program) -> Result<(), Diagnostic> {
        self.process_imports(&program.statements);
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

    /// Evaluate one REPL entry: hoist its declarations, run its statements
    /// against the persistent global scope, and return the value of a trailing
    /// bare expression (so `2 + 2` prints `4`). Declarations and assignments
    /// stay available on later lines. No static checking — runtime checks only.
    pub fn eval_repl(&mut self, program: &Program) -> Result<Option<Value>, Diagnostic> {
        self.process_imports(&program.statements);
        self.hoist(&program.statements)?;
        let last = program.statements.len().wrapping_sub(1);
        let mut result = None;
        for (i, stmt) in program.statements.iter().enumerate() {
            if matches!(stmt, Stmt::Function(_) | Stmt::Class(_)) {
                continue;
            }
            if i == last {
                if let Stmt::Expr(e) = stmt {
                    let v = self.eval(e, &self.globals.clone())?;
                    result = if matches!(v, Value::Nothing) { None } else { Some(v) };
                    continue;
                }
            }
            self.exec_stmt(stmt, &self.globals.clone())?;
        }
        Ok(result)
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
            // Statement boundaries are safe points for collection: no partly-
            // evaluated expression temporaries are live here.
            if self.heap.should_collect() {
                self.collect();
            }
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
                // The snapshot lives on the Rust stack across the loop body, so
                // root it — otherwise the GC couldn't see items whose only
                // remaining reference is this snapshot.
                let base = self.temp_roots.len();
                self.temp_roots.extend(items.iter().cloned());
                let mut outcome = Ok(Flow::Normal);
                for item in items {
                    env_declare(env, var, item);
                    match self.exec_block(body, env) {
                        Ok(Flow::Break) => break,
                        Ok(Flow::Continue) | Ok(Flow::Normal) => {}
                        Ok(ret @ Flow::Return(_)) => {
                            outcome = Ok(ret);
                            break;
                        }
                        Err(e) => {
                            outcome = Err(e);
                            break;
                        }
                    }
                }
                self.temp_roots.truncate(base);
                outcome
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
            Stmt::Import { .. } => Ok(Flow::Normal), // handled in process_imports
            Stmt::ImportFile { .. } => Ok(Flow::Normal), // spliced away before running
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
                    Value::Body(b) => self.set_body_field(&b, name, value, *span),
                    Value::Hitbox(h) => self.set_hitbox_field(&h, name, value, *span),
                    Value::PhysicsWorld(w) => {
                        if name == "gravity" {
                            w.borrow_mut().gravity = self.as_number(&value, *span)?;
                            Ok(())
                        } else {
                            Err(Diagnostic::new(
                                *span,
                                format!("a physics world has no field `{}`", name),
                            )
                            .with_hint("worlds have gravity; use methods like .add / .step"))
                        }
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
            Expr::Try { expr, .. } => {
                // Catch a real runtime error and turn it into `nothing`. An
                // `exit(...)` request (carried as a diagnostic) is never caught —
                // it must still stop the program.
                match self.eval(expr, env) {
                    Ok(v) => Ok(v),
                    Err(d) if d.exit.is_some() => Err(d),
                    Err(_) => Ok(Value::Nothing),
                }
            }
            Expr::Otherwise { value, fallback, .. } => {
                // The value's own errors are NOT caught here (use `try` for that);
                // `otherwise` only supplies a default when the value is `nothing`.
                let v = self.eval(value, env)?;
                if matches!(v, Value::Nothing) {
                    self.eval(fallback, env)
                } else {
                    Ok(v)
                }
            }
            Expr::ListLit { items, .. } => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.eval(item, env)?);
                }
                Ok(self.new_list(values))
            }
            Expr::DictionaryLit { entries, span } => {
                let mut map = PtMap::new();
                for (k, v) in entries {
                    let key_v = self.eval(k, env)?;
                    let key = self.as_map_key(&key_v, *span)?;
                    let val = self.eval(v, env)?;
                    map.set(key, val);
                }
                Ok(self.new_dict(map))
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
            Expr::Function { decl, .. } => Ok(Value::Function(Rc::new(FunctionObj {
                decl: decl.clone(),
                closure: env.clone(),
            }))),
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
            // Math functions are only available after `import math`.
            if b.is_math() && !self.imports.contains("math") {
                return Err(Diagnostic::new(span, format!("`{}` needs the math module", name))
                    .with_hint("add `import math` at the top of your file"));
            }
            if b.is_ai() && !self.imports.contains("ai") {
                return Err(Diagnostic::new(span, format!("`{}` needs the ai module", name))
                    .with_hint("add `import ai` at the top of your file"));
            }
            if b.is_gamekit() && !self.imports.contains("gamekit") {
                return Err(Diagnostic::new(span, format!("`{}` needs the gamekit module", name))
                    .with_hint("add `import gamekit` at the top of your file"));
            }
            if b.is_web() && !self.imports.contains("web") {
                return Err(Diagnostic::new(span, format!("`{}` needs the web module", name))
                    .with_hint("add `import web` at the top of your file"));
            }
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

        Ok(self.new_instance(ClassInstance { def, fields: field_values }))
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
            Value::Body(b) => self.get_body_field(b, name, span),
            Value::Hitbox(h) => self.get_hitbox_field(h, name, span),
            Value::PhysicsWorld(w) => match name {
                "gravity" => Ok(Value::Number(w.borrow().gravity)),
                _ => Err(Diagnostic::new(
                    span,
                    format!("a physics world has no field `{}`", name),
                )
                .with_hint("use methods: .add, .remove, .add_tilemap, .step, .hits, .sync_hitboxes")),
            },
            Value::Tilemap(m) => self.get_tilemap_field(m, name, span),
            _ => Err(Diagnostic::new(
                span,
                format!("a {} has no field `{}`", obj.type_name(), name),
            )
            .with_hint("methods like `.length()` must be called; field access works on classes and gamekit bodies")),
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
        let scope = Scope::new_child(&func.closure);
        if let Some(sv) = self_val {
            env_declare(&scope, "self", sv);
        }

        let required = func.decl.params.iter().filter(|p| p.default.is_none()).count();
        if args.len() > func.decl.params.len() || args.len() < required {
            return Err(Diagnostic::new(
                span,
                format!(
                    "`{}` takes {} argument(s), but got {}",
                    func.decl.name,
                    func.decl.params.len(),
                    args.len()
                ),
            ));
        }

        // The call's scope is a GC root while its body runs (collection can
        // happen at any statement boundary inside it).
        self.frames.push(scope.clone());
        let result = self.run_call_body(&func.decl, args, &scope);
        self.frames.pop();
        result
    }

    fn run_call_body(&mut self, decl: &FunctionDecl, args: Vec<Value>, scope: &Env) -> EvalResult {
        let mut args = args.into_iter();
        for param in &decl.params {
            let value = match args.next() {
                Some(v) => v,
                None => {
                    // Must have a default (checked by arity in the caller).
                    let default = param.default.as_ref().unwrap();
                    self.eval(default, scope)?
                }
            };
            env_declare(scope, &param.name, value);
        }

        match self.exec_block(&decl.body, scope)? {
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
            Value::Network(net) => self.network_method(&net.clone(), name, args, span),
            Value::Body(b) => self.body_method(&b.clone(), name, args, span),
            Value::Hitbox(h) => self.hitbox_method(&h.clone(), name, args, span),
            Value::PhysicsWorld(w) => self.world_method(&w.clone(), name, args, span),
            Value::Tilemap(m) => self.tilemap_method(&m.clone(), name, args, span),
            Value::WebModule => self.web_method(name, args, span),
            other => Err(Diagnostic::new(
                span,
                format!("a {} has no method `{}`", other.type_name(), name),
            )),
        }
    }

    fn web_method(&mut self, name: &str, args: Vec<Value>, span: Span) -> EvalResult {
        if !self.imports.contains("web") {
            return Err(Diagnostic::new(span, format!("`web.{}` needs the web module", name))
                .with_hint("add `import web` at the top of your file"));
        }
        match name {
            "get" => {
                self.expect_arity("get", &args, 1, span)?;
                let url = self.as_text(&args[0], span)?;
                let body = crate::web::http_get(&url, span)?;
                Ok(Value::text(body))
            }
            "get_json" => {
                self.expect_arity("get_json", &args, 1, span)?;
                let url = self.as_text(&args[0], span)?;
                crate::web::http_get_json(&url, span)
            }
            "post_json" => {
                self.expect_arity("post_json", &args, 2, span)?;
                let url = self.as_text(&args[0], span)?;
                let body = crate::web::http_post_json(&url, &args[1], span)?;
                Ok(Value::text(body))
            }
            _ => Err(Diagnostic::new(
                span,
                format!("web has no method `{}`", name),
            ).with_hint("try `get`, `get_json`, or `post_json`")),
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
            "sorted" => {
                self.expect_arity(name, &args, 0, span)?;
                let mut items = list.borrow().clone();
                let all_num = items.iter().all(|v| matches!(v, Value::Number(_)));
                let all_text = items.iter().all(|v| matches!(v, Value::Text(_)));
                if !all_num && !all_text {
                    return Err(Diagnostic::new(span, "sorted() needs a list of all numbers or all text")
                        .with_hint("sort a mixed or nested list yourself with a loop"));
                }
                use std::cmp::Ordering;
                if all_num {
                    items.sort_by(|a, b| match (a, b) {
                        (Value::Number(x), Value::Number(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
                        _ => Ordering::Equal,
                    });
                } else {
                    items.sort_by(|a, b| match (a, b) {
                        (Value::Text(x), Value::Text(y)) => x.cmp(y),
                        _ => Ordering::Equal,
                    });
                }
                Ok(self.new_list(items))
            }
            "transformed_by" => {
                self.expect_arity(name, &args, 1, span)?;
                let f = args.into_iter().next().unwrap();
                self.require_callable(&f, "transformed_by", span)?;
                let items: Vec<Value> = list.borrow().clone();
                let base = self.temp_roots.len();
                self.temp_roots.extend(items.iter().cloned());
                let mut out = Vec::with_capacity(items.len());
                let mut err = None;
                for item in items {
                    match self.call_value(f.clone(), vec![item], span) {
                        Ok(r) => {
                            self.temp_roots.push(r.clone());
                            out.push(r);
                        }
                        Err(e) => {
                            err = Some(e);
                            break;
                        }
                    }
                }
                self.temp_roots.truncate(base);
                if let Some(e) = err {
                    return Err(e);
                }
                Ok(self.new_list(out))
            }
            "kept_if" => {
                self.expect_arity(name, &args, 1, span)?;
                let f = args.into_iter().next().unwrap();
                self.require_callable(&f, "kept_if", span)?;
                let items: Vec<Value> = list.borrow().clone();
                let base = self.temp_roots.len();
                self.temp_roots.extend(items.iter().cloned());
                let mut out = Vec::new();
                let mut err = None;
                for item in items {
                    match self.call_value(f.clone(), vec![item.clone()], span) {
                        Ok(Value::Bool(true)) => out.push(item),
                        Ok(Value::Bool(false)) => {}
                        Ok(other) => {
                            err = Some(Diagnostic::new(
                                span,
                                format!("kept_if's function must return true or false, but it returned a {}", other.type_name()),
                            ));
                            break;
                        }
                        Err(e) => {
                            err = Some(e);
                            break;
                        }
                    }
                }
                self.temp_roots.truncate(base);
                if let Some(e) = err {
                    return Err(e);
                }
                Ok(self.new_list(out))
            }
            "combined" => {
                self.expect_arity(name, &args, 2, span)?;
                let mut it = args.into_iter();
                let start = it.next().unwrap();
                let f = it.next().unwrap();
                self.require_callable(&f, "combined", span)?;
                let items: Vec<Value> = list.borrow().clone();
                let base = self.temp_roots.len();
                self.temp_roots.extend(items.iter().cloned());
                let acc_slot = self.temp_roots.len();
                self.temp_roots.push(start.clone());
                let mut acc = start;
                let mut err = None;
                for item in items {
                    match self.call_value(f.clone(), vec![acc.clone(), item], span) {
                        Ok(r) => {
                            acc = r;
                            self.temp_roots[acc_slot] = acc.clone();
                        }
                        Err(e) => {
                            err = Some(e);
                            break;
                        }
                    }
                }
                self.temp_roots.truncate(base);
                if let Some(e) = err {
                    return Err(e);
                }
                Ok(acc)
            }
            _ => Err(Diagnostic::new(span, format!("a list has no method `{}`", name)).with_hint(
                "lists have length, is_empty, append, pop, get, contains, first, last, index_of, remove_at, reversed, join, sorted, transformed_by, kept_if, combined",
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

    fn network_method(
        &mut self,
        net: &Rc<RefCell<crate::nn::Net>>,
        name: &str,
        args: Vec<Value>,
        span: Span,
    ) -> EvalResult {
        match name {
            "predict" => {
                self.expect_arity(name, &args, 1, span)?;
                let input = self.as_number_row(&args[0], span)?;
                let n = net.borrow();
                if input.len() != n.inputs() {
                    return Err(Diagnostic::new(
                        span,
                        format!("this network expects {} input(s), but that example has {}", n.inputs(), input.len()),
                    ));
                }
                let out = n.predict(&input);
                Ok(self.new_list(out.into_iter().map(Value::Number).collect()))
            }
            "train" | "train_once" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err(Diagnostic::new(
                        span,
                        format!("{}() takes examples, answers, and optional settings", name),
                    ));
                }
                let inputs = self.as_number_rows(&args[0], span)?;
                let targets = self.as_number_rows(&args[1], span)?;
                if inputs.len() != targets.len() {
                    return Err(Diagnostic::new(
                        span,
                        format!("got {} examples but {} answers — they must line up", inputs.len(), targets.len()),
                    ));
                }
                let opts = args.get(2);
                self.check_shapes(net, &inputs, &targets, span)?;
                let cfg = self.train_cfg(opts, span)?;
                let loss = if name == "train_once" {
                    // The live, per-frame path always runs on the CPU: one epoch
                    // on a GPU would be dominated by upload/readback overhead.
                    net.borrow_mut().train_epoch(&inputs, &targets, &cfg)
                } else {
                    let epochs = self.opt_number(opts, "epochs", 1000.0, span)?.max(0.0) as u64;
                    let device = net.borrow().device();
                    self.run_training(net, device, &cfg, &inputs, &targets, epochs, span)?
                };
                Ok(Value::Number(loss))
            }
            "loss" | "error" => {
                self.expect_arity(name, &args, 2, span)?;
                let inputs = self.as_number_rows(&args[0], span)?;
                let targets = self.as_number_rows(&args[1], span)?;
                Ok(Value::Number(net.borrow().loss(&inputs, &targets)))
            }
            "save" => {
                self.expect_arity(name, &args, 1, span)?;
                let path = self.as_text(&args[0], span)?;
                net.borrow().save(&path).map(|_| Value::Nothing).map_err(|e| {
                    Diagnostic::new(span, format!("couldn't save the network to \"{}\": {}", path, e))
                })
            }
            _ => Err(Diagnostic::new(span, format!("a neural network has no method `{}`", name))
                .with_hint("networks have train, train_once, predict, loss, save")),
        }
    }

    fn get_body_field(&self, b: &Rc<RefCell<crate::gamekit::Body>>, name: &str, span: Span) -> EvalResult {
        let b = b.borrow();
        Ok(match name {
            "x" => Value::Number(b.x),
            "y" => Value::Number(b.y),
            "width" => Value::Number(b.width),
            "height" => Value::Number(b.height),
            "vx" => Value::Number(b.vx),
            "vy" => Value::Number(b.vy),
            "solid" => Value::Bool(b.solid),
            "static" => Value::Bool(b.is_static),
            "on_ground" => Value::Bool(b.on_ground),
            "center_x" => Value::Number(b.center_x()),
            "center_y" => Value::Number(b.center_y()),
            _ => {
                return Err(Diagnostic::new(span, format!("a body has no field `{}`", name))
                    .with_hint("bodies have x, y, width, height, vx, vy, solid, static, on_ground"));
            }
        })
    }

    fn set_body_field(
        &self,
        b: &Rc<RefCell<crate::gamekit::Body>>,
        name: &str,
        value: Value,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let mut b = b.borrow_mut();
        match name {
            "x" => b.x = self.as_number(&value, span)?,
            "y" => b.y = self.as_number(&value, span)?,
            "width" => b.width = self.as_number(&value, span)?.max(0.0),
            "height" => b.height = self.as_number(&value, span)?.max(0.0),
            "vx" => b.vx = self.as_number(&value, span)?,
            "vy" => b.vy = self.as_number(&value, span)?,
            "solid" => b.solid = self.as_bool_val(&value, span)?,
            "static" => b.is_static = self.as_bool_val(&value, span)?,
            "on_ground" => b.on_ground = self.as_bool_val(&value, span)?,
            _ => {
                return Err(Diagnostic::new(span, format!("can't set field `{}` on a body", name)));
            }
        }
        Ok(())
    }

    fn get_hitbox_field(&self, h: &Rc<RefCell<crate::gamekit::Hitbox>>, name: &str, span: Span) -> EvalResult {
        let h = h.borrow();
        Ok(match name {
            "offset_x" => Value::Number(h.offset_x),
            "offset_y" => Value::Number(h.offset_y),
            "width" => Value::Number(h.width),
            "height" => Value::Number(h.height),
            "kind" => Value::text(h.kind.clone()),
            "active" => Value::Bool(h.active),
            "x" => Value::Number(h.world_xy().0),
            "y" => Value::Number(h.world_xy().1),
            _ => {
                return Err(Diagnostic::new(span, format!("a hitbox has no field `{}`", name))
                    .with_hint("hitboxes have offset_x, offset_y, width, height, kind, active"));
            }
        })
    }

    fn set_hitbox_field(
        &self,
        h: &Rc<RefCell<crate::gamekit::Hitbox>>,
        name: &str,
        value: Value,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let mut h = h.borrow_mut();
        match name {
            "offset_x" => h.offset_x = self.as_number(&value, span)?,
            "offset_y" => h.offset_y = self.as_number(&value, span)?,
            "width" => h.width = self.as_number(&value, span)?.max(0.0),
            "height" => h.height = self.as_number(&value, span)?.max(0.0),
            "kind" => h.kind = self.as_text(&value, span)?,
            "active" => h.active = self.as_bool_val(&value, span)?,
            _ => {
                return Err(Diagnostic::new(span, format!("can't set field `{}` on a hitbox", name)));
            }
        }
        Ok(())
    }

    fn body_method(
        &mut self,
        body: &Rc<RefCell<crate::gamekit::Body>>,
        name: &str,
        args: Vec<Value>,
        span: Span,
    ) -> EvalResult {
        match name {
            "move" => {
                self.expect_arity(name, &args, 2, span)?;
                let dx = self.as_number(&args[0], span)?;
                let dy = self.as_number(&args[1], span)?;
                body.borrow_mut().move_by(dx, dy);
                Ok(Value::Nothing)
            }
            "set_velocity" => {
                self.expect_arity(name, &args, 2, span)?;
                let vx = self.as_number(&args[0], span)?;
                let vy = self.as_number(&args[1], span)?;
                body.borrow_mut().set_velocity(vx, vy);
                Ok(Value::Nothing)
            }
            "bump" => {
                self.expect_arity(name, &args, 2, span)?;
                let vx = self.as_number(&args[0], span)?;
                let vy = self.as_number(&args[1], span)?;
                body.borrow_mut().bump(vx, vy);
                Ok(Value::Nothing)
            }
            "jump" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(Diagnostic::new(span, "jump takes a speed, and optional force: true")
                        .with_hint("hero.jump(700)"));
                }
                let speed = self.as_number(&args[0], span)?;
                let force = match args.get(1) {
                    Some(Value::Dictionary(_)) => self.opt_bool(args.get(1), "force", false, span)?,
                    Some(v) => self.as_bool_val(v, span)?,
                    None => false,
                };
                Ok(Value::Bool(body.borrow_mut().jump(speed, force)))
            }
            _ => Err(Diagnostic::new(span, format!("a body has no method `{}`", name))
                .with_hint("bodies have move, set_velocity, bump, jump")),
        }
    }

    fn hitbox_method(
        &mut self,
        hb: &Rc<RefCell<crate::gamekit::Hitbox>>,
        name: &str,
        args: Vec<Value>,
        span: Span,
    ) -> EvalResult {
        match name {
            "overlaps" => {
                self.expect_arity(name, &args, 1, span)?;
                match &args[0] {
                    Value::Hitbox(other) => Ok(Value::Bool(crate::gamekit::hitboxes_overlap(
                        &hb.borrow(),
                        &other.borrow(),
                    ))),
                    other => Err(Diagnostic::new(
                        span,
                        format!("overlaps needs a hitbox, got a {}", other.type_name()),
                    )),
                }
            }
            _ => Err(Diagnostic::new(span, format!("a hitbox has no method `{}`", name))
                .with_hint("hitboxes have overlaps(other)")),
        }
    }

    fn world_method(
        &mut self,
        world: &Rc<RefCell<crate::gamekit::World>>,
        name: &str,
        args: Vec<Value>,
        span: Span,
    ) -> EvalResult {
        match name {
            "add" => {
                self.expect_arity(name, &args, 1, span)?;
                match &args[0] {
                    Value::Body(b) => world.borrow_mut().add_body(b.clone()),
                    Value::Hitbox(h) => world.borrow_mut().add_hitbox(h.clone()),
                    Value::Tilemap(m) => world.borrow_mut().add_tilemap(m.clone()),
                    other => {
                        return Err(Diagnostic::new(
                            span,
                            format!(
                                "world.add needs a body, hitbox, or tilemap, got a {}",
                                other.type_name()
                            ),
                        ).with_hint("for solid tiles use world.add_tilemap(map, solid_tiles: [\"#\"])"));
                    }
                }
                Ok(Value::Nothing)
            }
            "remove" => {
                self.expect_arity(name, &args, 1, span)?;
                match &args[0] {
                    Value::Body(b) => {
                        world.borrow_mut().remove_body(b);
                    }
                    Value::Hitbox(h) => {
                        world.borrow_mut().remove_hitbox(h);
                    }
                    other => {
                        return Err(Diagnostic::new(
                            span,
                            format!(
                                "world.remove needs a body or hitbox, got a {}",
                                other.type_name()
                            ),
                        ));
                    }
                }
                Ok(Value::Nothing)
            }
            "add_tilemap" => {
                // map, solid_tiles: ["#"]  → args [map, options-dict] or just [map]
                if args.is_empty() || args.len() > 2 {
                    return Err(Diagnostic::new(
                        span,
                        "add_tilemap takes a tilemap, and optional solid_tiles: [\"#\"]",
                    ));
                }
                let map = match &args[0] {
                    Value::Tilemap(m) => m.clone(),
                    other => {
                        return Err(Diagnostic::new(
                            span,
                            format!("add_tilemap needs a tilemap, got a {}", other.type_name()),
                        ));
                    }
                };
                let solids = self.read_solid_tiles(args.get(1), span)?;
                if !solids.is_empty() {
                    map.borrow_mut().set_solid_tiles(solids);
                }
                world.borrow_mut().add_tilemap(map);
                Ok(Value::Nothing)
            }
            "step" => {
                self.expect_arity(name, &args, 1, span)?;
                let delta = self.as_number(&args[0], span)?;
                world.borrow_mut().step(delta);
                Ok(Value::Nothing)
            }
            "sync_hitboxes" => {
                // Hitboxes follow their owner automatically; kept for readable call sites.
                self.expect_arity(name, &args, 0, span)?;
                Ok(Value::Nothing)
            }
            "hits" => {
                self.expect_arity(name, &args, 2, span)?;
                match (&args[0], &args[1]) {
                    (Value::Hitbox(a), Value::Hitbox(h)) => {
                        Ok(Value::Bool(world.borrow_mut().hits(a, h)))
                    }
                    _ => Err(Diagnostic::new(span, "world.hits needs an attack hitbox and a hurt hitbox")),
                }
            }
            _ => Err(Diagnostic::new(span, format!("a physics world has no method `{}`", name))
                .with_hint("worlds have add, remove, add_tilemap, step, hits, sync_hitboxes")),
        }
    }

    fn get_tilemap_field(
        &self,
        m: &Rc<RefCell<crate::gamekit::Tilemap>>,
        name: &str,
        span: Span,
    ) -> EvalResult {
        let m = m.borrow();
        Ok(match name {
            "cell_size" => Value::Number(m.cell_size),
            "width" => Value::Number(m.width() as f64),
            "height" => Value::Number(m.height() as f64),
            _ => {
                return Err(Diagnostic::new(span, format!("a tilemap has no field `{}`", name))
                    .with_hint("tilemaps have cell_size, width, height, and tile_at(x, y)"));
            }
        })
    }

    fn tilemap_method(
        &mut self,
        map: &Rc<RefCell<crate::gamekit::Tilemap>>,
        name: &str,
        args: Vec<Value>,
        span: Span,
    ) -> EvalResult {
        match name {
            "tile_at" => {
                self.expect_arity(name, &args, 2, span)?;
                let x = self.as_number(&args[0], span)?.floor() as i64;
                let y = self.as_number(&args[1], span)?.floor() as i64;
                Ok(match map.borrow().tile_at(x, y) {
                    Some(ch) => Value::text(ch.to_string()),
                    None => Value::Nothing,
                })
            }
            _ => Err(Diagnostic::new(span, format!("a tilemap has no method `{}`", name))
                .with_hint("tilemaps have tile_at(x, y)")),
        }
    }

    /// Read `solid_tiles:` from an options dict (list of 1-char texts, or one text of chars).
    fn read_solid_tiles(&self, opts: Option<&Value>, span: Span) -> Result<Vec<char>, Diagnostic> {
        let Some(raw) = dict_get(opts, "solid_tiles").or_else(|| {
            // Allow a bare list/text as the second positional arg.
            match opts {
                Some(Value::List(_)) | Some(Value::Text(_)) => opts.cloned(),
                _ => None,
            }
        }) else {
            return Ok(Vec::new());
        };
        match raw {
            Value::List(items) => {
                let mut out = Vec::new();
                for item in items.borrow().iter() {
                    let s = self.as_text(item, span)?;
                    let mut chars = s.chars();
                    let Some(ch) = chars.next() else {
                        return Err(Diagnostic::new(
                            span,
                            "solid_tiles entries must be one character each, e.g. \"#\"",
                        ));
                    };
                    if chars.next().is_some() {
                        return Err(Diagnostic::new(
                            span,
                            format!("solid_tiles entry \"{}\" should be a single character", s),
                        ));
                    }
                    out.push(ch);
                }
                Ok(out)
            }
            Value::Text(s) => Ok(s.chars().collect()),
            other => Err(Diagnostic::new(
                span,
                format!(
                    "solid_tiles needs a list of characters like [\"#\"], got a {}",
                    other.type_name()
                ),
            )),
        }
    }

    fn read_tilemap_rows(&self, opts: Option<&Value>, span: Span) -> Result<Vec<String>, Diagnostic> {
        let rows_v = dict_get(opts, "rows").ok_or_else(|| {
            Diagnostic::new(span, "tilemap needs rows: [\"###\", \"...\"]")
                .with_hint("each text item is one row of tile characters")
        })?;
        match rows_v {
            Value::List(items) => {
                let mut rows = Vec::new();
                for item in items.borrow().iter() {
                    rows.push(self.as_text(item, span)?);
                }
                if rows.is_empty() {
                    return Err(Diagnostic::new(span, "tilemap rows can't be empty"));
                }
                Ok(rows)
            }
            other => Err(Diagnostic::new(
                span,
                format!("tilemap rows must be a list of text, got a {}", other.type_name()),
            )),
        }
    }

    fn as_bool_val(&self, v: &Value, span: Span) -> Result<bool, Diagnostic> {
        match v {
            Value::Bool(b) => Ok(*b),
            other => Err(Diagnostic::new(
                span,
                format!("expected true or false, got a {}", other.type_name()),
            )),
        }
    }

    fn opt_bool(&self, opts: Option<&Value>, key: &str, default: bool, span: Span) -> Result<bool, Diagnostic> {
        match dict_get(opts, key) {
            Some(v) => self.as_bool_val(&v, span),
            None => Ok(default),
        }
    }

    /// Run `epochs` of training, dispatching on the network's chosen device:
    /// no device → the classic online CPU trainer; `cpu` → the batched CPU
    /// trainer; a GPU kind → the GPU, falling back to the batched CPU trainer
    /// (with a note) if no such device can be opened.
    fn run_training(
        &self,
        net: &Rc<RefCell<crate::nn::Net>>,
        device: Option<crate::gpu::DeviceKind>,
        cfg: &crate::nn::TrainCfg,
        inputs: &[Vec<f64>],
        targets: &[Vec<f64>],
        epochs: u64,
        span: Span,
    ) -> Result<f64, Diagnostic> {
        use crate::gpu::DeviceKind;
        match device {
            None => Ok(self.train_cpu(net, cfg, inputs, targets, epochs, false)),
            Some(DeviceKind::Cpu) => {
                println!("neural network: training on the cpu");
                Ok(self.train_cpu(net, cfg, inputs, targets, epochs, true))
            }
            Some(kind) => match crate::gpu::open(kind) {
                Ok(gpu) => {
                    println!("neural network: training on {}", gpu.info);
                    let mut n = net.borrow_mut();
                    crate::gpu::train(&gpu, &mut n, cfg, inputs, targets, epochs)
                        .map_err(|e| Diagnostic::new(span, format!("GPU training failed: {}", e)))
                }
                Err(reason) => {
                    println!("neural network: no GPU available ({}), using the cpu", reason);
                    Ok(self.train_cpu(net, cfg, inputs, targets, epochs, true))
                }
            },
        }
    }

    /// The CPU trainer loop. `batched` picks full-dataset gradient descent (the
    /// GPU's algorithm); otherwise the classic per-sample online updates.
    fn train_cpu(
        &self,
        net: &Rc<RefCell<crate::nn::Net>>,
        cfg: &crate::nn::TrainCfg,
        inputs: &[Vec<f64>],
        targets: &[Vec<f64>],
        epochs: u64,
        batched: bool,
    ) -> f64 {
        let mut last = net.borrow().loss(inputs, targets);
        for _ in 0..epochs {
            let mut n = net.borrow_mut();
            last = if batched {
                n.train_epoch_batched(inputs, targets, cfg)
            } else {
                n.train_epoch(inputs, targets, cfg)
            };
        }
        last
    }

    /// Check that every example matches the network's input width and every
    /// answer matches its output width.
    fn check_shapes(
        &self,
        net: &Rc<RefCell<crate::nn::Net>>,
        inputs: &[Vec<f64>],
        targets: &[Vec<f64>],
        span: Span,
    ) -> Result<(), Diagnostic> {
        let n = net.borrow();
        for (i, row) in inputs.iter().enumerate() {
            if row.len() != n.inputs() {
                return Err(Diagnostic::new(
                    span,
                    format!("example {} has {} number(s), but the network expects {} input(s)", i + 1, row.len(), n.inputs()),
                ));
            }
        }
        for (i, row) in targets.iter().enumerate() {
            if row.len() != n.outputs() {
                return Err(Diagnostic::new(
                    span,
                    format!("answer {} has {} number(s), but the network has {} output(s)", i + 1, row.len(), n.outputs()),
                ));
            }
        }
        Ok(())
    }

    /// Read the optimizer/rate/decay settings from the trailing options dict.
    fn train_cfg(&self, opts: Option<&Value>, span: Span) -> Result<crate::nn::TrainCfg, Diagnostic> {
        let name = self.opt_text(opts, "optimizer", "sgd", span)?;
        let opt = crate::nn::Opt::from_name(&name).ok_or_else(|| {
            Diagnostic::new(span, format!("unknown optimizer \"{}\"", name))
                .with_hint("choose sgd, momentum, rmsprop, or adam")
        })?;
        let rate = self.opt_number(opts, "rate", default_rate(opt), span)?;
        let decay = self.opt_number(opts, "decay", 0.0, span)?;
        Ok(crate::nn::TrainCfg { opt, rate, decay })
    }

    fn opt_number(&self, opts: Option<&Value>, key: &str, default: f64, span: Span) -> Result<f64, Diagnostic> {
        match dict_get(opts, key) {
            Some(v) => self.as_number(&v, span),
            None => Ok(default),
        }
    }

    fn opt_text(&self, opts: Option<&Value>, key: &str, default: &str, span: Span) -> Result<String, Diagnostic> {
        match dict_get(opts, key) {
            Some(v) => self.as_text(&v, span),
            None => Ok(default.to_string()),
        }
    }

    /// A list of numbers → `Vec<f64>`.
    fn as_number_row(&self, v: &Value, span: Span) -> Result<Vec<f64>, Diagnostic> {
        match v {
            Value::List(l) => l.borrow().iter().map(|item| self.as_number(item, span)).collect(),
            other => Err(Diagnostic::new(
                span,
                format!("expected a list of numbers, got a {}", other.type_name()),
            )),
        }
    }

    /// A list of lists of numbers → rows of `f64`.
    fn as_number_rows(&self, v: &Value, span: Span) -> Result<Vec<Vec<f64>>, Diagnostic> {
        match v {
            Value::List(rows) => rows.borrow().iter().map(|r| self.as_number_row(r, span)).collect(),
            other => Err(Diagnostic::new(
                span,
                format!("expected a list of examples (a list of lists of numbers), got a {}", other.type_name()),
            )),
        }
    }

    /// Interpret the `hidden:` argument (a number or a list of numbers) as the
    /// hidden layer sizes.
    fn hidden_sizes(&self, hidden: Option<&Value>, span: Span) -> Result<Vec<usize>, Diagnostic> {
        match hidden {
            None | Some(Value::Nothing) => Ok(Vec::new()),
            Some(Value::Number(n)) => Ok(vec![positive_size(*n, span, "a hidden layer")?]),
            Some(Value::List(l)) => l
                .borrow()
                .iter()
                .map(|item| {
                    let n = self.as_number(item, span)?;
                    positive_size(n, span, "a hidden layer")
                })
                .collect(),
            Some(other) => Err(Diagnostic::new(
                span,
                format!("hidden should be a number or a list of numbers, got a {}", other.type_name()),
            )),
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
            Builtin::Input => {
                if args.len() > 1 {
                    return Err(Diagnostic::new(span, "input() takes an optional prompt (0 or 1 argument)"));
                }
                if let Some(prompt) = args.first() {
                    use std::io::Write;
                    print!("{}", prompt.display());
                    let _ = std::io::stdout().flush();
                }
                let mut line = String::new();
                match std::io::stdin().read_line(&mut line) {
                    Ok(0) => Ok(Value::text("")), // end of input
                    Ok(_) => {
                        while line.ends_with('\n') || line.ends_with('\r') {
                            line.pop();
                        }
                        // Strip a byte-order mark some Windows pipes prepend.
                        let line = line.strip_prefix('\u{feff}').unwrap_or(&line);
                        Ok(Value::text(line))
                    }
                    Err(e) => Err(Diagnostic::new(span, format!("couldn't read input: {}", e))),
                }
            }
            Builtin::Exit => {
                if args.len() > 1 {
                    return Err(Diagnostic::new(span, "exit() takes an optional status code (0 or 1 argument)"));
                }
                let code = match args.first() {
                    Some(v) => self.as_number(v, span)?.clamp(0.0, 255.0) as i32,
                    None => 0,
                };
                Err(Diagnostic::exit(code))
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
            Builtin::Round => {
                if args.is_empty() || args.len() > 2 {
                    return Err(Diagnostic::new(span, "round takes a number and an optional number of decimal places")
                        .with_hint("round(3.14159) → 3, or round(3.14159, 2) → 3.14"));
                }
                let n = self.as_number(&args[0], span)?;
                let places = match args.get(1) {
                    Some(v) => self.as_number(v, span)?.max(0.0) as i32,
                    None => 0,
                };
                let factor = 10f64.powi(places);
                Ok(Value::Number((n * factor).round() / factor))
            }
            Builtin::Abs | Builtin::Sqrt | Builtin::Floor | Builtin::Ceil => {
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
            Builtin::ReadCsv => {
                self.expect_arity("read_csv", &args, 1, span)?;
                let path = self.as_text(&args[0], span)?;
                let rows = self.read_csv_rows(&path, span)?;
                let list: Vec<Value> = rows
                    .into_iter()
                    .map(|row| self.new_list(row.into_iter().map(Value::Number).collect()))
                    .collect();
                Ok(self.new_list(list))
            }
            Builtin::LoadDataset => {
                if args.is_empty() || args.len() > 2 {
                    return Err(Diagnostic::new(span, "load_dataset takes a file path and optional settings")
                        .with_hint("e.g. load_dataset(\"data.csv\", outputs: 1)"));
                }
                let path = self.as_text(&args[0], span)?;
                let outputs = self.opt_number(args.get(1), "outputs", 1.0, span)?.max(1.0) as usize;
                let rows = self.read_csv_rows(&path, span)?;
                let mut examples: Vec<Value> = Vec::with_capacity(rows.len());
                let mut answers: Vec<Value> = Vec::with_capacity(rows.len());
                for (i, row) in rows.iter().enumerate() {
                    if row.len() <= outputs {
                        return Err(Diagnostic::new(
                            span,
                            format!("row {} has {} value(s), too few for {} output(s) plus at least one input", i + 1, row.len(), outputs),
                        ));
                    }
                    let split = row.len() - outputs;
                    let inputs: Vec<Value> = row[..split].iter().map(|&n| Value::Number(n)).collect();
                    let outs: Vec<Value> = row[split..].iter().map(|&n| Value::Number(n)).collect();
                    examples.push(self.new_list(inputs));
                    answers.push(self.new_list(outs));
                }
                let ex = self.new_list(examples);
                let an = self.new_list(answers);
                Ok(self.new_list(vec![ex, an]))
            }
            Builtin::Save => {
                self.expect_arity("save", &args, 2, span)?;
                let path = self.as_text(&args[1], span)?;
                let json = self.value_to_saved(&args[0], span)?;
                let text = serde_json::to_string_pretty(&json)
                    .map_err(|e| Diagnostic::new(span, format!("couldn't save: {}", e)))?;
                // Write to a temp file then rename, so a crash mid-save can't
                // leave a half-written (corrupt) save behind.
                let tmp = format!("{}.tmp", path);
                std::fs::write(&tmp, text.as_bytes())
                    .and_then(|_| std::fs::rename(&tmp, &path))
                    .map(|_| Value::Nothing)
                    .map_err(|e| Diagnostic::new(span, format!("couldn't save \"{}\": {}", path, e)))
            }
            Builtin::Load => {
                self.expect_arity("load", &args, 1, span)?;
                let path = self.as_text(&args[0], span)?;
                match std::fs::read_to_string(&path) {
                    Ok(text) => {
                        let j: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
                            Diagnostic::new(span, format!("the save file \"{}\" is damaged: {}", path, e))
                        })?;
                        Ok(self.saved_to_value(&j))
                    }
                    // A missing save is normal (first run) — hand back nothing.
                    Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Value::Nothing),
                    Err(e) => Err(Diagnostic::new(span, format!("couldn't read \"{}\": {}", path, e))),
                }
            }
            Builtin::HasSave => {
                self.expect_arity("has_save", &args, 1, span)?;
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
            Builtin::DrawTextScreen => {
                self.expect_arity("draw_text_screen", &args, 5, span)?;
                let text = args[0].display();
                let x = self.as_number(&args[1], span)? as f32;
                let y = self.as_number(&args[2], span)? as f32;
                let size = self.as_number(&args[3], span)? as i32;
                let color = self.as_color(&args[4], span)?;
                self.push_draw(DrawCmd::ScreenText { text, x, y, size, color, font: None }, span)
            }
            Builtin::DrawRectangleScreen => {
                self.expect_arity("draw_rectangle_screen", &args, 5, span)?;
                let x = self.as_number(&args[0], span)? as f32;
                let y = self.as_number(&args[1], span)? as f32;
                let w = self.as_number(&args[2], span)? as f32;
                let h = self.as_number(&args[3], span)? as f32;
                let color = self.as_color(&args[4], span)?;
                self.push_draw(DrawCmd::ScreenRect { x, y, w, h, color }, span)
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
            Builtin::LoadSpriteSheet => {
                // `load_sprite_sheet(path, cell_width: 32, cell_height: 32)`
                if args.is_empty() || args.len() > 2 {
                    return Err(Diagnostic::new(
                        span,
                        "load_sprite_sheet needs a path and cell_width / cell_height",
                    )
                    .with_hint(
                        "e.g. load_sprite_sheet(\"hero.png\", cell_width: 32, cell_height: 32)",
                    ));
                }
                let path = self.as_text(&args[0], span)?;
                if !std::path::Path::new(&path).exists() {
                    return Err(Diagnostic::new(span, format!("couldn't find sprite file \"{}\"", path)));
                }
                let opts = args.get(1);
                let cell_w = self.opt_number(opts, "cell_width", 0.0, span)? as i32;
                let cell_h = self.opt_number(opts, "cell_height", 0.0, span)? as i32;
                if cell_w <= 0 || cell_h <= 0 {
                    return Err(Diagnostic::new(
                        span,
                        "load_sprite_sheet needs positive cell_width and cell_height",
                    ));
                }
                let g = self.gfx_or_err(span)?;
                let id = g.borrow_mut().queue_sprite_sheet(path, cell_w, cell_h);
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
            Builtin::DrawFrame | Builtin::DrawFrameScaled => {
                // draw_frame(sheet, frame, x, y [, scale] [, flip_x: true])
                // Trailing keyword options become a dictionary argument.
                let (scale, opts_idx) = if b == Builtin::DrawFrameScaled {
                    if args.len() < 5 {
                        return Err(Diagnostic::new(
                            span,
                            "draw_frame_scaled needs a sheet, frame, x, y, and scale",
                        ));
                    }
                    (self.as_number(&args[4], span)? as f32, 5)
                } else if args.len() >= 4 {
                    (1.0_f32, 4)
                } else {
                    return Err(Diagnostic::new(
                        span,
                        "draw_frame needs a sheet, frame, x, and y",
                    )
                    .with_hint("e.g. draw_frame(sheet, frame, x, y) or draw_frame(..., flip_x: true)"));
                };
                let id = self.as_index(&args[0], span)?;
                let frame = self.as_number(&args[1], span)? as i32;
                let x = self.as_number(&args[2], span)? as f32;
                let y = self.as_number(&args[3], span)? as f32;
                let flip_x = self.opt_bool(args.get(opts_idx), "flip_x", false, span)?;
                let g = self.gfx_or_err(span)?;
                let (cell_w, cell_h) = match g.borrow().sheet_meta.get(&id).copied() {
                    Some(meta) => meta,
                    None => {
                        return Err(Diagnostic::new(
                            span,
                            "draw_frame needs a sprite sheet from load_sprite_sheet",
                        )
                        .with_hint("use load_sprite_sheet(path, cell_width: …, cell_height: …)"));
                    }
                };
                self.push_draw(
                    DrawCmd::SpriteFrame {
                        id,
                        frame,
                        cell_w,
                        cell_h,
                        x,
                        y,
                        scale,
                        flip_x,
                    },
                    span,
                )
            }
            Builtin::SpriteWidth | Builtin::SpriteHeight => {
                self.expect_arity(b.name(), &args, 1, span)?;
                let id = self.as_index(&args[0], span)?;
                let g = self.gfx_or_err(span)?;
                let size = g.borrow().sprite_sizes.get(&id).copied();
                let n = match size {
                    Some((w, h)) => {
                        if b == Builtin::SpriteWidth {
                            w
                        } else {
                            h
                        }
                    }
                    None => 0, // not loaded yet (e.g. read during init, before the window opens)
                };
                Ok(Value::Number(n as f64))
            }
            Builtin::FrameCount => {
                self.expect_arity("frame_count", &args, 1, span)?;
                let id = self.as_index(&args[0], span)?;
                let g = self.gfx_or_err(span)?;
                let g = g.borrow();
                let Some((cw, ch)) = g.sheet_meta.get(&id).copied() else {
                    return Err(Diagnostic::new(
                        span,
                        "frame_count needs a sprite sheet from load_sprite_sheet",
                    ));
                };
                let (tw, th) = g.sprite_sizes.get(&id).copied().unwrap_or((0, 0));
                Ok(Value::Number(crate::gfx::sheet_frame_count(tw, th, cw, ch) as f64))
            }
            Builtin::SetCamera => {
                self.expect_arity("set_camera", &args, 2, span)?;
                let x = self.as_number(&args[0], span)? as f32;
                let y = self.as_number(&args[1], span)? as f32;
                let g = self.gfx_or_err(span)?;
                let mut g = g.borrow_mut();
                g.camera_x = x;
                g.camera_y = y;
                g.apply_camera_bounds();
                Ok(Value::Nothing)
            }
            Builtin::CenterCamera => {
                self.expect_arity("center_camera", &args, 2, span)?;
                let x = self.as_number(&args[0], span)? as f32;
                let y = self.as_number(&args[1], span)? as f32;
                let g = self.gfx_or_err(span)?;
                let mut g = g.borrow_mut();
                g.camera_x = x - g.screen_w as f32 / 2.0;
                g.camera_y = y - g.screen_h as f32 / 2.0;
                g.apply_camera_bounds();
                Ok(Value::Nothing)
            }
            Builtin::CameraBounds => {
                self.expect_arity("camera_bounds", &args, 4, span)?;
                let min_x = self.as_number(&args[0], span)? as f32;
                let min_y = self.as_number(&args[1], span)? as f32;
                let max_x = self.as_number(&args[2], span)? as f32;
                let max_y = self.as_number(&args[3], span)? as f32;
                if max_x < min_x || max_y < min_y {
                    return Err(Diagnostic::new(
                        span,
                        "camera_bounds needs max_x >= min_x and max_y >= min_y",
                    ));
                }
                let g = self.gfx_or_err(span)?;
                let mut g = g.borrow_mut();
                g.camera_bounds = Some((min_x, min_y, max_x, max_y));
                g.apply_camera_bounds();
                Ok(Value::Nothing)
            }
            Builtin::CameraX | Builtin::CameraY => {
                self.expect_arity(b.name(), &args, 0, span)?;
                let g = self.gfx_or_err(span)?;
                let g = g.borrow();
                Ok(Value::Number(if b == Builtin::CameraX {
                    g.camera_x as f64
                } else {
                    g.camera_y as f64
                }))
            }
            Builtin::Burst => {
                // burst(x, y, color, count) or with speed: / life: options.
                if args.len() < 4 || args.len() > 5 {
                    return Err(Diagnostic::new(
                        span,
                        "burst needs x, y, color, and count",
                    )
                    .with_hint("e.g. burst(x, y, orange, 16) or burst(..., speed: 200, life: 0.5)"));
                }
                let x = self.as_number(&args[0], span)? as f32;
                let y = self.as_number(&args[1], span)? as f32;
                let color = self.as_color(&args[2], span)?;
                let count = self.as_number(&args[3], span)? as i32;
                let opts = args.get(4);
                let speed = self.opt_number(opts, "speed", 180.0, span)? as f32;
                let life = self.opt_number(opts, "life", 0.45, span)? as f32;
                let g = self.gfx_or_err(span)?;
                crate::gfx::spawn_burst(
                    &mut g.borrow_mut().particles,
                    x,
                    y,
                    color,
                    count,
                    speed.max(0.0),
                    life.max(0.0),
                );
                Ok(Value::Nothing)
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
                // `play_sound(id)` or `play_sound(id, loop: true)`.
                if args.is_empty() || args.len() > 2 {
                    return Err(Diagnostic::new(span, "play_sound needs a sound id")
                        .with_hint("e.g. play_sound(beep) or play_sound(beep, loop: true)"));
                }
                let id = self.as_index(&args[0], span)?;
                let looping = if args.len() == 2 {
                    self.opt_bool(args.get(1), "loop", false, span)?
                } else {
                    false
                };
                let g = self.gfx_or_err(span)?;
                g.borrow_mut().sound_cmds.push(SoundCmd::Play { id, looping });
                Ok(Value::Nothing)
            }
            Builtin::StopSound => {
                self.expect_arity("stop_sound", &args, 1, span)?;
                let id = self.as_index(&args[0], span)?;
                let g = self.gfx_or_err(span)?;
                g.borrow_mut().sound_cmds.push(SoundCmd::Stop(id));
                Ok(Value::Nothing)
            }
            Builtin::SetSoundVolume | Builtin::SetSoundPitch | Builtin::SetSoundPan => {
                self.expect_arity(b.name(), &args, 2, span)?;
                let id = self.as_index(&args[0], span)?;
                let n = self.as_number(&args[1], span)? as f32;
                let g = self.gfx_or_err(span)?;
                let cmd = match b {
                    Builtin::SetSoundVolume => SoundCmd::SetVolume { id, volume: n.clamp(0.0, 1.0) },
                    Builtin::SetSoundPitch => SoundCmd::SetPitch { id, pitch: n.max(0.0) },
                    Builtin::SetSoundPan => SoundCmd::SetPan { id, pan: n.clamp(0.0, 1.0) },
                    _ => unreachable!(),
                };
                g.borrow_mut().sound_cmds.push(cmd);
                Ok(Value::Nothing)
            }
            Builtin::LoadMusic => {
                self.expect_arity("load_music", &args, 1, span)?;
                let path = self.as_text(&args[0], span)?;
                if !std::path::Path::new(&path).exists() {
                    return Err(Diagnostic::new(span, format!("couldn't find music file \"{}\"", path)));
                }
                let g = self.gfx_or_err(span)?;
                let id = g.borrow_mut().queue_music(path);
                Ok(Value::Number(id as f64))
            }
            Builtin::PlayMusic => {
                self.expect_arity("play_music", &args, 1, span)?;
                let id = self.as_index(&args[0], span)?;
                let g = self.gfx_or_err(span)?;
                g.borrow_mut().music_cmds.push(MusicCmd::Play(id));
                Ok(Value::Nothing)
            }
            Builtin::StopMusic => {
                self.expect_arity("stop_music", &args, 1, span)?;
                let id = self.as_index(&args[0], span)?;
                let g = self.gfx_or_err(span)?;
                g.borrow_mut().music_cmds.push(MusicCmd::Stop(id));
                Ok(Value::Nothing)
            }
            Builtin::SetMusicVolume | Builtin::SetMusicPitch | Builtin::SetMusicPan => {
                self.expect_arity(b.name(), &args, 2, span)?;
                let id = self.as_index(&args[0], span)?;
                let n = self.as_number(&args[1], span)? as f32;
                let g = self.gfx_or_err(span)?;
                let cmd = match b {
                    Builtin::SetMusicVolume => MusicCmd::SetVolume { id, volume: n.clamp(0.0, 1.0) },
                    Builtin::SetMusicPitch => MusicCmd::SetPitch { id, pitch: n.max(0.0) },
                    Builtin::SetMusicPan => MusicCmd::SetPan { id, pan: n.clamp(0.0, 1.0) },
                    _ => unreachable!(),
                };
                g.borrow_mut().music_cmds.push(cmd);
                Ok(Value::Nothing)
            }
            Builtin::FadeMusic => {
                self.expect_arity("fade_music", &args, 3, span)?;
                let id = self.as_index(&args[0], span)?;
                let target = (self.as_number(&args[1], span)? as f32).clamp(0.0, 1.0);
                let seconds = (self.as_number(&args[2], span)? as f32).max(0.0);
                let g = self.gfx_or_err(span)?;
                g.borrow_mut().music_cmds.push(MusicCmd::Fade { id, target, seconds });
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
            Builtin::NeuralNetwork => {
                // Positional (inputs, hidden, outputs) or keyword options.
                let (inputs, hidden, outputs) = match args.len() {
                    1 => {
                        let o = args.first();
                        (
                            self.opt_number(o, "inputs", 0.0, span)?,
                            dict_get(o, "hidden"),
                            self.opt_number(o, "outputs", 0.0, span)?,
                        )
                    }
                    3 => (
                        self.as_number(&args[0], span)?,
                        Some(args[1].clone()),
                        self.as_number(&args[2], span)?,
                    ),
                    _ => {
                        return Err(Diagnostic::new(span, "neural_network needs inputs, hidden, and outputs")
                            .with_hint("e.g. neural_network(inputs: 2, hidden: [6, 6], outputs: 1)"));
                    }
                };
                let inputs = positive_size(inputs, span, "the number of inputs")?;
                let outputs = positive_size(outputs, span, "the number of outputs")?;
                let mut sizes = vec![inputs];
                sizes.extend(self.hidden_sizes(hidden.as_ref(), span)?);
                sizes.push(outputs);

                self.next_rand(); // advance the RNG so each network differs
                let mut net = crate::nn::Net::new(sizes, self.rng_state.get());
                if let Some(dv) = dict_get(args.first(), "device") {
                    let name = self.as_text(&dv, span)?;
                    let kind = crate::gpu::DeviceKind::parse(&name).ok_or_else(|| {
                        Diagnostic::new(span, format!("unknown device \"{}\"", name))
                            .with_hint("choose cpu, gpu, cuda, rocm, mps, vulkan, or dx12")
                    })?;
                    net.set_device(Some(kind));
                }
                Ok(Value::Network(Rc::new(RefCell::new(net))))
            }
            Builtin::LoadNetwork => {
                self.expect_arity("load_network", &args, 1, span)?;
                let path = self.as_text(&args[0], span)?;
                match crate::nn::Net::load(&path) {
                    Some(net) => Ok(Value::Network(Rc::new(RefCell::new(net)))),
                    None => Err(Diagnostic::new(span, format!("couldn't load a network from \"{}\"", path))
                        .with_hint("is it a file saved with a network's .save(...)?")),
                }
            }
            Builtin::Population => {
                let o = args.first();
                let count = positive_size(self.opt_number(o, "count", 0.0, span)?, span, "the population size")?;
                let inputs = positive_size(self.opt_number(o, "inputs", 0.0, span)?, span, "the number of inputs")?;
                let outputs = positive_size(self.opt_number(o, "outputs", 0.0, span)?, span, "the number of outputs")?;
                let mut sizes = vec![inputs];
                sizes.extend(self.hidden_sizes(dict_get(o, "hidden").as_ref(), span)?);
                sizes.push(outputs);
                let mut brains = Vec::with_capacity(count);
                for _ in 0..count {
                    self.next_rand(); // a different starting brain each time
                    let net = crate::nn::Net::new(sizes.clone(), self.rng_state.get());
                    brains.push(Value::Network(Rc::new(RefCell::new(net))));
                }
                Ok(self.new_list(brains))
            }
            Builtin::Evolve => {
                if args.len() < 2 || args.len() > 3 {
                    return Err(Diagnostic::new(span, "evolve takes brains, scores, and optional settings")
                        .with_hint("e.g. evolve(brains, scores, mutation: 0.1, keep: 2)"));
                }
                let brains = self.as_network_list(&args[0], span)?;
                let scores = self.as_number_row(&args[1], span)?;
                if brains.len() != scores.len() {
                    return Err(Diagnostic::new(
                        span,
                        format!("got {} brains but {} scores — there must be one score per brain", brains.len(), scores.len()),
                    ));
                }
                let opts = args.get(2);
                let mutation = self.opt_number(opts, "mutation", 0.1, span)?;
                let keep = self.opt_number(opts, "keep", 1.0, span)?.max(0.0) as usize;
                let nets: Vec<crate::nn::Net> = brains.iter().map(|b| b.borrow().clone()).collect();
                self.next_rand();
                let seed = self.rng_state.get();
                let next = crate::nn::evolve(&nets, &scores, mutation, keep, seed);
                let list: Vec<Value> = next
                    .into_iter()
                    .map(|n| Value::Network(Rc::new(RefCell::new(n))))
                    .collect();
                Ok(self.new_list(list))
            }
            Builtin::BestOf => {
                self.expect_arity("best_of", &args, 2, span)?;
                let brains = self.as_network_list(&args[0], span)?;
                let scores = self.as_number_row(&args[1], span)?;
                if brains.is_empty() || brains.len() != scores.len() {
                    return Err(Diagnostic::new(span, "best_of needs a non-empty population and one score per brain"));
                }
                let mut best = 0;
                for i in 1..scores.len() {
                    if scores[i] > scores[best] {
                        best = i;
                    }
                }
                Ok(Value::Network(brains[best].clone()))
            }
            Builtin::PhysicsWorld => {
                let o = args.first();
                let gravity = self.opt_number(o, "gravity", 1800.0, span)?;
                Ok(Value::PhysicsWorld(Rc::new(RefCell::new(crate::gamekit::World::new(gravity)))))
            }
            Builtin::Body => {
                let o = args.first();
                let x = self.opt_number(o, "x", 0.0, span)?;
                let y = self.opt_number(o, "y", 0.0, span)?;
                let width = self.opt_number(o, "width", 32.0, span)?.max(0.0);
                let height = self.opt_number(o, "height", 32.0, span)?.max(0.0);
                let mut body = crate::gamekit::Body::new(x, y, width, height);
                if let Some(v) = dict_get(o, "solid") {
                    body.solid = self.as_bool_val(&v, span)?;
                }
                if let Some(v) = dict_get(o, "static") {
                    body.is_static = self.as_bool_val(&v, span)?;
                }
                if let Some(v) = dict_get(o, "vx") {
                    body.vx = self.as_number(&v, span)?;
                }
                if let Some(v) = dict_get(o, "vy") {
                    body.vy = self.as_number(&v, span)?;
                }
                Ok(Value::Body(Rc::new(RefCell::new(body))))
            }
            Builtin::Hitbox => {
                let o = args.first();
                let owner = match dict_get(o, "owner") {
                    Some(Value::Body(b)) => Some(b),
                    Some(other) => {
                        return Err(Diagnostic::new(
                            span,
                            format!("hitbox owner needs a body, got a {}", other.type_name()),
                        ));
                    }
                    None => None,
                };
                let ox = self.opt_number(o, "offset_x", 0.0, span)?;
                let oy = self.opt_number(o, "offset_y", 0.0, span)?;
                let width = self.opt_number(o, "width", 32.0, span)?.max(0.0);
                let height = self.opt_number(o, "height", 32.0, span)?.max(0.0);
                let kind = match dict_get(o, "kind") {
                    Some(v) => self.as_text(&v, span)?,
                    None => "hurt".into(),
                };
                let active = match dict_get(o, "active") {
                    Some(v) => self.as_bool_val(&v, span)?,
                    None => true,
                };
                Ok(Value::Hitbox(Rc::new(RefCell::new(crate::gamekit::Hitbox::new(
                    owner, ox, oy, width, height, kind, active,
                )))))
            }
            Builtin::Overlaps => {
                self.expect_arity("overlaps", &args, 2, span)?;
                match (&args[0], &args[1]) {
                    (Value::Hitbox(a), Value::Hitbox(b)) => {
                        Ok(Value::Bool(crate::gamekit::hitboxes_overlap(&a.borrow(), &b.borrow())))
                    }
                    _ => Err(Diagnostic::new(span, "overlaps needs two hitboxes")),
                }
            }
            Builtin::Pressed => {
                self.expect_arity("pressed", &args, 1, span)?;
                let key = self.as_text(&args[0], span)?.to_lowercase();
                let aliases = key_aliases(&key);
                let bridge = self.gfx.as_ref().ok_or_else(|| {
                    Diagnostic::new(span, "pressed() only works inside a game window")
                })?;
                let g = bridge.borrow();
                let down = g.keys_pressed.contains(&key)
                    || aliases.iter().any(|k| g.keys_pressed.contains(*k));
                Ok(Value::Bool(down))
            }
            Builtin::DrawBody => {
                self.expect_arity("draw_body", &args, 2, span)?;
                let body = match &args[0] {
                    Value::Body(b) => b.borrow(),
                    other => {
                        return Err(Diagnostic::new(
                            span,
                            format!("draw_body needs a body, got a {}", other.type_name()),
                        ));
                    }
                };
                let color = self.as_color(&args[1], span)?;
                let gfx = self.gfx.as_ref().ok_or_else(|| {
                    Diagnostic::new(span, "draw_body only works inside a game window")
                })?;
                gfx.borrow_mut().draw.push(crate::gfx::DrawCmd::Rect {
                    x: body.x as f32,
                    y: body.y as f32,
                    w: body.width as f32,
                    h: body.height as f32,
                    color,
                });
                Ok(Value::Nothing)
            }
            Builtin::DrawHitbox => {
                self.expect_arity("draw_hitbox", &args, 2, span)?;
                let hb = match &args[0] {
                    Value::Hitbox(h) => h.borrow(),
                    other => {
                        return Err(Diagnostic::new(
                            span,
                            format!("draw_hitbox needs a hitbox, got a {}", other.type_name()),
                        ));
                    }
                };
                let color = self.as_color(&args[1], span)?;
                let (x, y, w, h) = hb.world_rect();
                let gfx = self.gfx.as_ref().ok_or_else(|| {
                    Diagnostic::new(span, "draw_hitbox only works inside a game window")
                })?;
                push_outline(&mut gfx.borrow_mut().draw, x, y, w, h, color);
                Ok(Value::Nothing)
            }
            Builtin::DrawHitboxes => {
                self.expect_arity("draw_hitboxes", &args, 1, span)?;
                let world = match &args[0] {
                    Value::PhysicsWorld(w) => w.borrow(),
                    other => {
                        return Err(Diagnostic::new(
                            span,
                            format!("draw_hitboxes needs a physics world, got a {}", other.type_name()),
                        ));
                    }
                };
                let gfx = self.gfx.as_ref().ok_or_else(|| {
                    Diagnostic::new(span, "draw_hitboxes only works inside a game window")
                })?;
                let mut g = gfx.borrow_mut();
                for hb in world.hitboxes() {
                    let h = hb.borrow();
                    if !h.active {
                        continue;
                    }
                    let (x, y, w, ht) = h.world_rect();
                    let color = match h.kind.as_str() {
                        "attack" => crate::gfx::Color(220, 60, 60, 200),
                        "pickup" => crate::gfx::Color(60, 200, 80, 200),
                        _ => crate::gfx::Color(60, 140, 220, 200),
                    };
                    push_outline(&mut g.draw, x, y, w, ht, color);
                }
                Ok(Value::Nothing)
            }
            Builtin::Tilemap => {
                let o = args.first();
                let cell_size = self.opt_number(o, "cell_size", 32.0, span)?;
                if cell_size < 1.0 {
                    return Err(Diagnostic::new(span, "cell_size must be at least 1"));
                }
                let rows = self.read_tilemap_rows(o, span)?;
                let mut map = crate::gamekit::Tilemap::new(cell_size, rows);
                let solids = self.read_solid_tiles(o, span)?;
                if !solids.is_empty() {
                    map.set_solid_tiles(solids);
                }
                Ok(Value::Tilemap(Rc::new(RefCell::new(map))))
            }
            Builtin::TileAt => {
                self.expect_arity("tile_at", &args, 3, span)?;
                let map = match &args[0] {
                    Value::Tilemap(m) => m.borrow(),
                    other => {
                        return Err(Diagnostic::new(
                            span,
                            format!("tile_at needs a tilemap, got a {}", other.type_name()),
                        ));
                    }
                };
                let x = self.as_number(&args[1], span)?.floor() as i64;
                let y = self.as_number(&args[2], span)?.floor() as i64;
                Ok(match map.tile_at(x, y) {
                    Some(ch) => Value::text(ch.to_string()),
                    None => Value::Nothing,
                })
            }
            Builtin::DrawTilemap => {
                if args.is_empty() || args.len() > 2 {
                    return Err(Diagnostic::new(
                        span,
                        "draw_tilemap takes a tilemap and tile_colors: / tile_images: dictionary",
                    ));
                }
                let map = match &args[0] {
                    Value::Tilemap(m) => m.borrow(),
                    other => {
                        return Err(Diagnostic::new(
                            span,
                            format!("draw_tilemap needs a tilemap, got a {}", other.type_name()),
                        ));
                    }
                };
                // Per-character styling. A value that's a Text is an image path
                // (drawn as a sprite); anything else is a color (drawn as a rect).
                // Accept `tile_colors:` / `tile_images:` named args (merged), or a
                // bare dictionary whose values may be either.
                let mut style = PtMap::new();
                match args.get(1) {
                    Some(Value::Dictionary(m)) => {
                        let mb = m.borrow();
                        let named: Vec<Value> = [
                            mb.get(&MapKey::Text("tile_colors".into())),
                            mb.get(&MapKey::Text("tile_images".into())),
                        ]
                        .into_iter()
                        .flatten()
                        .collect();
                        if named.is_empty() {
                            for (k, v) in &mb.entries {
                                style.set(k.clone(), v.clone());
                            }
                        } else {
                            for src in named {
                                match src {
                                    Value::Dictionary(d) => {
                                        for (k, v) in &d.borrow().entries {
                                            style.set(k.clone(), v.clone());
                                        }
                                    }
                                    other => {
                                        return Err(Diagnostic::new(
                                            span,
                                            format!(
                                                "tile_colors / tile_images need a dictionary, got a {}",
                                                other.type_name()
                                            ),
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    Some(other) => {
                        return Err(Diagnostic::new(
                            span,
                            format!(
                                "draw_tilemap styling needs a dictionary, got a {}",
                                other.type_name()
                            ),
                        ));
                    }
                    None => {}
                }
                let gfx = self.gfx.as_ref().ok_or_else(|| {
                    Diagnostic::new(span, "draw_tilemap only works inside a game window")
                })?;
                let mut g = gfx.borrow_mut();
                let cell = map.cell_size as f32;
                for (ty, row) in map.rows.iter().enumerate() {
                    for (tx, ch) in row.chars().enumerate() {
                        if ch == ' ' {
                            continue;
                        }
                        let Some(v) = style.get(&MapKey::Text(ch.to_string())) else {
                            continue;
                        };
                        let x = tx as f32 * cell;
                        let y = ty as f32 * cell;
                        match v {
                            Value::Text(path) => {
                                let id = g.sprite_for_path(path.as_str());
                                g.draw.push(crate::gfx::DrawCmd::SpriteRect {
                                    id,
                                    x,
                                    y,
                                    w: cell,
                                    h: cell,
                                });
                            }
                            other => {
                                let color = self.as_color(&other, span)?;
                                g.draw.push(crate::gfx::DrawCmd::Rect {
                                    x,
                                    y,
                                    w: cell,
                                    h: cell,
                                    color,
                                });
                            }
                        }
                    }
                }
                Ok(Value::Nothing)
            }
            Builtin::WebGetJson => {
                self.expect_arity("get_json", &args, 1, span)?;
                let url = self.as_text(&args[0], span)?;
                crate::web::http_get_json(&url, span)
            }
            Builtin::WebPostJson => {
                self.expect_arity("post_json", &args, 2, span)?;
                let url = self.as_text(&args[0], span)?;
                let body = crate::web::http_post_json(&url, &args[1], span)?;
                Ok(Value::text(body))
            }
            Builtin::ParseJson => {
                self.expect_arity("parse_json", &args, 1, span)?;
                let text = self.as_text(&args[0], span)?;
                crate::web::parse_json(&text, span)
            }
            Builtin::ToJson => {
                self.expect_arity("to_json", &args, 1, span)?;
                let text = crate::web::to_json(&args[0]).map_err(|e| {
                    Diagnostic::new(span, format!("couldn't turn that value into JSON: {}", e))
                })?;
                Ok(Value::text(text))
            }
        }
    }

    /// Read a `Value::List` of networks (a population) into their handles.
    fn as_network_list(&self, v: &Value, span: Span) -> Result<Vec<Rc<RefCell<crate::nn::Net>>>, Diagnostic> {
        match v {
            Value::List(l) => l
                .borrow()
                .iter()
                .map(|item| match item {
                    Value::Network(n) => Ok(n.clone()),
                    other => Err(Diagnostic::new(
                        span,
                        format!("a population should hold networks, but found a {}", other.type_name()),
                    )),
                })
                .collect(),
            other => Err(Diagnostic::new(
                span,
                format!("expected a population (a list of networks), got a {}", other.type_name()),
            )),
        }
    }

    /// Parse a CSV/whitespace-separated numeric file into rows of numbers,
    /// skipping blank lines, `#` comments, and a single non-numeric header row.
    fn read_csv_rows(&self, path: &str, span: Span) -> Result<Vec<Vec<f64>>, Diagnostic> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| Diagnostic::new(span, format!("couldn't read file \"{}\": {}", path, e)))?;
        let mut rows: Vec<Vec<f64>> = Vec::new();
        for (i, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fields: Vec<&str> = line.split([',', ';', '\t', ' ']).filter(|s| !s.is_empty()).collect();
            let mut row = Vec::with_capacity(fields.len());
            let mut ok = true;
            for f in &fields {
                match f.parse::<f64>() {
                    Ok(n) => row.push(n),
                    Err(_) => {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                // Tolerate a header, but only as the first data-bearing line.
                if rows.is_empty() {
                    continue;
                }
                return Err(Diagnostic::new(
                    span,
                    format!("line {} of \"{}\" has a value that isn't a number", i + 1, path),
                ));
            }
            if !row.is_empty() {
                rows.push(row);
            }
        }
        if rows.is_empty() {
            return Err(Diagnostic::new(span, format!("found no data rows in \"{}\"", path))
                .with_hint("each line should be numbers separated by commas"));
        }
        Ok(rows)
    }

    fn gfx_or_err(&self, span: Span) -> Result<Rc<RefCell<GfxBridge>>, Diagnostic> {
        self.gfx.clone().ok_or_else(|| {
            Diagnostic::new(
                span,
                "this drawing/input/sound function only works inside a `game` (or `window`) block",
            )
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

    fn require_callable(&self, v: &Value, method: &str, span: Span) -> Result<(), Diagnostic> {
        if is_callable(v) {
            Ok(())
        } else {
            Err(Diagnostic::new(
                span,
                format!("{} needs a function, got a {}", method, v.type_name()),
            )
            .with_hint("pass a function by name, e.g. numbers.transformed_by(double)"))
        }
    }

    /// Serialize a value for `save(...)`: the JSON-shaped types plus class
    /// instances, which are tagged with `~type` so `load` can rebuild them.
    fn value_to_saved(&self, v: &Value, span: Span) -> Result<serde_json::Value, Diagnostic> {
        use serde_json::Value as J;
        Ok(match v {
            Value::Number(n) => {
                if n.fract() == 0.0 && n.is_finite() && n.abs() < 9.007e15 {
                    J::Number((*n as i64).into())
                } else if let Some(num) = serde_json::Number::from_f64(*n) {
                    J::Number(num)
                } else {
                    return Err(Diagnostic::new(span, "that number can't be saved (not a finite value)"));
                }
            }
            Value::Text(s) => J::String((**s).clone()),
            Value::Bool(b) => J::Bool(*b),
            Value::Nothing => J::Null,
            Value::List(items) => {
                let mut arr = Vec::new();
                for it in items.borrow().iter() {
                    arr.push(self.value_to_saved(it, span)?);
                }
                J::Array(arr)
            }
            Value::Dictionary(map) => {
                let mut obj = serde_json::Map::new();
                for (k, val) in map.borrow().entries.iter() {
                    let key = match k {
                        MapKey::Text(s) => s.clone(),
                        MapKey::Number(n) => format_number(*n),
                        MapKey::Bool(b) => if *b { "true".into() } else { "false".into() },
                    };
                    obj.insert(key, self.value_to_saved(val, span)?);
                }
                J::Object(obj)
            }
            Value::Class(inst) => {
                let inst = inst.borrow();
                let mut obj = serde_json::Map::new();
                obj.insert("~type".into(), J::String(inst.def.name.clone()));
                for f in &inst.def.fields {
                    let val = inst.fields.get(&f.name).cloned().unwrap_or(Value::Nothing);
                    obj.insert(f.name.clone(), self.value_to_saved(&val, span)?);
                }
                J::Object(obj)
            }
            other => {
                return Err(Diagnostic::new(span, format!("can't save a {}", other.type_name()))
                    .with_hint("save numbers, text, true/false, lists, dictionaries, or your own class values"));
            }
        })
    }

    /// Rebuild a value read by `load(...)` on the collected heap, reconstructing
    /// any tagged class the program still defines (otherwise it's a dictionary).
    fn saved_to_value(&mut self, j: &serde_json::Value) -> Value {
        use serde_json::Value as J;
        match j {
            J::Null => Value::Nothing,
            J::Bool(b) => Value::Bool(*b),
            J::Number(n) => Value::Number(n.as_f64().unwrap_or(0.0)),
            J::String(s) => Value::text(s.clone()),
            J::Array(items) => {
                let list: Vec<Value> = items.iter().map(|x| self.saved_to_value(x)).collect();
                self.new_list(list)
            }
            J::Object(map) => {
                if let Some(J::String(tname)) = map.get("~type") {
                    if let Some(def) = self.classes.get(tname).cloned() {
                        let mut fields = HashMap::new();
                        for f in &def.fields {
                            let val = map.get(&f.name).map(|x| self.saved_to_value(x)).unwrap_or(Value::Nothing);
                            fields.insert(f.name.clone(), val);
                        }
                        return self.new_instance(ClassInstance { def, fields });
                    }
                }
                let mut pt = PtMap::new();
                for (k, v) in map.iter() {
                    pt.set(MapKey::Text(k.clone()), self.saved_to_value(v));
                }
                self.new_dict(pt)
            }
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

/// Look up a key in the trailing options dictionary of a call, if present.
fn dict_get(opts: Option<&Value>, key: &str) -> Option<Value> {
    if let Some(Value::Dictionary(m)) = opts {
        m.borrow().get(&MapKey::Text(key.to_string()))
    } else {
        None
    }
}

/// A sensible default learning rate per optimizer (users can override with `rate:`).
fn default_rate(opt: crate::nn::Opt) -> f64 {
    use crate::nn::Opt::*;
    match opt {
        Sgd => 0.5,
        Momentum => 0.2,
        RmsProp => 0.01,
        Adam => 0.05,
    }
}

/// Interpret a Number as a positive layer size (whole number ≥ 1).
fn positive_size(n: f64, span: Span, what: &str) -> Result<usize, Diagnostic> {
    if n < 1.0 || n.fract() != 0.0 {
        Err(Diagnostic::new(span, format!("{} must be a whole number of at least 1, got {}", what, format_number(n))))
    } else {
        Ok(n as usize)
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
        (Value::Network(x), Value::Network(y)) => Rc::ptr_eq(x, y),
        (Value::Body(x), Value::Body(y)) => Rc::ptr_eq(x, y),
        (Value::Hitbox(x), Value::Hitbox(y)) => Rc::ptr_eq(x, y),
        (Value::PhysicsWorld(x), Value::PhysicsWorld(y)) => Rc::ptr_eq(x, y),
        (Value::Tilemap(x), Value::Tilemap(y)) => Rc::ptr_eq(x, y),
        (Value::WebModule, Value::WebModule) => true,
        _ => false,
    }
}

/// Friendly aliases for `pressed("jump")` etc. (on top of the raw key name).
fn key_aliases(name: &str) -> Vec<&'static str> {
    match name {
        "jump" => vec!["space", "up", "w"],
        "left" => vec!["left", "a"],
        "right" => vec!["right", "d"],
        "up" => vec!["up", "w"],
        "down" => vec!["down", "s"],
        "attack" | "strike" => vec!["z", "j", "space"],
        _ => Vec::new(),
    }
}

fn push_outline(cmds: &mut Vec<crate::gfx::DrawCmd>, x: f64, y: f64, w: f64, h: f64, color: crate::gfx::Color) {
    let t = 2.0f32;
    let x = x as f32;
    let y = y as f32;
    let w = w as f32;
    let h = h as f32;
    cmds.push(crate::gfx::DrawCmd::Rect { x, y, w, h: t, color });
    cmds.push(crate::gfx::DrawCmd::Rect { x, y: y + h - t, w, h: t, color });
    cmds.push(crate::gfx::DrawCmd::Rect { x, y, w: t, h, color });
    cmds.push(crate::gfx::DrawCmd::Rect { x: x + w - t, y, w: t, h, color });
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
        Stmt::Import { span, .. } => *span,
        Stmt::ImportFile { span, .. } => *span,
        Stmt::Game(g) => g.span,
        Stmt::Window(w) => w.span,
        Stmt::Expr(e) => e.span(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load_round_trips_values() {
        let mut interp = Interpreter::new();
        // dictionary { "n": 7, "flag": true, "tags": ["a", "b"], "nada": nothing }
        let mut m = PtMap::new();
        m.set(MapKey::Text("n".into()), Value::Number(7.0));
        m.set(MapKey::Text("flag".into()), Value::Bool(true));
        let tags = Value::List(Rc::new(RefCell::new(vec![Value::text("a"), Value::text("b")])));
        m.set(MapKey::Text("tags".into()), tags);
        m.set(MapKey::Text("nada".into()), Value::Nothing);
        let dict = Value::Dictionary(Rc::new(RefCell::new(m)));

        let span = Span::new(0, 0);
        let json = interp.value_to_saved(&dict, span).unwrap();
        let text = serde_json::to_string(&json).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        let back = interp.saved_to_value(&parsed);

        match back {
            Value::Dictionary(m) => {
                let m = m.borrow();
                assert!(matches!(m.get(&MapKey::Text("n".into())), Some(Value::Number(n)) if (n - 7.0).abs() < 1e-9));
                assert!(matches!(m.get(&MapKey::Text("flag".into())), Some(Value::Bool(true))));
                assert!(matches!(m.get(&MapKey::Text("nada".into())), Some(Value::Nothing)));
                match m.get(&MapKey::Text("tags".into())) {
                    Some(Value::List(l)) => {
                        let l = l.borrow();
                        assert_eq!(l.len(), 2);
                        assert_eq!(l[0].display(), "a");
                        assert_eq!(l[1].display(), "b");
                    }
                    other => panic!("tags round-tripped wrong: {:?}", other.map(|v| v.display())),
                }
            }
            other => panic!("expected a dictionary, got {}", other.type_name()),
        }
    }

    #[test]
    fn cannot_save_a_function() {
        let interp = Interpreter::new();
        let builtin = Value::Builtin(crate::value::Builtin::Print);
        assert!(interp.value_to_saved(&builtin, Span::new(0, 0)).is_err());
    }
}
