//! The `plaintext` command-line tool.
//!
//! Subcommands:
//!   plaintext run <file.pt>     parse and execute a program
//!   plaintext check <file.pt>   parse only, report any errors
//!   plaintext new <name>        scaffold a new project folder
//!   plaintext version           print the version

mod ast;
mod checker;
mod diagnostics;
mod game;
mod gc;
mod gfx;
mod interpreter;
mod lexer;
mod parser;
mod token;
mod ui;
mod value;

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
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(|s| s.as_str());

    match command {
        Some("run") => cmd_run(args.get(1)),
        Some("check") => cmd_check(args.get(1)),
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
/// first) and the table mapping each span's file id to a display name.
struct Loaded {
    program: ast::Program,
    files: Vec<String>,
}

/// Load an entry file and every file it imports, splicing them into one program.
/// Returns an already-rendered error on failure.
fn load_program(entry: &str) -> Result<Loaded, String> {
    let mut ctx = LoadCtx {
        out: Vec::new(),
        files: Vec::new(),
        loaded: Vec::new(),
        in_progress: Vec::new(),
    };
    load_file(Path::new(entry), true, &mut ctx)?;
    Ok(Loaded { program: ast::Program { statements: ctx.out }, files: ctx.files })
}

struct LoadCtx {
    out: Vec<ast::Stmt>,
    /// File id → display name; a file's id is its index here.
    files: Vec<String>,
    loaded: Vec<PathBuf>,
    in_progress: Vec<PathBuf>,
}

fn load_file(path: &Path, is_entry: bool, ctx: &mut LoadCtx) -> Result<(), String> {
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
    let program = parse_file(&shown, &src, file_id)?;

    ctx.in_progress.push(canonical.clone());
    let parent = canonical.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut own = Vec::new();
    for stmt in program.statements {
        match stmt {
            ast::Stmt::ImportFile { path: rel, .. } => {
                load_file(&parent.join(&rel), false, ctx)?;
            }
            ast::Stmt::Game(_) | ast::Stmt::Window(_) if !is_entry => {
                ctx.in_progress.pop();
                return Err(format!(
                    "Cannot import `{}`: an imported file can't contain a `game` or `window` block.",
                    shown
                ));
            }
            other => own.push(other),
        }
    }
    ctx.out.extend(own);
    ctx.in_progress.pop();
    ctx.loaded.push(canonical);
    Ok(())
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
    let Loaded { program, files } = match load_program(&path) {
        Ok(l) => l,
        Err(msg) => {
            eprintln!("{}", msg);
            return ExitCode::FAILURE;
        }
    };
    if !type_check(&files, &program) {
        return ExitCode::FAILURE;
    }

    // A file with a top-level `game` or `window` block runs on a Raylib window
    // instead of as a console program.
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

fn cmd_check(path: Option<&String>) -> ExitCode {
    let path = match path {
        Some(p) => p.clone(),
        None => {
            eprintln!("Usage: plaintext check <file.pt>");
            return ExitCode::FAILURE;
        }
    };
    let Loaded { program, files } = match load_program(&path) {
        Ok(l) => l,
        Err(msg) => {
            eprintln!("{}", msg);
            return ExitCode::FAILURE;
        }
    };
    if type_check(&files, &program) {
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
