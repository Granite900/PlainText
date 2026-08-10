//! The `plaintext` command-line tool.
//!
//! Subcommands:
//!   plaintext run <file.pt>     parse and execute a program
//!   plaintext check <file.pt>   parse only, report any errors
//!   plaintext build <file.pt>   bundle into a standalone executable
//!   plaintext repl              start an interactive session
//!   plaintext lsp               language server (stdio) for editors
//!   plaintext edit_tilemap <file.pt>   paint a tilemap and rewrite the file
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
mod gamekit;
mod gc;
mod gfx;
mod gpu;
mod interpreter;
mod lexer;
mod load;
mod lsp;
mod nn;
mod parser;
mod tilemap_edit_source;
mod tilemap_editor;
mod token;
mod ui;
mod value;
mod web;

use std::collections::HashMap;
use std::path::Path;
use std::process::ExitCode;

use checker::Checker;
use diagnostics::Diagnostic;
use interpreter::Interpreter;
use load::{load_bundle, load_program, parse_file, Loaded};

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
        Some("lsp") => {
            if let Err(e) = lsp::run() {
                eprintln!("language server error: {}", e);
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Some("new") => cmd_new(args.get(1)),
        Some("edit_tilemap") => cmd_edit_tilemap(args.get(1)),
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
         plaintext lsp               Language server for editors (stdio)\n  \
         plaintext edit_tilemap <file.pt>  Paint a tilemap (rewrites the file)\n  \
         plaintext new <name>        Create a new project folder\n  \
         plaintext version           Print the version"
    );
}

fn cmd_edit_tilemap(path: Option<&String>) -> ExitCode {
    let path = match path {
        Some(p) => p.clone(),
        None => {
            eprintln!("Usage: plaintext edit_tilemap <file.pt>");
            return ExitCode::FAILURE;
        }
    };
    match tilemap_editor::run(Path::new(&path)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("{}", msg);
            ExitCode::FAILURE
        }
    }
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
    let map: HashMap<String, String> = payload.files.into_iter().collect();
    match load_bundle(payload.entry, map) {
        Ok(loaded) => execute(loaded),
        Err(msg) => {
            eprintln!("{}", msg);
            ExitCode::FAILURE
        }
    }
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
