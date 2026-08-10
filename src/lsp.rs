//! Language server (`plaintext lsp`) over stdio.
//!
//! Speaks LSP: diagnostics (same checker as `plaintext check`), hover, go to
//! definition (current file), and basic completions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::ast::{Expr, Program, Stmt};
use crate::checker::Checker;
use crate::diagnostics::Diagnostic;
use crate::load::{load_program_with_source, parse_file_diag};
use crate::token::Span;
use crate::value::Builtin;

/// Run the language server until the client disconnects.
pub fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        let (service, socket) = LspService::new(|client| Backend {
            client,
            docs: Arc::new(RwLock::new(HashMap::new())),
        });
        Server::new(stdin, stdout, socket).serve(service).await;
        Ok(())
    })
}

#[derive(Clone)]
struct DocState {
    path: PathBuf,
    text: String,
}

struct Backend {
    client: Client,
    docs: Arc<RwLock<HashMap<Url, DocState>>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> LspResult<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "plaintext".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![".".into()]),
                    ..Default::default()
                }),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "PlainText language server ready")
            .await;
    }

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let path = uri_to_path(&uri);
        {
            let mut docs = self.docs.write().await;
            docs.insert(
                uri.clone(),
                DocState {
                    path,
                    text: params.text_document.text,
                },
            );
        }
        self.publish_diags(&uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(change) = params.content_changes.into_iter().last() {
            let mut docs = self.docs.write().await;
            if let Some(doc) = docs.get_mut(&uri) {
                doc.text = change.text;
            }
        }
        self.publish_diags(&uri).await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.publish_diags(&params.text_document.uri).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.docs.write().await.remove(&uri);
        self.client.publish_diagnostics(uri, Vec::new(), None).await;
    }

    async fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let docs = self.docs.read().await;
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };
        let Some(word) = word_at(&doc.text, pos) else {
            return Ok(None);
        };

        if let Some(info) = builtin_hover(&word) {
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: info,
                }),
                range: None,
            }));
        }

        if let Some(info) = keyword_hover(&word) {
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: info,
                }),
                range: None,
            }));
        }

        if let Ok(loaded) = load_program_with_source(&doc.path, &doc.text) {
            if let Some(desc) = describe_symbol(&loaded.program, &word) {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: desc,
                    }),
                    range: None,
                }));
            }
        }

        Ok(None)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let docs = self.docs.read().await;
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };
        let Some(word) = word_at(&doc.text, pos) else {
            return Ok(None);
        };

        let Ok(loaded) = load_program_with_source(&doc.path, &doc.text) else {
            return Ok(None);
        };

        if let Some(span) = find_definition(&loaded.program, &word, 0) {
            let range = span_to_range(span, word.len());
            return Ok(Some(GotoDefinitionResponse::Scalar(Location { uri, range })));
        }
        Ok(None)
    }

    async fn completion(&self, params: CompletionParams) -> LspResult<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let docs = self.docs.read().await;
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };

        let mut items: Vec<CompletionItem> = Vec::new();

        for kw in KEYWORDS {
            items.push(CompletionItem {
                label: (*kw).into(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("keyword".into()),
                ..Default::default()
            });
        }

        for name in BUILTIN_NAMES {
            items.push(CompletionItem {
                label: (*name).into(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some("builtin".into()),
                documentation: builtin_hover(name).map(|v| {
                    Documentation::MarkupContent(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: v,
                    })
                }),
                ..Default::default()
            });
        }

        if let Ok(loaded) = load_program_with_source(&doc.path, &doc.text) {
            for name in collect_top_names(&loaded.program) {
                items.push(CompletionItem {
                    label: name.clone(),
                    kind: Some(CompletionItemKind::VARIABLE),
                    detail: Some("in this file".into()),
                    ..Default::default()
                });
            }
        }

        Ok(Some(CompletionResponse::Array(items)))
    }
}

impl Backend {
    async fn publish_diags(&self, uri: &Url) {
        let (path, text) = {
            let docs = self.docs.read().await;
            match docs.get(uri) {
                Some(d) => (d.path.clone(), d.text.clone()),
                None => return,
            }
        };

        let diags = analyze(&path, &text);
        self.client
            .publish_diagnostics(uri.clone(), diags, None)
            .await;
    }
}

