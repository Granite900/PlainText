//! Runtime values and the lexical environment.
//!
//! Rust note: heap objects (lists, maps, classes) are wrapped in
//! `Rc<RefCell<T>>`. `Rc` is a shared-ownership pointer (reference counting)
//! and `RefCell` allows mutation through a shared reference, checked at run
//! time. This is the deliberate "bootstrap" heap from the plan — a real
//! tracing GC replaces it in a later milestone. Cloning a `Value` that holds
//! an `Rc` just bumps a counter, so two variables can share and mutate the
//! same list, matching how the language should behave.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use rustc_hash::FxHashMap;

use crate::ast::FunctionDecl;

pub type Env = Rc<RefCell<Scope>>;

#[derive(Clone)]
pub enum Value {
    Number(f64),
    Text(Rc<String>),
    Bool(bool),
    Nothing,
    List(Rc<RefCell<Vec<Value>>>),
    Dictionary(Rc<RefCell<PtMap>>),
    Class(Rc<RefCell<ClassInstance>>),
    /// A neural network (from `import ai`). Holds only numbers, so it can't form
    /// reference cycles and the GC never needs to trace it.
    Network(Rc<RefCell<crate::nn::Net>>),
    /// Game-kit objects (`import gamekit`). Bodies/hitboxes point at each other
    /// with `Rc` but not through `Value`, so the cycle GC does not need to
    /// clear them — they die when nothing in the program holds them.
    Body(Rc<RefCell<crate::gamekit::Body>>),
    Hitbox(Rc<RefCell<crate::gamekit::Hitbox>>),
    PhysicsWorld(Rc<RefCell<crate::gamekit::World>>),
    Tilemap(Rc<RefCell<crate::gamekit::Tilemap>>),
    /// The `web` helper from `import web` (`web.get`, `web.get_json`, …).
    WebModule,
    Function(Rc<FunctionObj>),
    /// A method already bound to its receiver, produced by `value.method`.
    BoundMethod { receiver: Box<Value>, func: Rc<FunctionObj> },
    Builtin(Builtin),
}

/// A user-defined function or method, capturing the environment it was defined
/// in (for closures).
pub struct FunctionObj {
    pub decl: Rc<FunctionDecl>,
    pub closure: Env,
}

/// The definition of a `class` (struct): its declared fields plus its methods.
pub struct ClassDef {
    pub name: String,
    pub fields: Vec<crate::ast::Field>,
    pub methods: HashMap<String, Rc<FunctionObj>>,
}

/// A live instance of a class.
pub struct ClassInstance {
    pub def: Rc<ClassDef>,
    pub fields: HashMap<String, Value>,
}

/// Names of the built-in free functions. Kept as an enum so dispatch is a
/// simple match rather than string comparison at every call.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Builtin {
    Print,
    Input,
    Exit,
    ToText,
    ToNumber,
    Min,
    Greatest,
    Abs,
    Sqrt,
    Floor,
    Ceil,
    Round,
    Length,
    RandomBetween,
    // Math
    Pow,
    Clamp,
    Sin,
    Cos,
    Tan,
    // File I/O
    ReadFile,
    WriteFile,
    AppendFile,
    FileExists,
    ReadCsv,
    LoadDataset,
    // Saved data (structured save/load)
    Save,
    Load,
    HasSave,
    // Time
    Now,
    Clock,
    // Graphics (only usable inside a game block)
    ClearScreen,
    DrawCircle,
    DrawRectangle,
    DrawLine,
    DrawText,
    Rgb,
    Rgba,
    ScreenWidth,
    ScreenHeight,
    // Input
    KeyDown,
    KeyPressed,
    MouseX,
    MouseY,
    MouseDown,
    MousePressed,
    // Sprites & sound / music
    LoadSprite,
    DrawSprite,
    DrawSpriteScaled,
    DrawSpriteRotated,
    SpriteWidth,
    SpriteHeight,
    LoadSound,
    PlaySound,
    StopSound,
    SetSoundVolume,
    SetSoundPitch,
    SetSoundPan,
    LoadMusic,
    PlayMusic,
    StopMusic,
    SetMusicVolume,
    SetMusicPitch,
    SetMusicPan,
    FadeMusic,
    LoadFont,
    // Timers
    After,
    Every,
    // AI (only usable after `import ai`)
    NeuralNetwork,
    LoadNetwork,
    Population,
    Evolve,
    BestOf,
    // Game kit (only usable after `import gamekit`)
    PhysicsWorld,
    Body,
    Hitbox,
    Overlaps,
    Pressed,
    DrawBody,
    DrawHitbox,
    DrawHitboxes,
    Tilemap,
    TileAt,
    DrawTilemap,
    // Web (only usable after `import web`)
    WebGetJson,
    WebPostJson,
    ParseJson,
    ToJson,
}

