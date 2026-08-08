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
mod gfx;
mod interpreter;
mod lexer;
mod parser;
mod token;
mod ui;
mod value;

use std::path::Path;
use std::process::ExitCode;

use checker::Checker;
use interpreter::Interpreter;
use lexer::Lexer;
use parser::Parser;

/// Type-check a program, printing every diagnostic. Returns true if it's clean.
fn type_check(path: &str, program: &ast::Program) -> bool {
    let errors = Checker::new().check(program);
    if errors.is_empty() {
        return true;
    }
    for d in &errors {
        eprintln!("{}\n", d.render(path));
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
         plaintext new <name>        Create a new project folder\n  \
         plaintext version           Print the version"
    );
}

/// Load a source file, or print a friendly error and exit.
fn read_source(path: &str) -> Result<String, ExitCode> {
    match std::fs::read_to_string(path) {
        Ok(src) => Ok(src),
        Err(e) => {
            eprintln!("Could not read `{}`: {}", path, e);
            Err(ExitCode::FAILURE)
        }
    }
}

/// Lex + parse a source string into a program, printing diagnostics on failure.
fn parse_source(path: &str, src: &str) -> Option<ast::Program> {
    let tokens = match Lexer::new(src).tokenize() {
        Ok(t) => t,
        Err(d) => {
            eprintln!("{}", d.render(path));
            return None;
        }
    };
    match Parser::new(tokens).parse_program() {
        Ok(p) => Some(p),
        Err(d) => {
            eprintln!("{}", d.render(path));
            None
        }
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
    let src = match read_source(&path) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let program = match parse_source(&path, &src) {
        Some(p) => p,
        None => return ExitCode::FAILURE,
    };
    if !type_check(&path, &program) {
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
            return match r {
                Ok(()) => ExitCode::SUCCESS,
                Err(d) => {
                    eprintln!("{}", d.render(&path));
                    ExitCode::FAILURE
                }
            };
        }
    }

    let mut interp = Interpreter::new();
    match interp.run(&program) {
        Ok(()) => ExitCode::SUCCESS,
        Err(d) => {
            eprintln!("{}", d.render(&path));
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
    let src = match read_source(&path) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let program = match parse_source(&path, &src) {
        Some(p) => p,
        None => return ExitCode::FAILURE,
    };
    if type_check(&path, &program) {
        println!("OK — `{}` has no errors.", path);
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
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
