//! Load a PlainText program and its file imports into one merged AST.
//!
//! Shared by the CLI (`run` / `check` / `build`) and the language server.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ast::{Program, Stmt};
use crate::lexer::Lexer;
use crate::parser::Parser;

/// Everything loaded from an entry file: the merged program (dependencies
/// first), the file-id → display-name table, and — for `build` — every source
/// keyed by a stable relative path, plus which key is the entry.
pub struct Loaded {
    pub program: Program,
    pub files: Vec<String>,
    pub sources: Vec<(String, String)>,
    pub entry_key: String,
}

/// Lex + parse one file (stamping its spans with `file_id`), returning either
/// its program or an already-rendered error string pointing at the right file.
pub fn parse_file(path: &str, src: &str, file_id: u16) -> Result<Program, String> {
    let tokens = Lexer::with_file(src, file_id)
        .tokenize()
        .map_err(|d| d.render(path))?;
    Parser::new(tokens)
        .parse_program()
        .map_err(|d| d.render(path))
}

/// Lex + parse, keeping structured diagnostics (for the language server).
pub fn parse_file_diag(
    _path: &str,
    src: &str,
    file_id: u16,
) -> Result<Program, crate::diagnostics::Diagnostic> {
    let tokens = Lexer::with_file(src, file_id).tokenize()?;
    Parser::new(tokens).parse_program()
}

/// Load an entry file and every file it imports, splicing them into one program.
pub fn load_program(entry: &str) -> Result<Loaded, String> {
    let path = Path::new(entry);
    let src = std::fs::read_to_string(path).map_err(|e| {
        format!(
            "Could not read `{}`: {}",
            clean_path(&path.display().to_string()),
            e
        )
    })?;
    load_program_with_source(path, &src)
}

/// Like [`load_program`], but the entry file's text comes from `source`
/// (editor buffer) instead of necessarily matching disk.
pub fn load_program_with_source(entry: &Path, source: &str) -> Result<Loaded, String> {
    let mut ctx = LoadCtx {
        out: Vec::new(),
        files: Vec::new(),
        sources: Vec::new(),
        loaded: Vec::new(),
        in_progress: Vec::new(),
    };
    let entry_key = entry
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("main.pt")
        .to_string();
    load_entry(entry, &entry_key, source, &mut ctx)?;
    Ok(Loaded {
        program: Program {
            statements: ctx.out,
        },
        files: ctx.files,
        sources: ctx.sources,
        entry_key,
    })
}

struct LoadCtx {
    out: Vec<Stmt>,
    files: Vec<String>,
    sources: Vec<(String, String)>,
    loaded: Vec<PathBuf>,
    in_progress: Vec<PathBuf>,
}

fn load_entry(path: &Path, key: &str, source: &str, ctx: &mut LoadCtx) -> Result<(), String> {
    let shown = clean_path(&path.display().to_string());
    let canonical = if path.exists() {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    } else {
        path.to_path_buf()
    };

    let file_id = ctx.files.len() as u16;
    ctx.files.push(shown.clone());
    ctx.sources.push((key.to_string(), source.to_string()));
    let program = parse_file(&shown, source, file_id)?;

    ctx.in_progress.push(canonical.clone());
    let (imports, own) = match split_imports(program.statements, true, &shown) {
        Ok(split) => split,
        Err(e) => {
            ctx.in_progress.pop();
            return Err(e);
        }
    };
    let parent = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    for rel in imports {
        load_disk_file(&parent.join(&rel), &norm_join(&dir_of(key), &rel), false, ctx)?;
    }
    ctx.out.extend(own);
    ctx.in_progress.pop();
    ctx.loaded.push(canonical);
    Ok(())
}