impl Builtin {
    pub fn name(self) -> &'static str {
        match self {
            Builtin::Print => "print",
            Builtin::Input => "input",
            Builtin::Exit => "exit",
            Builtin::ToText => "to_text",
            Builtin::ToNumber => "to_number",
            Builtin::Min => "min",
            Builtin::Greatest => "greatest",
            Builtin::Abs => "abs",
            Builtin::Sqrt => "sqrt",
            Builtin::Floor => "floor",
            Builtin::Ceil => "ceil",
            Builtin::Round => "round",
            Builtin::Length => "length",
            Builtin::RandomBetween => "random_between",
            Builtin::Pow => "pow",
            Builtin::Clamp => "clamp",
            Builtin::Sin => "sin",
            Builtin::Cos => "cos",
            Builtin::Tan => "tan",
            Builtin::ReadFile => "read_file",
            Builtin::WriteFile => "write_file",
            Builtin::AppendFile => "append_file",
            Builtin::FileExists => "file_exists",
            Builtin::ReadCsv => "read_csv",
            Builtin::LoadDataset => "load_dataset",
            Builtin::Save => "save",
            Builtin::Load => "load",
            Builtin::HasSave => "has_save",
            Builtin::Now => "now",
            Builtin::Clock => "clock",
            Builtin::ClearScreen => "clear_screen",
            Builtin::DrawCircle => "draw_circle",
            Builtin::DrawRectangle => "draw_rectangle",
            Builtin::DrawLine => "draw_line",
            Builtin::DrawText => "draw_text",
            Builtin::Rgb => "rgb",
            Builtin::Rgba => "rgba",
            Builtin::ScreenWidth => "screen_width",
            Builtin::ScreenHeight => "screen_height",
            Builtin::KeyDown => "key_down",
            Builtin::KeyPressed => "key_pressed",
            Builtin::MouseX => "mouse_x",
            Builtin::MouseY => "mouse_y",
            Builtin::MouseDown => "mouse_down",
            Builtin::MousePressed => "mouse_pressed",
            Builtin::LoadSprite => "load_sprite",
            Builtin::DrawSprite => "draw_sprite",
            Builtin::DrawSpriteScaled => "draw_sprite_scaled",
            Builtin::DrawSpriteRotated => "draw_sprite_rotated",
            Builtin::SpriteWidth => "sprite_width",
            Builtin::SpriteHeight => "sprite_height",
            Builtin::LoadSound => "load_sound",
            Builtin::PlaySound => "play_sound",
            Builtin::StopSound => "stop_sound",
            Builtin::SetSoundVolume => "set_sound_volume",
            Builtin::SetSoundPitch => "set_sound_pitch",
            Builtin::SetSoundPan => "set_sound_pan",
            Builtin::LoadMusic => "load_music",
            Builtin::PlayMusic => "play_music",
            Builtin::StopMusic => "stop_music",
            Builtin::SetMusicVolume => "set_music_volume",
            Builtin::SetMusicPitch => "set_music_pitch",
            Builtin::SetMusicPan => "set_music_pan",
            Builtin::FadeMusic => "fade_music",
            Builtin::LoadFont => "load_font",
            Builtin::After => "after",
            Builtin::Every => "every",
            Builtin::NeuralNetwork => "neural_network",
            Builtin::LoadNetwork => "load_network",
            Builtin::Population => "population",
            Builtin::Evolve => "evolve",
            Builtin::BestOf => "best_of",
            Builtin::PhysicsWorld => "physics_world",
            Builtin::Body => "body",
            Builtin::Hitbox => "hitbox",
            Builtin::Overlaps => "overlaps",
            Builtin::Pressed => "pressed",
            Builtin::DrawBody => "draw_body",
            Builtin::DrawHitbox => "draw_hitbox",
            Builtin::DrawHitboxes => "draw_hitboxes",
            Builtin::Tilemap => "tilemap",
            Builtin::TileAt => "tile_at",
            Builtin::DrawTilemap => "draw_tilemap",
            Builtin::WebGetJson => "get_json",
            Builtin::WebPostJson => "post_json",
            Builtin::ParseJson => "parse_json",
            Builtin::ToJson => "to_json",
        }
    }

    /// Whether this builtin belongs to the `ai` module (`import ai`).
    pub fn is_ai(self) -> bool {
        matches!(
            self,
            Builtin::NeuralNetwork | Builtin::LoadNetwork | Builtin::Population | Builtin::Evolve | Builtin::BestOf
        )
    }

    /// Whether this builtin belongs to the `gamekit` module (`import gamekit`).
    pub fn is_gamekit(self) -> bool {
        matches!(
            self,
            Builtin::PhysicsWorld
                | Builtin::Body
                | Builtin::Hitbox
                | Builtin::Overlaps
                | Builtin::Pressed
                | Builtin::DrawBody
                | Builtin::DrawHitbox
                | Builtin::DrawHitboxes
                | Builtin::Tilemap
                | Builtin::TileAt
                | Builtin::DrawTilemap
        )
    }

    /// Whether this builtin belongs to the `web` module (`import web`).
    pub fn is_web(self) -> bool {
        matches!(
            self,
            Builtin::WebGetJson | Builtin::WebPostJson | Builtin::ParseJson | Builtin::ToJson
        )
    }

    /// Whether this builtin belongs to the `math` module (only available after
    /// `import math`).
    pub fn is_math(self) -> bool {
        use Builtin::*;
        matches!(
            self,
            Min | Greatest | Abs | Sqrt | Floor | Ceil | Round | Pow | Clamp | Sin | Cos | Tan
                | RandomBetween
        )
    }

    pub fn from_name(name: &str) -> Option<Builtin> {
        Some(match name {
            "print" => Builtin::Print,
            "input" => Builtin::Input,
            "exit" => Builtin::Exit,
            "to_text" => Builtin::ToText,
            "to_number" => Builtin::ToNumber,
            "min" => Builtin::Min,
            "greatest" => Builtin::Greatest,
            "abs" => Builtin::Abs,
            "sqrt" => Builtin::Sqrt,
            "floor" => Builtin::Floor,
            "ceil" => Builtin::Ceil,
            "round" => Builtin::Round,
            "length" => Builtin::Length,
            "random_between" => Builtin::RandomBetween,
            "pow" => Builtin::Pow,
            "clamp" => Builtin::Clamp,
            "sin" => Builtin::Sin,
            "cos" => Builtin::Cos,
            "tan" => Builtin::Tan,
            "read_file" => Builtin::ReadFile,
            "write_file" => Builtin::WriteFile,
            "append_file" => Builtin::AppendFile,
            "file_exists" => Builtin::FileExists,
            "read_csv" => Builtin::ReadCsv,
            "load_dataset" => Builtin::LoadDataset,
            "save" => Builtin::Save,
            "load" => Builtin::Load,
            "has_save" => Builtin::HasSave,
            "now" => Builtin::Now,
            "clock" => Builtin::Clock,
            "clear_screen" => Builtin::ClearScreen,
            "draw_circle" => Builtin::DrawCircle,
            "draw_rectangle" => Builtin::DrawRectangle,
            "draw_line" => Builtin::DrawLine,
            "draw_text" => Builtin::DrawText,
            "rgb" => Builtin::Rgb,
            "rgba" => Builtin::Rgba,
            "screen_width" => Builtin::ScreenWidth,
            "screen_height" => Builtin::ScreenHeight,
            "key_down" => Builtin::KeyDown,
            "key_pressed" => Builtin::KeyPressed,
            "mouse_x" => Builtin::MouseX,
            "mouse_y" => Builtin::MouseY,
            "mouse_down" => Builtin::MouseDown,
            "mouse_pressed" => Builtin::MousePressed,
            "load_sprite" => Builtin::LoadSprite,
            "draw_sprite" => Builtin::DrawSprite,
            "draw_sprite_scaled" => Builtin::DrawSpriteScaled,
            "draw_sprite_rotated" => Builtin::DrawSpriteRotated,
            "sprite_width" => Builtin::SpriteWidth,
            "sprite_height" => Builtin::SpriteHeight,
            "load_sound" => Builtin::LoadSound,
            "play_sound" => Builtin::PlaySound,
            "stop_sound" => Builtin::StopSound,
            "set_sound_volume" => Builtin::SetSoundVolume,
            "set_sound_pitch" => Builtin::SetSoundPitch,
            "set_sound_pan" => Builtin::SetSoundPan,
            "load_music" => Builtin::LoadMusic,
            "play_music" => Builtin::PlayMusic,
            "stop_music" => Builtin::StopMusic,
            "set_music_volume" => Builtin::SetMusicVolume,
            "set_music_pitch" => Builtin::SetMusicPitch,
            "set_music_pan" => Builtin::SetMusicPan,
            "fade_music" => Builtin::FadeMusic,
            "load_font" => Builtin::LoadFont,
            "after" => Builtin::After,
            "every" => Builtin::Every,
            "neural_network" => Builtin::NeuralNetwork,
            "load_network" => Builtin::LoadNetwork,
            "population" => Builtin::Population,
            "evolve" => Builtin::Evolve,
            "best_of" => Builtin::BestOf,
            "physics_world" => Builtin::PhysicsWorld,
            "body" => Builtin::Body,
            "hitbox" => Builtin::Hitbox,
            "overlaps" => Builtin::Overlaps,
            "pressed" => Builtin::Pressed,
            "draw_body" => Builtin::DrawBody,
            "draw_hitbox" => Builtin::DrawHitbox,
            "draw_hitboxes" => Builtin::DrawHitboxes,
            "tilemap" => Builtin::Tilemap,
            "tile_at" => Builtin::TileAt,
            "draw_tilemap" => Builtin::DrawTilemap,
            "get_json" => Builtin::WebGetJson,
            "post_json" => Builtin::WebPostJson,
            "parse_json" => Builtin::ParseJson,
            "to_json" => Builtin::ToJson,
            _ => return None,
        })
    }
}