fn analyze(path: &Path, text: &str) -> Vec<tower_lsp::lsp_types::Diagnostic> {
    let shown = path.display().to_string();

    match load_program_with_source(path, text) {
        Ok(loaded) => {
            let errors = Checker::new().check(&loaded.program);
            errors
                .into_iter()
                .filter(|d| d.span.file == 0)
                .map(|d| diag_to_lsp(&d, text))
                .collect()
        }
        Err(msg) => match parse_file_diag(&shown, text, 0) {
            Ok(_) => vec![tower_lsp::lsp_types::Diagnostic {
                range: Range::new(Position::new(0, 0), Position::new(0, 1)),
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("plaintext".into()),
                message: msg,
                ..Default::default()
            }],
            Err(d) => vec![diag_to_lsp(&d, text)],
        },
    }
}

fn diag_to_lsp(d: &Diagnostic, source: &str) -> tower_lsp::lsp_types::Diagnostic {
    let mut message = d.message.clone();
    if let Some(hint) = &d.hint {
        message.push_str("\nHint: ");
        message.push_str(hint);
    }
    let start = Position::new(
        d.span.line.saturating_sub(1) as u32,
        d.span.col.saturating_sub(1) as u32,
    );
    let line = source
        .lines()
        .nth(d.span.line.saturating_sub(1))
        .unwrap_or("");
    let end_col = ((d.span.col.saturating_sub(1)) + 1).min(line.chars().count().max(1));
    let end = Position::new(start.line, end_col as u32);
    tower_lsp::lsp_types::Diagnostic {
        range: Range::new(start, end),
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("plaintext".into()),
        message,
        ..Default::default()
    }
}

fn uri_to_path(uri: &Url) -> PathBuf {
    uri.to_file_path()
        .unwrap_or_else(|_| PathBuf::from(uri.path()))
}

fn word_at(text: &str, pos: Position) -> Option<String> {
    let line = text.lines().nth(pos.line as usize)?;
    let col = pos.character as usize;
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return None;
    }
    let mut i = col.min(chars.len().saturating_sub(1));
    if i < chars.len() && !is_ident_char(chars[i]) {
        if i > 0 && is_ident_char(chars[i - 1]) {
            i -= 1;
        } else {
            return None;
        }
    }
    let mut start = i;
    while start > 0 && is_ident_char(chars[start - 1]) {
        start -= 1;
    }
    let mut end = i + 1;
    while end < chars.len() && is_ident_char(chars[end]) {
        end += 1;
    }
    let word: String = chars[start..end].iter().collect();
    if word.is_empty() {
        None
    } else {
        Some(word)
    }
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn span_to_range(span: Span, len: usize) -> Range {
    let start = Position::new(
        span.line.saturating_sub(1) as u32,
        span.col.saturating_sub(1) as u32,
    );
    let end = Position::new(start.line, start.character + len as u32);
    Range::new(start, end)
}

fn find_definition(program: &Program, name: &str, file: u16) -> Option<Span> {
    for stmt in &program.statements {
        match stmt {
            Stmt::Function(f) if f.name == name && f.span.file == file => return Some(f.span),
            Stmt::Class(c) if c.name == name && c.span.file == file => return Some(c.span),
            Stmt::Class(c) if c.span.file == file => {
                for m in &c.methods {
                    if m.name == name {
                        return Some(m.span);
                    }
                }
            }
            Stmt::Assign {
                target: Expr::Ident(n, _),
                span,
                ..
            } if n == name && span.file == file => return Some(*span),
            _ => {}
        }
    }
    for stmt in &program.statements {
        if let Stmt::Function(f) = stmt {
            if f.span.file != file {
                continue;
            }
            for p in &f.params {
                if p.name == name {
                    return Some(p.span);
                }
            }
            if let Some(s) = find_in_block(&f.body, name) {
                return Some(s);
            }
        }
        if let Stmt::Class(c) = stmt {
            if c.span.file != file {
                continue;
            }
            for m in &c.methods {
                for p in &m.params {
                    if p.name == name {
                        return Some(p.span);
                    }
                }
                if let Some(s) = find_in_block(&m.body, name) {
                    return Some(s);
                }
            }
        }
    }
    None
}

