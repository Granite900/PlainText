//! The static type checker.
//!
//! It runs after parsing and before execution. It infers a type for every
//! expression, enforces the rules from the design (no implicit Number/Text
//! mixing, boolean-only conditions, real fields/methods, optional narrowing),
//! and implements the `Dynamic` gradual-typing escape hatch: a bare untyped
//! function parameter is `Dynamic`, and anything derived from a `Dynamic`
//! value is `Dynamic` too (so its use is checked at run time instead).
//!
//! Errors are collected into a list rather than stopping at the first, so a
//! single run can report several problems.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ast::*;
use crate::diagnostics::Diagnostic;
use crate::token::Span;

/// A PlainText type, as understood by the checker.
#[derive(Clone)]
pub enum Ty {
    Number,
    Text,
    Bool,
    /// The type of the `nothing` literal and of a function that returns no value.
    Nothing,
    List(Box<Ty>),
    Dictionary(Box<Ty>, Box<Ty>),
    Class(String),
    /// A neural network (from `import ai`).
    Network,
    /// `T?` — may hold a `T` or `nothing`.
    Optional(Box<Ty>),
    Function(Rc<FnSig>),
    /// The gradual escape hatch: compatible with everything, checked at runtime.
    Dynamic,
}

pub struct FnSig {
    /// The id used to look up the return type lazily (`name` or `Class::method`).
    pub id: Option<String>,
    pub params: Vec<Ty>,
    pub required: usize,
}

impl Ty {
    fn describe(&self) -> String {
        match self {
            Ty::Number => "Number".into(),
            Ty::Text => "Text".into(),
            Ty::Bool => "Boolean".into(),
            Ty::Nothing => "nothing".into(),
            Ty::List(t) => format!("{} list", t.describe()),
            Ty::Dictionary(k, v) => format!("dictionary of {} to {}", k.describe(), v.describe()),
            Ty::Class(n) => n.clone(),
            Ty::Network => "neural network".into(),
            Ty::Optional(t) => format!("{}?", t.describe()),
            Ty::Function(_) => "function".into(),
            Ty::Dynamic => "anything".into(),
        }
    }
}

struct ClassType {
    fields: Vec<(String, Ty)>,
    methods: HashMap<String, Rc<FunctionDecl>>,
}

struct FnEntry {
    decl: Rc<FunctionDecl>,
    self_class: Option<String>,
}

pub struct Checker {
    classes: HashMap<String, Rc<ClassType>>,
    /// For each class, the set of fields that have a default value (so a
    /// literal may omit them).
    class_defaults: HashMap<String, HashSet<String>>,
    /// Signatures of every callable function/method, keyed by id.
    sigs: HashMap<String, Rc<FnSig>>,
    bodies: HashMap<String, FnEntry>,
    ret_cache: HashMap<String, Ty>,
    inferring: HashSet<String>,
    checked: HashSet<String>,
    /// A stack of variable scopes. Only functions push a scope; `if`/`while`/
    /// `for` share their enclosing function's scope, matching the interpreter.
    scopes: Vec<HashMap<String, Ty>>,
    /// Flow-narrowed places (e.g. `x` or `c.nickname` proven non-nothing).
    narrowed: HashMap<String, Ty>,
    /// Standard-library modules brought in with `import`.
    imports: HashSet<String>,
    errors: Vec<Diagnostic>,
}

impl Checker {
    pub fn new() -> Checker {
        Checker {
            classes: HashMap::new(),
            class_defaults: HashMap::new(),
            sigs: HashMap::new(),
            bodies: HashMap::new(),
            ret_cache: HashMap::new(),
            inferring: HashSet::new(),
            checked: HashSet::new(),
            scopes: vec![HashMap::new()],
            narrowed: HashMap::new(),
            imports: HashSet::new(),
            errors: Vec::new(),
        }
    }

    /// Check a program. Returns the collected diagnostics (empty = all good).
    pub fn check(mut self, program: &Program) -> Vec<Diagnostic> {
        // Collect imported modules first (order-independent, like the runtime).
        for stmt in &program.statements {
            if let Stmt::Import { module, .. } = stmt {
                self.imports.insert(module.clone());
            }
        }
        // Math constants come with `import math`; colors/align words are always on.
        if self.imports.contains("math") {
            self.declare("pi", Ty::Number);
            self.declare("e", Ty::Number);
        }
        if self.imports.contains("ai") {
            for word in ["sgd", "adam", "momentum", "rmsprop", "cpu", "gpu", "auto", "cuda", "rocm", "mps", "vulkan", "dx12"] {
                self.declare(word, Ty::Text);
            }
        }
        for (name, _) in crate::gfx::named_colors() {
            self.declare(name, Ty::List(Box::new(Ty::Number)));
        }
        for word in crate::ui::align_words() {
            self.declare(word, Ty::Text);
        }
        self.register_classes(&program.statements);
        self.register_functions(&program.statements);

        // Check top-level (non-declaration) statements in source order, so
        // global variable types are known.
        for stmt in &program.statements {
            if matches!(stmt, Stmt::Function(_) | Stmt::Class(_)) {
                continue;
            }
            self.check_stmt(stmt);
        }

        // Check every function/method body once.
        let ids: Vec<String> = self.bodies.keys().cloned().collect();
        for id in ids {
            self.ensure_fn(&id);
        }

        self.errors
    }

    // ---- registration ----------------------------------------------------