impl Value {
    pub fn text(s: impl Into<String>) -> Value {
        Value::Text(Rc::new(s.into()))
    }

    /// The name of a value's type, for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Number(_) => "Number",
            Value::Text(_) => "Text",
            Value::Bool(_) => "Boolean",
            Value::Nothing => "nothing",
            Value::List(_) => "list",
            Value::Dictionary(_) => "dictionary",
            Value::Class(t) => {
                // Leak-free static name isn't possible for a dynamic string,
                // so callers that need the specific class name use display.
                let _ = t;
                "class"
            }
            Value::Network(_) => "neural network",
            Value::Body(_) => "body",
            Value::Hitbox(_) => "hitbox",
            Value::PhysicsWorld(_) => "physics world",
            Value::Tilemap(_) => "tilemap",
            Value::WebModule => "web",
            Value::Function(_) | Value::BoundMethod { .. } | Value::Builtin(_) => "function",
        }
    }

    /// How the value prints (in `print(...)` and string interpolation).
    pub fn display(&self) -> String {
        match self {
            Value::Number(n) => format_number(*n),
            Value::Text(s) => (**s).clone(),
            Value::Bool(b) => if *b { "true".into() } else { "false".into() },
            Value::Nothing => "nothing".into(),
            Value::List(items) => {
                let parts: Vec<String> = items.borrow().iter().map(|v| v.display_inner()).collect();
                format!("[{}]", parts.join(", "))
            }
            Value::Dictionary(m) => {
                let m = m.borrow();
                let parts: Vec<String> = m
                    .entries
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k.display_inner(), v.display_inner()))
                    .collect();
                format!("dictionary {{{}}}", parts.join(", "))
            }
            Value::Class(t) => {
                let t = t.borrow();
                let parts: Vec<String> = t
                    .def
                    .fields
                    .iter()
                    .map(|f| {
                        let v = t.fields.get(&f.name).cloned().unwrap_or(Value::Nothing);
                        format!("{}: {}", f.name, v.display_inner())
                    })
                    .collect();
                format!("{} {{{}}}", t.def.name, parts.join(", "))
            }
            Value::Network(n) => {
                let n = n.borrow();
                format!("<neural network {} → {}>", n.inputs(), n.outputs())
            }
            Value::Body(b) => {
                let b = b.borrow();
                format!("<body at ({}, {})>", format_number(b.x), format_number(b.y))
            }
            Value::Hitbox(h) => {
                let h = h.borrow();
                format!("<hitbox \"{}\">", h.kind)
            }
            Value::PhysicsWorld(w) => {
                let w = w.borrow();
                format!("<physics world gravity={}>", format_number(w.gravity))
            }
            Value::Tilemap(m) => {
                let m = m.borrow();
                format!(
                    "<tilemap {}x{} cell={}>",
                    m.width(),
                    m.height(),
                    format_number(m.cell_size)
                )
            }
            Value::WebModule => "<web>".into(),
            Value::Function(f) => format!("<function {}>", f.decl.name),
            Value::BoundMethod { func, .. } => format!("<method {}>", func.decl.name),
            Value::Builtin(b) => format!("<builtin {}>", b.name()),
        }
    }

    /// Like `display`, but quotes text so nested values are unambiguous.
    fn display_inner(&self) -> String {
        match self {
            Value::Text(s) => format!("\"{}\"", s),
            _ => self.display(),
        }
    }

}