fn load_disk_file(path: &Path, key: &str, is_entry: bool, ctx: &mut LoadCtx) -> Result<(), String> {
    let shown = clean_path(&path.display().to_string());
    let canonical = std::fs::canonicalize(path)
        .map_err(|_| format!("Could not find file `{}`.", shown))?;
    if ctx.loaded.contains(&canonical) || ctx.in_progress.contains(&canonical) {
        return Ok(());
    }
    let src = std::fs::read_to_string(&canonical)
        .map_err(|e| format!("Could not read `{}`: {}", shown, e))?;

    let file_id = ctx.files.len() as u16;
    ctx.files.push(shown.clone());
    ctx.sources.push((key.to_string(), src.clone()));
    let program = parse_file(&shown, &src, file_id)?;

    ctx.in_progress.push(canonical.clone());
    let (imports, own) = match split_imports(program.statements, is_entry, &shown) {
        Ok(split) => split,
        Err(e) => {
            ctx.in_progress.pop();
            return Err(e);
        }
    };
    let parent = canonical.parent().map(Path::to_path_buf).unwrap_or_default();
    for rel in imports {
        load_disk_file(
            &parent.join(&rel),
            &norm_join(&dir_of(key), &rel),
            false,
            ctx,
        )?;
    }
    ctx.out.extend(own);
    ctx.in_progress.pop();
    ctx.loaded.push(canonical);
    Ok(())
}

/// Split a parsed file's statements into its file-imports (relative paths) and
/// everything else, rejecting a `game`/`window` block in a non-entry file.
pub fn split_imports(
    stmts: Vec<Stmt>,
    is_entry: bool,
    shown: &str,
) -> Result<(Vec<String>, Vec<Stmt>), String> {
    let mut imports = Vec::new();
    let mut own = Vec::new();
    for stmt in stmts {
        match stmt {
            Stmt::ImportFile { path: rel, .. } => imports.push(rel),
            Stmt::Game(_) | Stmt::Window(_) if !is_entry => {
                return Err(format!(
                    "Cannot import `{}`: an imported file can't contain a `game` or `window` block.",
                    shown
                ));
            }
            other => own.push(other),
        }
    }
    Ok((imports, own))
}

fn dir_of(key: &str) -> String {
    match key.rfind('/') {
        Some(i) => key[..i].to_string(),
        None => String::new(),
    }
}

pub fn norm_join(dir: &str, rel: &str) -> String {
    let mut parts: Vec<&str> = dir.split('/').filter(|s| !s.is_empty() && *s != ".").collect();
    for seg in rel.split(['/', '\\']) {
        match seg {
            "" | "." => {}
            ".." => {
                if matches!(parts.last(), Some(&p) if p != "..") {
                    parts.pop();
                } else {
                    parts.push("..");
                }
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

pub fn clean_path(p: &str) -> String {
    p.strip_prefix(r"\\?\").unwrap_or(p).to_string()
}

/// Bundle-side twin of disk loading: walk an embedded source map by key.
pub fn load_bundle(entry: String, map: HashMap<String, String>) -> Result<Loaded, String> {
    let mut out = Vec::new();
    let mut files = Vec::new();
    let mut loaded: Vec<String> = Vec::new();
    let mut in_progress: Vec<String> = Vec::new();
    load_key(
        &entry,
        true,
        &map,
        &mut out,
        &mut files,
        &mut loaded,
        &mut in_progress,
    )?;
    Ok(Loaded {
        program: Program { statements: out },
        files,
        sources: Vec::new(),
        entry_key: entry,
    })
}

#[allow(clippy::too_many_arguments)]
fn load_key(
    key: &str,
    is_entry: bool,
    map: &HashMap<String, String>,
    out: &mut Vec<Stmt>,
    files: &mut Vec<String>,
    loaded: &mut Vec<String>,
    in_progress: &mut Vec<String>,
) -> Result<(), String> {
    if loaded.iter().any(|k| k == key) || in_progress.iter().any(|k| k == key) {
        return Ok(());
    }
    let src = map
        .get(key)
        .ok_or_else(|| format!("this bundled app is missing file `{}`.", key))?;
    let file_id = files.len() as u16;
    files.push(key.to_string());
    let program = parse_file(key, src, file_id)?;

    in_progress.push(key.to_string());
    let (imports, own) = match split_imports(program.statements, is_entry, key) {
        Ok(split) => split,
        Err(e) => {
            in_progress.pop();
            return Err(e);
        }
    };
    for rel in imports {
        let child = norm_join(&dir_of(key), &rel);
        load_key(&child, false, map, out, files, loaded, in_progress)?;
    }
    out.extend(own);
    in_progress.pop();
    loaded.push(key.to_string());
    Ok(())
}
