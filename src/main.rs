//! The `plaintext` command-line tool.
//!
//! Subcommands:
//!   plaintext run <file.pt>     parse and execute a program
//!   plaintext check <file.pt>   parse only, report any errors
//!   plaintext build <file.pt>   bundle into a standalone executable
//!   plaintext repl              start an interactive session
//!   plaintext new <name>        scaffold a new project folder
//!   plaintext version           print the version
//!
//! A binary produced by `build` has the program appended to it; on startup we
//! detect that (before parsing argv) and run the embedded program instead.

mod ast;
mod bundle;
mod checker;
mod diagnostics;
mod game;
mod gc;
mod gfx;
mod gpu;
mod interpreter;
mod lexer;
mod nn;
mod parser;
mod token;
mod ui;
mod value;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use checker::Checker;
use diagnostics::Diagnostic;
use interpreter::Interpreter;
use lexer::Lexer;
use parser::Parser;

/// Type-check a program, printing every diagnostic. Returns true if it's clean.
fn type_check(files: &[String], program: &ast::Program) -> bool {
    let errors = Checker::new().check(program);
    if errors.is_empty() {
        return true;
    }
    for d in &errors {
        eprintln!("{}\n", d.render_multi(files));
    }
    let n = errors.len();
    eprintln!("{} error{} found.", n, if n == 1 { "" } else { "s" });
    false
}

fn main() -> ExitCode {
    // If this binary was produced by `plaintext build`, it has a program
    // appended to it — run that instead of behaving as the CLI.
    if let Some(payload) = bundle::read_self() {
        return run_bundle(payload);
    }

    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(|s| s.as_str());

    match command {
        Some("run") => cmd_run(args.get(1)),
        Some("check") => cmd_check(args.get(1)),
        Some("build") => cmd_build(&args[1..]),
        Some("repl") => cmd_repl(),
        Some("new") => cmd_new(args.get(1)),
        Some("version") | Some("--version") | Some("-v") => {
            println!("plaintext {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("help") | Some("--help") | Some("-h") | None => {
            print_usage();
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("Unknown command `{}`.\n", other);
            print_usage();
            ExitCode::FAILURE
        }
    }
}

fn print_usage() {
    println!(
        "PlainText — a readability-first language.\n\n\
         Usage:\n  \
         plaintext run <file.pt>     Run a program\n  \
         plaintext check <file.pt>   Check a program for errors without running it\n  \
         plaintext build <file.pt>   Bundle into a standalone app (-o, --runtime, --run)\n  \
         plaintext repl              Start an interactive session\n  \
         plaintext new <name>        Create a new project folder\n  \
         plaintext version           Print the version"
    );
}

/// Lex + parse one file (stamping its spans with `file_id`), returning either
/// its program or an already-rendered error string pointing at the right file.
fn parse_file(path: &str, src: &str, file_id: u16) -> Result<ast::Program, String> {
    let tokens = Lexer::with_file(src, file_id).tokenize().map_err(|d| d.render(path))?;
    Parser::new(tokens).parse_program().map_err(|d| d.render(path))
}

/// Everything loaded from an entry file: the merged program (dependencies
/// first), the file-id → display-name table, and — for `build` — every source
/// keyed by a stable relative path, plus which key is the entry.
struct Loaded {
    program: ast::Program,
    files: Vec<String>,
    sources: Vec<(String, String)>,
    entry_key: String,
}

/// Load an entry file and every file it imports, splicing them into one program.
/// Returns an already-rendered error on failure.
fn load_program(entry: &str) -> Result<Loaded, String> {
    let mut ctx = LoadCtx {
        out: Vec::new(),
        files: Vec::new(),
        sources: Vec::new(),
        loaded: Vec::new(),
        in_progress: Vec::new(),
    };
    let entry_key = Path::new(entry)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("main.pt")
        .to_string();
    load_file(Path::new(entry), &entry_key, true, &mut ctx)?;
    Ok(Loaded {
        program: ast::Program { statements: ctx.out },
        files: ctx.files,
        sources: ctx.sources,
        entry_key,
    })
}

struct LoadCtx {
    out: Vec<ast::Stmt>,
    /// File id → display name; a file's id is its index here.
    files: Vec<String>,
    /// (relative key, source) for every file, for bundling.
    sources: Vec<(String, String)>,
    loaded: Vec<PathBuf>,
    in_progress: Vec<PathBuf>,
}

fn load_file(path: &Path, key: &str, is_entry: bool, ctx: &mut LoadCtx) -> Result<(), String> {
    let shown = clean_path(&path.display().to_string());
    let canonical = std::fs::canonicalize(path)
        .map_err(|_| format!("Could not find file `{}`.", shown))?;
    // Already merged, or currently being merged (an import cycle) — either way
    // its definitions are or will be present, so don't include it twice.
    if ctx.loaded.contains(&canonical) || ctx.in_progress.contains(&canonical) {
        return Ok(());
    }
    let src = std::fs::read_to_string(&canonical)
        .map_err(|e| format!("Could not read `{}`: {}", shown, e))?;

    // Assign this file its id (its index in the table) before lexing, so every
    // span it produces carries that id.
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
        load_file(&parent.join(&rel), &norm_join(&dir_of(key), &rel), false, ctx)?;
    }
    ctx.out.extend(own);
    ctx.in_progress.pop();
    ctx.loaded.push(canonical);
    Ok(())
}