/// Format an f64 the way PlainText shows numbers: integers without a trailing
/// `.0`, fractions as-is.
pub fn format_number(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        // Trim to a reasonable precision without trailing zeros.
        let s = format!("{}", n);
        s
    }
}

/// A key usable in a dictionary. Numbers are keyed on their bit pattern so equality is
/// exact.
#[derive(Clone)]
pub enum MapKey {
    Text(String),
    Number(f64),
    Bool(bool),
}

impl MapKey {
    fn eq_key(&self, other: &MapKey) -> bool {
        match (self, other) {
            (MapKey::Text(a), MapKey::Text(b)) => a == b,
            (MapKey::Number(a), MapKey::Number(b)) => a.to_bits() == b.to_bits(),
            (MapKey::Bool(a), MapKey::Bool(b)) => a == b,
            _ => false,
        }
    }

    pub fn to_value(&self) -> Value {
        match self {
            MapKey::Text(s) => Value::text(s.clone()),
            MapKey::Number(n) => Value::Number(*n),
            MapKey::Bool(b) => Value::Bool(*b),
        }
    }

    fn display_inner(&self) -> String {
        self.to_value().display_inner()
    }
}

/// Insertion-ordered dictionary. Linear lookup is fine for the small dictionaries typical of
/// v1 programs; this can be swapped for a hash map later without changing the
/// language.
#[derive(Default)]
pub struct PtMap {
    pub entries: Vec<(MapKey, Value)>,
}