    fn register_classes(&mut self, stmts: &[Stmt]) {
        // First pass: names, so field/param types can reference any class.
        let names: Vec<String> = stmts
            .iter()
            .filter_map(|s| if let Stmt::Class(d) = s { Some(d.name.clone()) } else { None })
            .collect();
        for name in &names {
            self.classes.insert(
                name.clone(),
                Rc::new(ClassType { fields: Vec::new(), methods: HashMap::new() }),
            );
        }
        // Second pass: fill in field types and methods.
        for stmt in stmts {
            if let Stmt::Class(decl) = stmt {
                let mut fields = Vec::new();
                let mut defaults = HashSet::new();
                for f in &decl.fields {
                    let ty = self.field_type(f);
                    fields.push((f.name.clone(), ty));
                    if f.default.is_some() {
                        defaults.insert(f.name.clone());
                    }
                }
                self.class_defaults.insert(decl.name.clone(), defaults);
                let mut methods = HashMap::new();
                for m in &decl.methods {
                    let id = format!("{}::{}", decl.name, m.name);
                    let decl_rc = Rc::new(m.clone());
                    methods.insert(m.name.clone(), decl_rc.clone());
                    self.sigs.insert(id.clone(), Rc::new(self.build_sig(Some(id.clone()), m)));
                    self.bodies.insert(id, FnEntry { decl: decl_rc, self_class: Some(decl.name.clone()) });
                }
                self.classes.insert(decl.name.clone(), Rc::new(ClassType { fields, methods }));
            }
        }
    }