/// Split a parsed file's statements into its file-imports (relative paths) and
/// everything else, rejecting a `game`/`window` block in a non-entry file.
/// Shared by the on-disk loader and the bundle loader.
fn split_imports(
    stmts: Vec<ast::Stmt>,
    is_entry: bool,
    shown: &str,
) -> Result<(Vec<String>, Vec<ast::Stmt>), String> {
    let mut imports = Vec::new();
    let mut own = Vec::new();
    for stmt in stmts {
        match stmt {
            ast::Stmt::ImportFile { path: rel, .. } => imports.push(rel),
            ast::Stmt::Game(_) | ast::Stmt::Window(_) if !is_entry => {
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

/// The directory portion of a relative key (`sub/a.pt` → `sub`, `a.pt` → ``).
fn dir_of(key: &str) -> String {
    match key.rfind('/') {
        Some(i) => key[..i].to_string(),
        None => String::new(),
    }
}

/// Join an import path onto a directory key and normalize `.`/`..`, keeping the
/// result relative to the bundle root (leading `..` are preserved). Backslashes
/// count as separators too, so a Windows-style import normalizes the same way.
fn norm_join(dir: &str, rel: &str) -> String {
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

/// Drop Windows' `\\?\` extended-length prefix so displayed paths read cleanly.
fn clean_path(p: &str) -> String {
    p.strip_prefix(r"\\?\").unwrap_or(p).to_string()
}

/// Turn a run result into an exit code: a real error prints and fails; an
/// `exit(code)` request becomes that status code.
fn finish(files: &[String], result: Result<(), Diagnostic>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(d) => match d.exit {
            Some(code) => ExitCode::from(code as u8),
            None => {
                eprintln!("{}", d.render_multi(files));
                ExitCode::FAILURE
            }
        },
    }
}

fn cmd_run(path: Option<&String>) -> ExitCode {
    let path = match path {
        Some(p) => p.clone(),
        None => {
            eprintln!("Usage: plaintext run <file.pt>");
            return ExitCode::FAILURE;
        }
    };
    let loaded = match load_program(&path) {
        Ok(l) => l,
        Err(msg) => {
            eprintln!("{}", msg);
            return ExitCode::FAILURE;
        }
    };
    if !type_check(&loaded.files, &loaded.program) {
        return ExitCode::FAILURE;
    }
    execute(loaded)
}

/// Run a loaded program: a top-level `game`/`window` block opens a Raylib
/// window; otherwise it runs as a console program.
fn execute(loaded: Loaded) -> ExitCode {
    let Loaded { program, files, .. } = loaded;
    for stmt in &program.statements {
        let result = match stmt {
            ast::Stmt::Game(g) => Some(game::run(&program, g)),
            ast::Stmt::Window(w) => Some(game::run_window(&program, w)),
            _ => None,
        };
        if let Some(r) = result {
            return finish(&files, r);
        }
    }
    let mut interp = Interpreter::new();
    finish(&files, interp.run(&program))
}

/// Reconstruct and run a program embedded by `plaintext build`. It was already
/// type-checked at build time, so this skips checking and just runs.
fn run_bundle(payload: bundle::Payload) -> ExitCode {
    match load_bundle(payload) {
        Ok(loaded) => execute(loaded),
        Err(msg) => {
            eprintln!("{}", msg);
            ExitCode::FAILURE
        }
    }
}

/// The bundle-side twin of `load_file`: walk the embedded source map by key
/// (instead of the disk), producing the same merged program + file table.
fn load_bundle(payload: bundle::Payload) -> Result<Loaded, String> {
    let map: HashMap<String, String> = payload.files.into_iter().collect();
    let mut out = Vec::new();
    let mut files = Vec::new();
    let mut loaded: Vec<String> = Vec::new();
    let mut in_progress: Vec<String> = Vec::new();
    load_key(&payload.entry, true, &map, &mut out, &mut files, &mut loaded, &mut in_progress)?;
    Ok(Loaded {
        program: ast::Program { statements: out },
        files,
        sources: Vec::new(),
        entry_key: payload.entry,
    })
}

#[allow(clippy::too_many_arguments)]
fn load_key(
    key: &str,
    is_entry: bool,
    map: &HashMap<String, String>,
    out: &mut Vec<ast::Stmt>,
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

fn cmd_check(path: Option<&String>) -> ExitCode {
    let path = match path {
        Some(p) => p.clone(),
        None => {
            eprintln!("Usage: plaintext check <file.pt>");
            return ExitCode::FAILURE;
        }
    };
    let loaded = match load_program(&path) {
        Ok(l) => l,
        Err(msg) => {
            eprintln!("{}", msg);
            return ExitCode::FAILURE;
        }
    };
    if type_check(&loaded.files, &loaded.program) {
        println!("OK — `{}` has no errors.", path);
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// An interactive read-eval-print loop. Type an expression to see its value;
/// definitions and variables persist across lines. Multi-line blocks (functions,
/// `if`, …) are read until their braces balance.
fn cmd_repl() -> ExitCode {
    use std::io::Write;
    println!("PlainText REPL — type an expression, or `exit` to quit.");
    let mut interp = Interpreter::new();
    let stdin = std::io::stdin();
    let mut buffer = String::new();
    loop {
        print!("{}", if buffer.is_empty() { "> " } else { "... " });
        let _ = std::io::stdout().flush();

        let mut line = String::new();
        match stdin.read_line(&mut line) {
            Ok(0) => {
                println!();
                break;
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("input error: {}", e);
                break;
            }
        }

        if buffer.is_empty() {
            let trimmed = line.trim();
            if trimmed == "exit" || trimmed == "quit" {
                break;
            }
            if trimmed.is_empty() {
                continue;
            }
        }

        buffer.push_str(&line);
        if brace_depth(&buffer) > 0 {
            continue; // an unfinished block — keep reading
        }

        let src = std::mem::take(&mut buffer);
        let program = match parse_file("repl", &src, 0) {
            Ok(p) => p,
            Err(msg) => {
                eprintln!("{}", msg);
                continue;
            }
        };
        match interp.eval_repl(&program) {
            Ok(Some(v)) => println!("{}", v.display()),
            Ok(None) => {}
            Err(d) => match d.exit {
                Some(code) => return ExitCode::from(code as u8),
                None => eprintln!("{}", d.render("repl")),
            },
        }
    }
    ExitCode::SUCCESS
}

/// Net `{` minus `}` in a source fragment, floored at zero — used to tell when a
/// REPL block is still open.
fn brace_depth(s: &str) -> i32 {
    let mut depth: i32 = 0;
    for c in s.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
    }
    depth.max(0)
}

/// `plaintext build <file.pt> [-o <out>] [--runtime <path>] [--run]`
///
/// Produces a standalone executable by appending the program's source to a copy
/// of a runtime binary. With no `--runtime`, that's this very interpreter (so
/// the output targets the host OS). Point `--runtime` at a macOS `plaintext`
/// binary to build a Mac app from any machine — it's just bytes appended, no
/// cross-compiler involved. `--run` builds then runs it, to check it end to end.
fn cmd_build(args: &[String]) -> ExitCode {
    let mut entry: Option<String> = None;
    let mut out_arg: Option<String> = None;
    let mut runtime_path: Option<String> = None;
    let mut run = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                i += 1;
                match args.get(i) {
                    Some(v) => out_arg = Some(v.clone()),
                    None => {
                        eprintln!("`-o` needs a file name.");
                        return ExitCode::FAILURE;
                    }
                }
            }
            "--runtime" => {
                i += 1;
                match args.get(i) {
                    Some(v) => runtime_path = Some(v.clone()),
                    None => {
                        eprintln!("`--runtime` needs a path to a plaintext binary.");
                        return ExitCode::FAILURE;
                    }
                }
            }
            "--run" => run = true,
            other if other.starts_with('-') => {
                eprintln!("Unknown option `{}`.", other);
                return ExitCode::FAILURE;
            }
            _ if entry.is_none() => entry = Some(args[i].clone()),
            other => {
                eprintln!("Unexpected argument `{}`.", other);
                return ExitCode::FAILURE;
            }
        }
        i += 1;
    }

    let entry = match entry {
        Some(e) => e,
        None => {
            eprintln!("Usage: plaintext build <file.pt> [-o <out>] [--runtime <path>] [--run]");
            return ExitCode::FAILURE;
        }
    };

    // Load + type-check first, so a broken program never becomes an app.
    let loaded = match load_program(&entry) {
        Ok(l) => l,
        Err(msg) => {
            eprintln!("{}", msg);
            return ExitCode::FAILURE;
        }
    };
    if !type_check(&loaded.files, &loaded.program) {
        return ExitCode::FAILURE;
    }

    // The runtime to embed into: an explicit binary (e.g. a macOS one for cross
    // building) or a clean copy of this interpreter.
    let runtime_bytes = match &runtime_path {
        Some(p) => match std::fs::read(p) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Could not read runtime `{}`: {}", p, e);
                return ExitCode::FAILURE;
            }
        },
        None => match std::env::current_exe().and_then(std::fs::read) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Could not read the interpreter binary: {}", e);
                return ExitCode::FAILURE;
            }
        },
    };
    let runtime = bundle::strip(&runtime_bytes); // start from a clean runtime

    let payload = bundle::encode(&bundle::Payload {
        entry: loaded.entry_key.clone(),
        files: loaded.sources.clone(),
    });
    let image = bundle::append(runtime, &payload);

    let out_path = out_arg.unwrap_or_else(|| default_output(&entry, runtime));
    if let Err(e) = std::fs::write(&out_path, &image) {
        eprintln!("Could not write `{}`: {}", out_path, e);
        return ExitCode::FAILURE;
    }
    make_executable(&out_path);
    copy_assets(&entry, &out_path);

    println!("Built {} ({} KB).", out_path, image.len() / 1024);
    print_run_note(&out_path, runtime);

    if run {
        // Verify end to end without needing to launch the new binary (which the
        // OS may block until trusted): read the payload back and run it here.
        println!("--- running {} ---", out_path);
        match std::fs::read(&out_path).ok().as_deref().and_then(bundle::extract) {
            Some(p) => return run_bundle(p),
            None => {
                eprintln!("Built, but couldn't read the payload back to run it.");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

/// Output file name when `-o` isn't given: the entry's stem, with `.exe` only if
/// the runtime is a Windows binary (so cross-built Mac apps get no extension).
fn default_output(entry: &str, runtime: &[u8]) -> String {
    let stem = Path::new(entry)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("app");
    if runtime.starts_with(b"MZ") {
        format!("{}.exe", stem)
    } else {
        stem.to_string()
    }
}

/// A short note about running the produced binary, tailored to its platform.
fn print_run_note(out_path: &str, runtime: &[u8]) {
    if runtime.starts_with(b"MZ") {
        println!("Run it:  {}", out_path);
        println!("(Windows may show a SmartScreen prompt — choose \"More info\" → \"Run anyway\".)");
    } else {
        println!("This is a macOS binary. On the Mac, make it runnable once:  chmod +x {}", out_path);
        println!("(If Gatekeeper blocks it:  xattr -dr com.apple.quarantine {})", out_path);
    }
}

#[cfg(unix)]
fn make_executable(path: &str) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(path) {
        let mut perms = meta.permissions();
        perms.set_mode(perms.mode() | 0o111);
        let _ = std::fs::set_permissions(path, perms);
    }
}

#[cfg(not(unix))]
fn make_executable(_path: &str) {}

/// If there's an `assets/` folder next to the program, copy it beside the app so
/// `load_sprite("assets/...")` still resolves when the app runs elsewhere.
fn copy_assets(entry: &str, out_path: &str) {
    let src = Path::new(entry).parent().unwrap_or(Path::new(".")).join("assets");
    if !src.is_dir() {
        return;
    }
    let dst = Path::new(out_path).parent().unwrap_or(Path::new(".")).join("assets");
    match copy_dir(&src, &dst) {
        Ok(()) => println!("Copied assets/ next to the app."),
        Err(e) => eprintln!("(note: couldn't copy assets/: {})", e),
    }
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn cmd_new(name: Option<&String>) -> ExitCode {
    let name = match name {
        Some(n) => n.clone(),
        None => {
            eprintln!("Usage: plaintext new <name>");
            return ExitCode::FAILURE;
        }
    };
    let dir = Path::new(&name);
    if dir.exists() {
        eprintln!("`{}` already exists.", name);
        return ExitCode::FAILURE;
    }
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("Could not create `{}`: {}", name, e);
        return ExitCode::FAILURE;
    }

    let config = format!(
        "// PlainText project configuration\n\
         name = \"{}\"\n\
         version = \"0.1.0\"\n\
         entry = \"main.pt\"\n",
        name
    );
    let main_pt = "make function called main() {\n    \
        print(\"Hello from PlainText!\")\n}\n";

    let cfg_path = dir.join("plaintext.toml");
    let main_path = dir.join("main.pt");
    if let Err(e) = std::fs::write(&cfg_path, config) {
        eprintln!("Could not write config: {}", e);
        return ExitCode::FAILURE;
    }
    if let Err(e) = std::fs::write(&main_path, main_pt) {
        eprintln!("Could not write main.pt: {}", e);
        return ExitCode::FAILURE;
    }

    println!("Created project `{}`.", name);
    println!("  {}", cfg_path.display());
    println!("  {}", main_path.display());
    println!("\nRun it with:  plaintext run {}/main.pt", name);
    ExitCode::SUCCESS
}