impl PtMap {
    pub fn new() -> PtMap {
        PtMap { entries: Vec::new() }
    }

    pub fn get(&self, key: &MapKey) -> Option<Value> {
        self.entries.iter().find(|(k, _)| k.eq_key(key)).map(|(_, v)| v.clone())
    }

    pub fn set(&mut self, key: MapKey, value: Value) {
        if let Some(slot) = self.entries.iter_mut().find(|(k, _)| k.eq_key(&key)) {
            slot.1 = value;
        } else {
            self.entries.push((key, value));
        }
    }

    pub fn has(&self, key: &MapKey) -> bool {
        self.entries.iter().any(|(k, _)| k.eq_key(key))
    }

    /// Remove a key, returning its value if it was present.
    pub fn remove(&mut self, key: &MapKey) -> Option<Value> {
        if let Some(pos) = self.entries.iter().position(|(k, _)| k.eq_key(key)) {
            Some(self.entries.remove(pos).1)
        } else {
            None
        }
    }

    /// Drop all entries. Used by the garbage collector to break a reference
    /// cycle this dictionary is part of.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// A lexical scope. Only function calls (and the top-level module) create one;
/// `if`/`while`/`for` blocks share their enclosing function's scope.
///
/// Variable lookup is the interpreter's hottest operation (a tight loop hits it
/// several times per iteration), so this map uses `FxHashMap` — the same fast,
/// non-cryptographic hasher rustc itself uses — instead of the standard
/// library's SipHash, which is much slower for the tiny string keys (`i`, `n`)
/// typical of program variables.
pub struct Scope {
    vars: FxHashMap<String, Value>,
    parent: Option<Env>,
}

impl Scope {
    pub fn new_global() -> Env {
        Rc::new(RefCell::new(Scope { vars: FxHashMap::default(), parent: None }))
    }