    fn register_functions(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            if let Stmt::Function(decl) = stmt {
                let id = decl.name.clone();
                let decl_rc = Rc::new(decl.clone());
                let sig = Rc::new(self.build_sig(Some(id.clone()), decl));
                self.sigs.insert(id.clone(), sig.clone());
                self.bodies.insert(id.clone(), FnEntry { decl: decl_rc, self_class: None });
                self.declare(&decl.name, Ty::Function(sig));
            }
        }
    }

    fn build_sig(&self, id: Option<String>, decl: &FunctionDecl) -> FnSig {
        let mut params = Vec::new();
        for p in &decl.params {
            // Annotated → that type. Bare (no type) → Dynamic (gradual typing).
            // A default value lets us infer the type when there's no annotation.
            let ty = match &p.ty {
                Some(ann) => self.resolve_ann(ann),
                None => match &p.default {
                    Some(d) => self.infer_default(d),
                    None => Ty::Dynamic,
                },
            };
            params.push(ty);
        }
        let required = decl.params.iter().filter(|p| p.default.is_none()).count();
        FnSig { id, params, required }
    }

    fn field_type(&self, f: &Field) -> Ty {
        match &f.ty {
            Some(ann) => self.resolve_ann(ann),
            None => match &f.default {
                Some(d) => self.infer_default(d),
                None => Ty::Dynamic, // parser guarantees one of the two exists
            },
        }
    }

    /// A cheap, standalone inference for default-value expressions (literals and
    /// simple constructions), used before scopes exist.
    fn infer_default(&self, e: &Expr) -> Ty {
        match e {
            Expr::Number(..) => Ty::Number,
            Expr::Text(..) => Ty::Text,
            Expr::Bool(..) => Ty::Bool,
            Expr::Nothing(..) => Ty::Nothing,
            Expr::ClassLit { name, .. } if self.classes.contains_key(name) => Ty::Class(name.clone()),
            Expr::Unary { op: UnaryOp::Neg, .. } => Ty::Number,
            _ => Ty::Dynamic,
        }
    }

    fn resolve_ann(&self, ann: &TypeAnn) -> Ty {
        match ann {
            TypeAnn::Named { name, optional } => {
                let base = match name.as_str() {
                    "Number" => Ty::Number,
                    "Text" => Ty::Text,
                    "Boolean" | "Bool" => Ty::Bool,
                    "Nothing" => Ty::Nothing,
                    "Anything" | "Dynamic" => Ty::Dynamic,
                    other => {
                        if self.classes.contains_key(other) {
                            Ty::Class(other.to_string())
                        } else {
                            // Unknown type name: fall back to Dynamic. (A
                            // dedicated diagnostic could go here later.)
                            Ty::Dynamic
                        }
                    }
                };
                if *optional {
                    Ty::Optional(Box::new(base))
                } else {
                    base
                }
            }
            TypeAnn::List(inner) => Ty::List(Box::new(self.resolve_ann(inner))),
            TypeAnn::Dictionary(k, v) => {
                Ty::Dictionary(Box::new(self.resolve_ann(k)), Box::new(self.resolve_ann(v)))
            }
        }
    }

    // ---- function-body checking + return inference -----------------------

    fn ensure_fn(&mut self, id: &str) {
        if self.checked.contains(id) || self.inferring.contains(id) {
            return;
        }
        self.inferring.insert(id.to_string());

        let entry = match self.bodies.get(id) {
            Some(e) => FnEntry { decl: e.decl.clone(), self_class: e.self_class.clone() },
            None => {
                self.inferring.remove(id);
                return;
            }
        };

        self.scopes.push(HashMap::new());
        if let Some(thing) = &entry.self_class {
            self.declare("self", Ty::Class(thing.clone()));
        }
        // Bind parameters using the already-computed signature.
        if let Some(sig) = self.sigs.get(id).cloned() {
            for (p, ty) in entry.decl.params.iter().zip(sig.params.iter()) {
                self.declare(&p.name, ty.clone());
            }
        }

        let mut returns: Vec<Ty> = Vec::new();
        self.check_block(&entry.decl.body, &mut returns);
        if !always_returns(&entry.decl.body) {
            returns.push(Ty::Nothing);
        }
        self.scopes.pop();

        let ret = self.join_returns(&returns, entry.decl.span);
        self.ret_cache.insert(id.to_string(), ret);
        self.inferring.remove(id);
        self.checked.insert(id.to_string());
    }

    fn return_of(&mut self, id: &str) -> Ty {
        if let Some(t) = self.ret_cache.get(id) {
            return t.clone();
        }
        if self.inferring.contains(id) {
            return Ty::Dynamic; // recursive call: break the cycle
        }
        self.ensure_fn(id);
        self.ret_cache.get(id).cloned().unwrap_or(Ty::Dynamic)
    }

    fn join_returns(&mut self, returns: &[Ty], span: Span) -> Ty {
        let mut acc: Option<Ty> = None;
        for t in returns {
            acc = Some(match acc {
                None => t.clone(),
                Some(prev) => match join(&prev, t) {
                    Some(j) => j,
                    None => {
                        self.error(
                            span,
                            format!(
                                "this function returns different types ({} and {})",
                                prev.describe(),
                                t.describe()
                            ),
                            Some("every path should return the same kind of value".into()),
                        );
                        Ty::Dynamic
                    }
                },
            });
        }
        acc.unwrap_or(Ty::Nothing)
    }

    // ---- statements ------------------------------------------------------

    fn check_block(&mut self, stmts: &[Stmt], returns: &mut Vec<Ty>) {
        for stmt in stmts {
            self.check_stmt_ret(stmt, returns);
        }
    }

    fn check_stmt(&mut self, stmt: &Stmt) {
        let mut ignored = Vec::new();
        self.check_stmt_ret(stmt, &mut ignored);
    }

    fn check_stmt_ret(&mut self, stmt: &Stmt, returns: &mut Vec<Ty>) {
        match stmt {
            Stmt::Assign { target, ty, value, span } => {
                let value_ty = self.type_of(value);
                self.check_assign(target, ty.as_ref(), value_ty, value, *span);
            }
            Stmt::Function(decl) => {
                // A nested function: register and check it locally.
                let id = decl.name.clone();
                let sig = Rc::new(self.build_sig(Some(id.clone()), decl));
                self.sigs.insert(id.clone(), sig.clone());
                self.bodies.insert(id.clone(), FnEntry { decl: Rc::new(decl.clone()), self_class: None });
                self.declare(&decl.name, Ty::Function(sig));
                self.ensure_fn(&id);
            }
            Stmt::Class(_) => {}
            Stmt::If { branches, else_body, .. } => {
                for (cond, body) in branches {
                    self.expect_bool(cond);
                    let restore = self.apply_narrowing(cond);
                    self.check_block(body, returns);
                    self.undo_narrowing(restore);
                }
                if let Some(body) = else_body {
                    self.check_block(body, returns);
                }
            }
            Stmt::While { cond, body, .. } => {
                self.expect_bool(cond);
                self.check_block(body, returns);
            }
            Stmt::ForEvery { var, iterable, body, span } => {
                let seq = self.type_of(iterable);
                let elem = self.element_type(&seq, *span);
                self.declare(var, elem);
                self.check_block(body, returns);
            }
            Stmt::Repeat { count, body, .. } => {
                let t = self.type_of(count);
                self.expect(&t, &Ty::Number, count.span(), "the repeat count must be a number");
                self.check_block(body, returns);
            }
            Stmt::Loop { body, .. } => self.check_block(body, returns),
            Stmt::Return { value, .. } => {
                let t = match value {
                    Some(e) => self.type_of(e),
                    None => Ty::Nothing,
                };
                returns.push(t);
            }
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Import { module, span } => {
                if module != "math" && module != "ai" {
                    self.error(
                        *span,
                        format!("unknown module `{}`", module),
                        Some("the modules are `math` and `ai`".into()),
                    );
                }
            }
            Stmt::ImportFile { .. } => {} // resolved and spliced away before checking
            Stmt::Window(win) => {
                for (_, v) in &win.props {
                    self.type_of(v);
                }
                self.scopes.push(HashMap::new());
                for w in &win.root {
                    self.check_widget(w);
                }
                self.scopes.pop();
            }
            Stmt::Game(game) => {
                // Property values are checked; then init + hooks run in a fresh
                // game scope (a new function-like scope).
                for (_, v) in &game.props {
                    self.type_of(v);
                }
                self.scopes.push(HashMap::new());
                for stmt in &game.init {
                    self.check_stmt(stmt);
                }
                for hook in &game.hooks {
                    self.scopes.push(HashMap::new());
                    for p in &hook.params {
                        self.declare(p, Ty::Dynamic);
                    }
                    let mut ignored = Vec::new();
                    self.check_block(&hook.body, &mut ignored);
                    self.scopes.pop();
                }
                self.scopes.pop();
            }
            Stmt::Expr(e) => {
                self.type_of(e);
            }
        }
    }

    fn check_widget(&mut self, w: &Widget) {
        const WIDGETS: &[&str] = &[
            "column", "row", "text", "button", "spacer", "text_field", "checkbox", "slider", "image",
        ];
        if !WIDGETS.contains(&w.name.as_str()) {
            self.error(
                w.span,
                format!("unknown widget `{}`", w.name),
                Some("widgets are column, row, text, button, spacer, text_field, checkbox, slider, image".into()),
            );
        }
        if let Some(label) = &w.label {
            self.type_of(label);
        }
        let mut has_bind = false;
        let mut has_value = false;
        for (name, expr) in &w.props {
            if name == "bind" {
                has_bind = true;
                // `bind:` needs the variable's name, not an expression.
                if !matches!(expr, Expr::Ident(_, _)) {
                    self.error(
                        expr.span(),
                        "bind needs a variable name".into(),
                        Some("write bind: myVariable".into()),
                    );
                } else {
                    self.type_of(expr); // still resolve it so unknown names error
                }
                continue;
            }
            if name == "value" || name == "checked" {
                has_value = true;
            }
            let t = self.type_of(expr);
            if (name == "on_click" || name == "on_change") && !matches!(t, Ty::Function(_) | Ty::Dynamic) {
                self.error(
                    expr.span(),
                    format!("{} needs a function, but this is a {}", name, t.describe()),
                    Some("pass a function, e.g. on_change: make function (new) { name = new }".into()),
                );
            }
        }
        // Interactive inputs rebuild from your variables each frame — without
        // `bind:` or `value:`/`checked:`, typed clicks have nowhere to stick.
        if matches!(w.name.as_str(), "text_field" | "checkbox" | "slider") && !has_bind && !has_value {
            self.error(
                w.span,
                format!("`{}` needs bind: or value: so its state can stick", w.name),
                Some("example: text_field (bind: name, width: 320)".into()),
            );
        }
        for child in &w.children {
            self.check_widget(child);
        }
    }

    fn check_assign(&mut self, target: &Expr, ann: Option<&TypeAnn>, value_ty: Ty, value: &Expr, span: Span) {
        match target {
            Expr::Ident(name, _) => {
                if let Some(ann) = ann {
                    // Explicit annotation: value must fit it; declares/updates.
                    let declared = self.resolve_ann(ann);
                    if !assignable(&value_ty, &declared) {
                        self.mismatch(span, &value_ty, &declared);
                    }
                    self.set_or_declare(name, declared);
                } else if let Some(existing) = self.lookup(name) {
                    // Reassignment: must stay compatible with the known type.
                    if !assignable(&value_ty, &existing) {
                        self.mismatch(span, &value_ty, &existing);
                    }
                } else {
                    // First write declares the variable — but a bare empty list
                    // or bare `nothing` has no inferable type.
                    self.require_inferable(&value_ty, value, span);
                    self.declare(name, value_ty);
                }
            }
            Expr::Field { object, name, span: fspan } => {
                let obj = self.type_of(object);
                match obj {
                    Ty::Class(tn) => {
                        if let Some(field_ty) = self.field_of(&tn, name) {
                            if !assignable(&value_ty, &field_ty) {
                                self.mismatch(span, &value_ty, &field_ty);
                            }
                        } else {
                            self.error(*fspan, format!("`{}` has no field `{}`", tn, name), None);
                        }
                    }
                    Ty::Dynamic => {}
                    other => self.error(
                        *fspan,
                        format!("can't set field `{}` on a {}", name, other.describe()),
                        None,
                    ),
                }
            }
            Expr::Index { object, index, span: ispan } => {
                let obj = self.type_of(object);
                let idx = self.type_of(index);
                match obj {
                    Ty::List(elem) => {
                        self.expect(&idx, &Ty::Number, *ispan, "a list index must be a number");
                        if !assignable(&value_ty, &elem) {
                            self.mismatch(span, &value_ty, &elem);
                        }
                    }
                    Ty::Dictionary(_, v) => {
                        if !assignable(&value_ty, &v) {
                            self.mismatch(span, &value_ty, &v);
                        }
                    }
                    Ty::Dynamic => {}
                    other => self.error(
                        *ispan,
                        format!("can't index into a {}", other.describe()),
                        None,
                    ),
                }
            }
            _ => self.error(
                span,
                "the left side of `=` must be a variable, field, or element".into(),
                None,
            ),
        }
    }

    fn require_inferable(&mut self, ty: &Ty, expr: &Expr, span: Span) {
        match (ty, expr) {
            (Ty::List(inner), Expr::ListLit { items, .. }) if items.is_empty() && matches!(**inner, Ty::Dynamic) => {
                self.error(
                    span,
                    "this empty list needs a type".into(),
                    Some("annotate it, e.g. `scores: Number list = []`".into()),
                );
            }
            (Ty::Nothing, Expr::Nothing(_)) => {
                self.error(
                    span,
                    "`nothing` on its own needs a type".into(),
                    Some("annotate it, e.g. `nickname: Text? = nothing`".into()),
                );
            }
            _ => {}
        }
    }

    // ---- expressions -----------------------------------------------------

    fn type_of(&mut self, expr: &Expr) -> Ty {
        // Flow narrowing wins over the declared type.
        if let Some(place) = place_of(expr) {
            if let Some(t) = self.narrowed.get(&place) {
                return t.clone();
            }
        }
        match expr {
            Expr::Number(..) => Ty::Number,
            Expr::Text(chunks, _) => {
                for c in chunks {
                    if let StrChunk::Expr(e) = c {
                        self.type_of(e); // interpolated pieces are still checked
                    }
                }
                Ty::Text
            }
            Expr::Bool(..) => Ty::Bool,
            Expr::Nothing(_) => Ty::Nothing,
            Expr::SelfRef(span) => self.lookup("self").unwrap_or_else(|| {
                self.error(*span, "`self` is only available inside a method".into(), None);
                Ty::Dynamic
            }),
            Expr::Ident(name, span) => self.type_of_ident(name, *span),
            Expr::Unary { op, expr, span } => {
                let t = self.type_of(expr);
                if matches!(t, Ty::Dynamic) {
                    return Ty::Dynamic;
                }
                match op {
                    UnaryOp::Neg => {
                        self.expect(&t, &Ty::Number, *span, "only numbers can be negated");
                        Ty::Number
                    }
                    UnaryOp::Not => {
                        self.expect(&t, &Ty::Bool, *span, "`not` needs a true/false value");
                        Ty::Bool
                    }
                }
            }
            Expr::Binary { op, left, right, span } => self.type_of_binary(*op, left, right, *span),
            Expr::IsNothing { expr, .. } => {
                self.type_of(expr);
                Ty::Bool
            }
            Expr::Try { expr, .. } => {
                // `try e` yields `nothing` on failure, so its type gains a `?`.
                match self.type_of(expr) {
                    Ty::Dynamic => Ty::Dynamic,
                    already @ Ty::Optional(_) => already,
                    other => Ty::Optional(Box::new(other)),
                }
            }
            Expr::Otherwise { value, fallback, span } => {
                let lt = self.type_of(value);
                let ft = self.type_of(fallback);
                if matches!(lt, Ty::Dynamic) || matches!(ft, Ty::Dynamic) {
                    return Ty::Dynamic;
                }
                // The value's `?` is discharged; the result is its inner type,
                // reconciled with the fallback.
                let inner = match lt {
                    Ty::Optional(inner) => *inner,
                    other => other,
                };
                join(&inner, &ft).unwrap_or_else(|| {
                    self.error(
                        *span,
                        format!("the `otherwise` fallback is a {}, but the value is a {}", ft.describe(), inner.describe()),
                        Some("make the fallback the same type as the value".into()),
                    );
                    inner
                })
            }
            Expr::ListLit { items, .. } => {
                let mut elem: Option<Ty> = None;
                for it in items {
                    let t = self.type_of(it);
                    elem = Some(match elem {
                        None => t,
                        Some(prev) => join(&prev, &t).unwrap_or_else(|| {
                            self.error(
                                it.span(),
                                "this list mixes different types".into(),
                                Some("every item should be the same kind of value".into()),
                            );
                            Ty::Dynamic
                        }),
                    });
                }
                Ty::List(Box::new(elem.unwrap_or(Ty::Dynamic)))
            }
            Expr::DictionaryLit { entries, .. } => {
                let mut kt: Option<Ty> = None;
                let mut vt: Option<Ty> = None;
                for (k, v) in entries {
                    let k2 = self.type_of(k);
                    let v2 = self.type_of(v);
                    kt = Some(kt.map_or(k2.clone(), |p| join(&p, &k2).unwrap_or(Ty::Dynamic)));
                    vt = Some(vt.map_or(v2.clone(), |p| join(&p, &v2).unwrap_or(Ty::Dynamic)));
                }
                Ty::Dictionary(Box::new(kt.unwrap_or(Ty::Dynamic)), Box::new(vt.unwrap_or(Ty::Dynamic)))
            }
            Expr::ClassLit { name, fields, span } => self.type_of_class_lit(name, fields, *span),
            Expr::Field { object, name, span } => {
                let obj = self.type_of(object);
                self.field_access(&obj, name, *span)
            }
            Expr::Index { object, index, span } => {
                let obj = self.type_of(object);
                let idx = self.type_of(index);
                self.index_access(&obj, &idx, *span)
            }
            Expr::Call { callee, args, span } => self.type_of_call(callee, args, *span),
            Expr::Function { decl, .. } => self.type_of_lambda(decl),
            Expr::Wait { expr, .. } => {
                self.type_of(expr);
                Ty::Dynamic
            }
        }
    }

    /// Type an anonymous function, checking its body inline so errors inside it
    /// are reported. The body closes over the current scopes, matching how the
    /// interpreter captures the enclosing environment. Its return type is left
    /// unresolved (`sig.id` is `None`), so calling it yields `Dynamic` — good
    /// enough for v1, where lambdas are mostly passed to list/UI helpers.
    fn type_of_lambda(&mut self, decl: &Rc<FunctionDecl>) -> Ty {
        let sig = Rc::new(self.build_sig(None, decl));
        self.scopes.push(HashMap::new());
        for (p, ty) in decl.params.iter().zip(sig.params.iter()) {
            self.declare(&p.name, ty.clone());
        }
        let mut returns = Vec::new();
        self.check_block(&decl.body, &mut returns);
        self.scopes.pop();
        Ty::Function(sig)
    }

    fn type_of_ident(&mut self, name: &str, span: Span) -> Ty {
        if let Some(t) = self.lookup(name) {
            return t;
        }
        if let Some(b) = crate::value::Builtin::from_name(name) {
            if b.is_math() && !self.imports.contains("math") {
                self.error(
                    span,
                    format!("`{}` needs the math module", name),
                    Some("add `import math` at the top of your file".into()),
                );
            }
            if b.is_ai() && !self.imports.contains("ai") {
                self.error(
                    span,
                    format!("`{}` needs the ai module", name),
                    Some("add `import ai` at the top of your file".into()),
                );
            }
            return Ty::Dynamic; // builtins are handled at their call sites
        }
        self.error(
            span,
            format!("unknown name `{}`", name),
            Some("check the spelling, or define it before this point".into()),
        );
        Ty::Dynamic
    }

    fn type_of_binary(&mut self, op: BinaryOp, left: &Expr, right: &Expr, span: Span) -> Ty {
        if matches!(op, BinaryOp::And | BinaryOp::Or) {
            self.expect_bool(left);
            self.expect_bool(right);
            return Ty::Bool;
        }
        let l = self.type_of(left);
        let r = self.type_of(right);
        if matches!(l, Ty::Dynamic) || matches!(r, Ty::Dynamic) {
            return match op {
                BinaryOp::Eq | BinaryOp::NotEq | BinaryOp::Lt | BinaryOp::LtEq | BinaryOp::Gt | BinaryOp::GtEq => Ty::Bool,
                _ => Ty::Dynamic,
            };
        }
        match op {
            BinaryOp::Eq | BinaryOp::NotEq => Ty::Bool,
            BinaryOp::Lt | BinaryOp::LtEq | BinaryOp::Gt | BinaryOp::GtEq => {
                let ok = matches!((&l, &r), (Ty::Number, Ty::Number) | (Ty::Text, Ty::Text));
                if !ok {
                    self.error(
                        span,
                        format!("can't compare a {} with a {}", l.describe(), r.describe()),
                        Some("`<`, `>` etc. compare two numbers or two texts".into()),
                    );
                }
                Ty::Bool
            }
            BinaryOp::Add => match (&l, &r) {
                (Ty::Number, Ty::Number) => Ty::Number,
                (Ty::Text, Ty::Text) => Ty::Text,
                _ => {
                    let hint = if matches!((&l, &r), (Ty::Number, Ty::Text) | (Ty::Text, Ty::Number)) {
                        Some("to join text and a number, use interpolation like \"{x}\" or to_text(x)".into())
                    } else {
                        None
                    };
                    self.error(span, format!("can't add a {} and a {}", l.describe(), r.describe()), hint);
                    Ty::Dynamic
                }
            },
            BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
                if !matches!((&l, &r), (Ty::Number, Ty::Number)) {
                    self.error(
                        span,
                        format!("this arithmetic needs two numbers, got a {} and a {}", l.describe(), r.describe()),
                        None,
                    );
                }
                Ty::Number
            }
            BinaryOp::And | BinaryOp::Or => unreachable!(),
        }
    }

    fn type_of_class_lit(&mut self, name: &str, fields: &[(String, Expr)], span: Span) -> Ty {
        let def = match self.classes.get(name).cloned() {
            Some(d) => d,
            None => {
                self.error(span, format!("unknown class `{}`", name), None);
                return Ty::Dynamic;
            }
        };
        let mut given: HashMap<String, Ty> = HashMap::new();
        for (fname, fexpr) in fields {
            let vt = self.type_of(fexpr);
            match def.fields.iter().find(|(n, _)| n == fname) {
                Some((_, ft)) => {
                    if !assignable(&vt, ft) {
                        self.mismatch(fexpr.span(), &vt, ft);
                    }
                }
                None => self.error(span, format!("`{}` has no field `{}`", name, fname), None),
            }
            given.insert(fname.clone(), vt);
        }
        // Every field must be supplied, unless it has a default or is optional
        // (an omitted optional field defaults to nothing).
        for (fname, fty) in &def.fields {
            let omittable = self.field_has_default(name, fname) || matches!(fty, Ty::Optional(_));
            if !given.contains_key(fname) && !omittable {
                self.error(
                    span,
                    format!("`{}` is missing a value for field `{}`", name, fname),
                    None,
                );
            }
        }
        Ty::Class(name.to_string())
    }

    fn type_of_call(&mut self, callee: &Expr, args: &[Expr], span: Span) -> Ty {
        // Method call: receiver.method(args)
        if let Expr::Field { object, name, span: fspan } = callee {
            let recv = self.type_of(object);
            let arg_tys: Vec<Ty> = args.iter().map(|a| self.type_of(a)).collect();
            return self.method_return(&recv, name, &arg_tys, args, *fspan);
        }

        // Named function call.
        if let Expr::Ident(fname, _) = callee {
            if let Some(b) = crate::value::Builtin::from_name(fname) {
                if b.is_math() && !self.imports.contains("math") {
                    self.error(
                        span,
                        format!("`{}` needs the math module", fname),
                        Some("add `import math` at the top of your file".into()),
                    );
                }
                if b.is_ai() && !self.imports.contains("ai") {
                    self.error(
                        span,
                        format!("`{}` needs the ai module", fname),
                        Some("add `import ai` at the top of your file".into()),
                    );
                }
                let arg_tys: Vec<Ty> = args.iter().map(|a| self.type_of(a)).collect();
                return self.builtin_return(b, &arg_tys, span);
            }
            if let Some(sig) = self.sigs.get(fname).cloned() {
                let arg_tys: Vec<Ty> = args.iter().map(|a| self.type_of(a)).collect();
                self.check_args(fname, &sig, &arg_tys, args, span);
                return sig.id.as_ref().map(|id| self.return_of(id)).unwrap_or(Ty::Dynamic);
            }
        }

        // A value that happens to be a function.
        let callee_ty = self.type_of(callee);
        let arg_tys: Vec<Ty> = args.iter().map(|a| self.type_of(a)).collect();
        match callee_ty {
            Ty::Function(sig) => {
                self.check_args("this function", &sig, &arg_tys, args, span);
                sig.id.as_ref().map(|id| self.return_of(id)).unwrap_or(Ty::Dynamic)
            }
            Ty::Dynamic => Ty::Dynamic,
            other => {
                self.error(span, format!("a {} isn't something you can call", other.describe()), None);
                Ty::Dynamic
            }
        }
    }

    fn check_args(&mut self, name: &str, sig: &FnSig, arg_tys: &[Ty], args: &[Expr], span: Span) {
        if arg_tys.len() > sig.params.len() || arg_tys.len() < sig.required {
            self.error(
                span,
                format!("`{}` takes {} argument(s), but got {}", name, sig.params.len(), arg_tys.len()),
                None,
            );
            return;
        }
        for (i, (at, pt)) in arg_tys.iter().zip(sig.params.iter()).enumerate() {
            if !assignable(at, pt) {
                self.error(
                    args[i].span(),
                    format!("argument {} should be a {}, but it's a {}", i + 1, pt.describe(), at.describe()),
                    None,
                );
            }
        }
    }

    fn method_return(&mut self, recv: &Ty, name: &str, arg_tys: &[Ty], args: &[Expr], span: Span) -> Ty {
        match recv {
            Ty::Dynamic => Ty::Dynamic,
            Ty::Optional(_) => {
                self.error(
                    span,
                    format!("this might be nothing, so `.{}(...)` isn't safe", name),
                    Some("check `if x is not nothing { ... }` first".into()),
                );
                Ty::Dynamic
            }
            Ty::Class(tn) => {
                let method = self.classes.get(tn).and_then(|t| t.methods.get(name).cloned());
                if method.is_some() {
                    let id = format!("{}::{}", tn, name);
                    if let Some(sig) = self.sigs.get(&id).cloned() {
                        self.check_args(name, &sig, arg_tys, args, span);
                    }
                    return self.return_of(&id);
                }
                // Field holding a function.
                if let Some(ft) = self.field_of(tn, name) {
                    if let Ty::Function(sig) = ft {
                        return sig.id.as_ref().map(|id| self.return_of(id)).unwrap_or(Ty::Dynamic);
                    }
                }
                self.error(span, format!("`{}` has no method `{}`", tn, name), None);
                Ty::Dynamic
            }
            Ty::List(elem) => self.list_method(elem, name, span),
            Ty::Text => self.text_method(name, span),
            Ty::Dictionary(k, v) => self.map_method(k, v, name, span),
            Ty::Network => self.network_method(name, span),
            other => {
                self.error(span, format!("a {} has no method `{}`", other.describe(), name), None);
                Ty::Dynamic
            }
        }
    }

    fn network_method(&mut self, name: &str, span: Span) -> Ty {
        match name {
            "predict" => Ty::List(Box::new(Ty::Number)),
            "train" | "train_once" | "loss" | "error" => Ty::Number,
            "save" => Ty::Nothing,
            _ => {
                self.error(span, format!("a neural network has no method `{}`", name), None);
                Ty::Dynamic
            }
        }
    }

    fn list_method(&mut self, elem: &Ty, name: &str, span: Span) -> Ty {
        match name {
            "length" | "index_of" => Ty::Number,
            "is_empty" | "contains" => Ty::Bool,
            "append" | "add" => Ty::Nothing,
            "pop" | "get" | "first" | "last" | "remove_at" => (*elem).clone(),
            "reversed" | "sorted" | "kept_if" => Ty::List(Box::new((*elem).clone())),
            "transformed_by" => Ty::List(Box::new(Ty::Dynamic)),
            "combined" => Ty::Dynamic,
            "join" => Ty::Text,
            _ => {
                self.error(span, format!("a list has no method `{}`", name), None);
                Ty::Dynamic
            }
        }
    }

    fn text_method(&mut self, name: &str, span: Span) -> Ty {
        match name {
            "length" => Ty::Number,
            "upper" | "lower" | "trim" | "replace" | "repeat" | "substring" => Ty::Text,
            "contains" | "starts_with" | "ends_with" => Ty::Bool,
            "split" => Ty::List(Box::new(Ty::Text)),
            _ => {
                self.error(span, format!("text has no method `{}`", name), None);
                Ty::Dynamic
            }
        }
    }

    fn map_method(&mut self, k: &Ty, v: &Ty, name: &str, span: Span) -> Ty {
        match name {
            "length" => Ty::Number,
            "has" | "is_empty" => Ty::Bool,
            "get" | "remove" => Ty::Optional(Box::new((*v).clone())),
            "keys" => Ty::List(Box::new((*k).clone())),
            "values" => Ty::List(Box::new((*v).clone())),
            _ => {
                self.error(span, format!("a dictionary has no method `{}`", name), None);
                Ty::Dynamic
            }
        }
    }

    fn builtin_return(&mut self, b: crate::value::Builtin, arg_tys: &[Ty], span: Span) -> Ty {
        use crate::value::Builtin::*;
        let _ = arg_tys;
        let _ = span;
        match b {
            Print | Exit | WriteFile | AppendFile | ClearScreen | DrawCircle | DrawRectangle | DrawLine
            | DrawText | DrawSprite | DrawSpriteScaled | DrawSpriteRotated | PlaySound | After
            | Every => Ty::Nothing,
            ToText | ReadFile | Input => Ty::Text,
            FileExists | KeyDown | KeyPressed | MouseDown | MousePressed => Ty::Bool,
            Rgb | Rgba => Ty::List(Box::new(Ty::Number)),
            ToNumber | Length | Min | Greatest | Abs | Sqrt | Floor | Ceil | Round | RandomBetween
            | Pow | Clamp | Sin | Cos | Tan | Now | Clock | ScreenWidth | ScreenHeight | MouseX
            | MouseY | LoadSprite | SpriteWidth | SpriteHeight | LoadSound | LoadFont => Ty::Number,
            NeuralNetwork | LoadNetwork | BestOf => Ty::Network,
            Population | Evolve => Ty::List(Box::new(Ty::Network)),
            // read_csv → rows of numbers; load_dataset → [examples, answers].
            ReadCsv => Ty::List(Box::new(Ty::List(Box::new(Ty::Number)))),
            LoadDataset => Ty::List(Box::new(Ty::List(Box::new(Ty::List(Box::new(Ty::Number)))))),
        }
    }

    fn field_access(&mut self, obj: &Ty, name: &str, span: Span) -> Ty {
        match obj {
            Ty::Dynamic => Ty::Dynamic,
            Ty::Class(tn) => {
                if let Some(ft) = self.field_of(tn, name) {
                    ft
                } else if self.classes.get(tn).map_or(false, |t| t.methods.contains_key(name)) {
                    let id = format!("{}::{}", tn, name);
                    self.sigs.get(&id).cloned().map(Ty::Function).unwrap_or(Ty::Dynamic)
                } else {
                    self.error(span, format!("`{}` has no field or method `{}`", tn, name), None);
                    Ty::Dynamic
                }
            }
            Ty::Optional(_) => {
                self.error(
                    span,
                    format!("this might be nothing, so `.{}` isn't safe", name),
                    Some("check `if x is not nothing { ... }` first".into()),
                );
                Ty::Dynamic
            }
            other => {
                self.error(
                    span,
                    format!("a {} has no field `{}`", other.describe(), name),
                    None,
                );
                Ty::Dynamic
            }
        }
    }

    fn index_access(&mut self, obj: &Ty, idx: &Ty, span: Span) -> Ty {
        match obj {
            Ty::Dynamic => Ty::Dynamic,
            Ty::List(elem) => {
                self.expect(idx, &Ty::Number, span, "a list index must be a number");
                (**elem).clone()
            }
            Ty::Text => {
                self.expect(idx, &Ty::Number, span, "a text index must be a number");
                Ty::Text
            }
            Ty::Dictionary(_, v) => (**v).clone(),
            other => {
                self.error(span, format!("can't index into a {}", other.describe()), None);
                Ty::Dynamic
            }
        }
    }

    fn element_type(&mut self, seq: &Ty, span: Span) -> Ty {
        match seq {
            Ty::Dynamic => Ty::Dynamic,
            Ty::List(e) => (**e).clone(),
            Ty::Text => Ty::Text,
            Ty::Dictionary(k, _) => (**k).clone(),
            other => {
                self.error(
                    span,
                    format!("can't loop over a {}", other.describe()),
                    Some("`for every` works on a list, text, or dictionary".into()),
                );
                Ty::Dynamic
            }
        }
    }

    // ---- narrowing -------------------------------------------------------

    /// If `cond` is `<place> is not nothing`, narrow that place to its inner
    /// type for the guarded block. Returns the previous binding to restore.
    fn apply_narrowing(&mut self, cond: &Expr) -> Option<(String, Option<Ty>)> {
        if let Expr::IsNothing { expr, negated: true, .. } = cond {
            if let Some(place) = place_of(expr) {
                let current = self.type_of(expr);
                if let Ty::Optional(inner) = current {
                    let prev = self.narrowed.insert(place.clone(), *inner);
                    return Some((place, prev));
                }
            }
        }
        None
    }

    fn undo_narrowing(&mut self, restore: Option<(String, Option<Ty>)>) {
        if let Some((place, prev)) = restore {
            match prev {
                Some(t) => {
                    self.narrowed.insert(place, t);
                }
                None => {
                    self.narrowed.remove(&place);
                }
            }
        }
    }

    // ---- scope + helpers -------------------------------------------------

    fn declare(&mut self, name: &str, ty: Ty) {
        self.scopes.last_mut().unwrap().insert(name.to_string(), ty);
    }

    fn set_or_declare(&mut self, name: &str, ty: Ty) {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), ty);
                return;
            }
        }
        self.scopes.last_mut().unwrap().insert(name.to_string(), ty);
    }

    fn lookup(&self, name: &str) -> Option<Ty> {
        for scope in self.scopes.iter().rev() {
            if let Some(t) = scope.get(name) {
                return Some(t.clone());
            }
        }
        None
    }

    fn field_of(&self, thing: &str, field: &str) -> Option<Ty> {
        self.classes
            .get(thing)
            .and_then(|t| t.fields.iter().find(|(n, _)| n == field))
            .map(|(_, t)| t.clone())
    }

    fn field_has_default(&self, thing: &str, field: &str) -> bool {
        self.class_defaults.get(thing).map_or(false, |set| set.contains(field))
    }

    fn expect(&mut self, actual: &Ty, expected: &Ty, span: Span, msg: &str) {
        if !assignable(actual, expected) {
            self.error(span, format!("{} (got a {})", msg, actual.describe()), None);
        }
    }

    fn expect_bool(&mut self, expr: &Expr) {
        let t = self.type_of(expr);
        if !matches!(t, Ty::Bool | Ty::Dynamic) {
            self.error(
                expr.span(),
                format!("this needs to be true or false, but it's a {}", t.describe()),
                Some("PlainText has no truthy/falsy values — use a comparison like `x > 0`".into()),
            );
        }
    }

    fn mismatch(&mut self, span: Span, from: &Ty, to: &Ty) {
        self.error(
            span,
            format!("expected a {}, but this is a {}", to.describe(), from.describe()),
            None,
        );
    }

    fn error(&mut self, span: Span, message: String, hint: Option<String>) {
        let mut d = Diagnostic::new(span, message);
        d.hint = hint;
        self.errors.push(d);
    }
}