fn find_in_block(body: &[Stmt], name: &str) -> Option<Span> {
    for stmt in body {
        match stmt {
            Stmt::Assign {
                target: Expr::Ident(n, _),
                span,
                ..
            } if n == name => return Some(*span),
            Stmt::If {
                branches,
                else_body,
                ..
            } => {
                for (_, b) in branches {
                    if let Some(s) = find_in_block(b, name) {
                        return Some(s);
                    }
                }
                if let Some(e) = else_body {
                    if let Some(s) = find_in_block(e, name) {
                        return Some(s);
                    }
                }
            }
            Stmt::While { body, .. }
            | Stmt::Loop { body, .. }
            | Stmt::Repeat { body, .. }
            | Stmt::ForEvery { body, .. } => {
                if let Some(s) = find_in_block(body, name) {
                    return Some(s);
                }
            }
            Stmt::Function(f) => {
                for p in &f.params {
                    if p.name == name {
                        return Some(p.span);
                    }
                }
                if let Some(s) = find_in_block(&f.body, name) {
                    return Some(s);
                }
            }
            _ => {}
        }
    }
    None
}

fn describe_symbol(program: &Program, name: &str) -> Option<String> {
    for stmt in &program.statements {
        match stmt {
            Stmt::Function(f) if f.name == name => {
                let params: Vec<&str> = f.params.iter().map(|p| p.name.as_str()).collect();
                return Some(format!("**function** `{}({})`", name, params.join(", ")));
            }
            Stmt::Class(c) if c.name == name => {
                return Some(format!("**class** `{}`", name));
            }
            Stmt::Assign {
                target: Expr::Ident(n, _),
                value,
                ..
            } if n == name => {
                let kind = expr_kind_hint(value);
                return Some(format!("**variable** `{}`{}", name, kind));
            }
            Stmt::Class(c) => {
                for m in &c.methods {
                    if m.name == name {
                        return Some(format!("**method** `{}.{}`", c.name, name));
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn expr_kind_hint(e: &Expr) -> String {
    match e {
        Expr::Number(_, _) => " — Number".into(),
        Expr::Text(_, _) => " — Text".into(),
        Expr::Bool(_, _) => " — Boolean".into(),
        Expr::ListLit { .. } => " — list".into(),
        Expr::DictionaryLit { .. } => " — dictionary".into(),
        Expr::Function { .. } => " — function".into(),
        Expr::ClassLit { name, .. } => format!(" — {}", name),
        _ => String::new(),
    }
}

fn collect_top_names(program: &Program) -> Vec<String> {
    let mut names = Vec::new();
    for stmt in &program.statements {
        match stmt {
            Stmt::Function(f) => names.push(f.name.clone()),
            Stmt::Class(c) => {
                names.push(c.name.clone());
                for m in &c.methods {
                    names.push(m.name.clone());
                }
            }
            Stmt::Assign {
                target: Expr::Ident(n, _),
                ..
            } => names.push(n.clone()),
            Stmt::Import { module, .. } => names.push(module.clone()),
            _ => {}
        }
    }
    names.sort();
    names.dedup();
    names
}

fn keyword_hover(word: &str) -> Option<String> {
    let text = match word {
        "make" => "Start a function: `make function called name(...) { ... }`",
        "function" => "A callable block of code.",
        "class" => "A custom type with fields and methods.",
        "if" | "else" => "Branch on a true/false condition.",
        "while" | "for" | "repeat" | "loop" => "Loop forms — see the cheatsheet.",
        "return" => "Leave a function with a value (or nothing).",
        "import" => "Bring in a module (`math`, `ai`, `gamekit`, `web`) or another `.pt` file.",
        "game" => "Open a game window with `on update` / `on draw`.",
        "window" => "Open a desktop UI window.",
        "nothing" => "The empty value (also used with `Text?` optionals).",
        "true" | "false" => "Boolean values.",
        "and" | "or" | "not" | "is" => "Wordy boolean / comparison helpers.",
        _ => return None,
    };
    Some(format!("**`{}`** — {}", word, text))
}

fn builtin_hover(name: &str) -> Option<String> {
    let text = match name {
        "print" => "Write values to the console.",
        "input" => "Read a line of text from the console.",
        "exit" => "Stop the program (`exit()` or `exit(code)`).",
        "to_text" | "to_number" => "Convert between text and numbers.",
        "length" => "Length of a list or text.",
        "min" | "greatest" | "abs" | "sqrt" | "floor" | "ceil" | "round" | "pow" | "clamp"
        | "sin" | "cos" | "tan" | "random_between" => {
            "Math helper — needs `import math` (except where noted in docs)."
        }
        "read_file" | "write_file" | "append_file" | "file_exists" => "File tools.",
        "read_csv" | "load_dataset" => "Load numeric / ML datasets from disk.",
        "now" | "clock" => "Time helpers.",
        "after" | "every" => "Run a function later / on an interval.",
        "neural_network" | "load_network" | "population" | "evolve" | "best_of" => {
            "AI tools — needs `import ai`."
        }
        "physics_world" | "body" | "hitbox" | "overlaps" | "pressed" | "draw_body"
        | "draw_hitbox" | "draw_hitboxes" | "tilemap" | "tile_at" | "draw_tilemap" => {
            "Game kit — needs `import gamekit`."
        }
        "get_json" | "post_json" | "parse_json" | "to_json" => {
            "Web / JSON tools — needs `import web` (or use `web.get` / `web.get_json`)."
        }
        "clear_screen" | "draw_circle" | "draw_rectangle" | "draw_line" | "draw_text"
        | "draw_text_screen" | "draw_rectangle_screen" | "rgb" | "rgba" | "screen_width"
        | "screen_height" | "key_down" | "key_pressed" | "mouse_x" | "mouse_y" | "mouse_down"
        | "mouse_pressed" | "load_sprite" | "load_sprite_sheet" | "draw_sprite"
        | "draw_sprite_scaled" | "draw_sprite_rotated" | "draw_frame" | "draw_frame_scaled"
        | "sprite_width" | "sprite_height" | "frame_count" | "set_camera" | "center_camera"
        | "camera_bounds" | "camera_x" | "camera_y" | "burst" | "load_sound" | "play_sound"
        | "stop_sound"
        | "set_sound_volume" | "set_sound_pitch" | "set_sound_pan" | "load_music" | "play_music"
        | "stop_music" | "set_music_volume" | "set_music_pitch" | "set_music_pan" | "fade_music"
        | "load_font" => {
            "Game / drawing helper — use inside a `game` block."
        }
        _ => {
            if Builtin::from_name(name).is_some() {
                "Built-in function."
            } else {
                return None;
            }
        }
    };
    Some(format!("**`{}`** — {}", name, text))
}

const KEYWORDS: &[&str] = &[
    "make", "function", "called", "class", "if", "else", "while", "for", "every", "in", "repeat",
    "times", "loop", "return", "break", "continue", "and", "or", "not", "is", "nothing", "true",
    "false", "import", "game", "window", "on", "try", "otherwise", "list", "dictionary", "of",
    "to", "self", "increase", "decrease",
];

const BUILTIN_NAMES: &[&str] = &[
    "print", "input", "exit", "to_text", "to_number", "length", "min", "greatest", "abs", "sqrt",
    "floor", "ceil", "round", "random_between", "pow", "clamp", "sin", "cos", "tan", "read_file",
    "write_file", "append_file", "file_exists", "read_csv", "load_dataset", "now", "clock", "after",
    "every", "clear_screen", "draw_circle", "draw_rectangle", "draw_line", "draw_text", "rgb",
    "rgba", "screen_width", "screen_height", "key_down", "key_pressed", "mouse_x", "mouse_y",
    "mouse_down", "mouse_pressed", "load_sprite", "load_sprite_sheet", "draw_sprite",
    "draw_sprite_scaled", "draw_sprite_rotated", "draw_frame", "draw_frame_scaled",
    "sprite_width", "sprite_height", "frame_count", "set_camera", "center_camera",
    "camera_bounds", "camera_x", "camera_y", "burst", "draw_text_screen",
    "draw_rectangle_screen", "load_sound", "play_sound",
    "stop_sound", "set_sound_volume", "set_sound_pitch", "set_sound_pan", "load_music",
    "play_music", "stop_music", "set_music_volume", "set_music_pitch", "set_music_pan",
    "fade_music", "load_font",
    "neural_network", "load_network", "population", "evolve", "best_of", "physics_world", "body",
    "hitbox", "overlaps", "pressed", "draw_body", "draw_hitbox", "draw_hitboxes", "tilemap",
    "tile_at", "draw_tilemap", "get_json", "post_json", "parse_json", "to_json",
];