    pub fn new_child(parent: &Env) -> Env {
        Rc::new(RefCell::new(Scope { vars: FxHashMap::default(), parent: Some(parent.clone()) }))
    }
}

/// Look up a variable, walking the parent chain.
pub fn env_get(env: &Env, name: &str) -> Option<Value> {
    let scope = env.borrow();
    if let Some(v) = scope.vars.get(name) {
        return Some(v.clone());
    }
    match &scope.parent {
        Some(p) => env_get(p, name),
        None => None,
    }
}

/// Assign to a variable: reassign the nearest existing binding in the scope
/// chain, or declare it in `env` if it doesn't exist anywhere (this is how
/// first-write-declares works).
pub fn env_set(env: &Env, name: &str, value: Value) {
    if assign_existing(env, name, &value) {
        return;
    }
    env.borrow_mut().vars.insert(name.to_string(), value);
}

fn assign_existing(env: &Env, name: &str, value: &Value) -> bool {
    let mut scope = env.borrow_mut();
    // `get_mut` finds and overwrites in a single hash lookup — the old
    // `contains_key` + `insert(name.to_string(), …)` hashed twice and allocated
    // a fresh String key on every reassignment (this runs millions of times in
    // a tight loop, so it mattered).
    if let Some(slot) = scope.vars.get_mut(name) {
        *slot = value.clone();
        return true;
    }
    match &scope.parent {
        Some(p) => {
            let p = p.clone();
            drop(scope);
            assign_existing(&p, name, value)
        }
        None => false,
    }
}

/// Declare a variable directly in `env` (used for parameters and loop vars,
/// which always live in the current scope).
pub fn env_declare(env: &Env, name: &str, value: Value) {
    env.borrow_mut().vars.insert(name.to_string(), value);
}

/// A snapshot of a scope for the garbage collector to trace: the values it
/// holds and its parent scope. (The GC marks through scopes but never clears
/// them, so it only needs to read.)
pub fn scope_snapshot(env: &Env) -> (Vec<Value>, Option<Env>) {
    let scope = env.borrow();
    let values = scope.vars.values().cloned().collect();
    (values, scope.parent.clone())
}