/// Is a value of type `from` allowed where `to` is expected? `Dynamic` is
/// compatible in both directions (that's the whole point of gradual typing),
/// and `nothing`/`T` fit into `T?`.
fn assignable(from: &Ty, to: &Ty) -> bool {
    match (from, to) {
        (Ty::Dynamic, _) | (_, Ty::Dynamic) => true,
        (Ty::Number, Ty::Number)
        | (Ty::Text, Ty::Text)
        | (Ty::Bool, Ty::Bool)
        | (Ty::Nothing, Ty::Nothing) => true,
        (Ty::Class(a), Ty::Class(b)) => a == b,
        (Ty::Network, Ty::Network) => true,
        (Ty::List(a), Ty::List(b)) => assignable(a, b),
        (Ty::Dictionary(ak, av), Ty::Dictionary(bk, bv)) => assignable(ak, bk) && assignable(av, bv),
        // Into an optional: nothing fits, and the inner type fits.
        (Ty::Nothing, Ty::Optional(_)) => true,
        (Ty::Optional(a), Ty::Optional(b)) => assignable(a, b),
        (other, Ty::Optional(b)) => assignable(other, b),
        (Ty::Function(_), Ty::Function(_)) => true,
        _ => false,
    }
}

/// The common type of two branches, or `None` if they're genuinely
/// incompatible. Used for list-literal elements and inferred return types.
fn join(a: &Ty, b: &Ty) -> Option<Ty> {
    if matches!(a, Ty::Dynamic) || matches!(b, Ty::Dynamic) {
        return Some(Ty::Dynamic);
    }
    if types_equal(a, b) {
        return Some(a.clone());
    }
    match (a, b) {
        // `T` joined with `nothing` (or `T?`) becomes `T?`.
        (Ty::Nothing, Ty::Optional(t)) | (Ty::Optional(t), Ty::Nothing) => {
            Some(Ty::Optional(t.clone()))
        }
        (Ty::Nothing, t) | (t, Ty::Nothing) => Some(Ty::Optional(Box::new(t.clone()))),
        (Ty::Optional(t), other) | (other, Ty::Optional(t)) => {
            join(t, other).map(|j| Ty::Optional(Box::new(j)))
        }
        (Ty::List(x), Ty::List(y)) => join(x, y).map(|j| Ty::List(Box::new(j))),
        _ => None,
    }
}

fn types_equal(a: &Ty, b: &Ty) -> bool {
    match (a, b) {
        (Ty::Number, Ty::Number)
        | (Ty::Text, Ty::Text)
        | (Ty::Bool, Ty::Bool)
        | (Ty::Nothing, Ty::Nothing)
        | (Ty::Dynamic, Ty::Dynamic) => true,
        (Ty::Class(x), Ty::Class(y)) => x == y,
        (Ty::Network, Ty::Network) => true,
        (Ty::List(x), Ty::List(y)) => types_equal(x, y),
        (Ty::Dictionary(xk, xv), Ty::Dictionary(yk, yv)) => types_equal(xk, yk) && types_equal(xv, yv),
        (Ty::Optional(x), Ty::Optional(y)) => types_equal(x, y),
        (Ty::Function(_), Ty::Function(_)) => true,
        _ => false,
    }
}

/// Whether a block always leaves via `return` on every path (so no implicit
/// `nothing` return needs to be added). Conservative: unsure → false.
fn always_returns(block: &[Stmt]) -> bool {
    block.iter().any(stmt_always_returns)
}

fn stmt_always_returns(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Return { .. } => true,
        Stmt::If { branches, else_body, .. } => {
            else_body.as_ref().map_or(false, |e| always_returns(e))
                && branches.iter().all(|(_, b)| always_returns(b))
        }
        _ => false,
    }
}

/// A canonical string naming a narrowable place: a plain variable, or a one-
/// level field path like `c.nickname`. Returns `None` for anything else.
fn place_of(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(name, _) => Some(name.clone()),
        Expr::SelfRef(_) => Some("self".to_string()),
        Expr::Field { object, name, .. } => {
            let base = place_of(object)?;
            Some(format!("{}.{}", base, name))
        }
        _ => None,
    }
}
