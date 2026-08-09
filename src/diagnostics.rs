//! One shared error type for every stage (lexer, parser, interpreter), so all
//! diagnostics print in the same human-readable shape:
//!
//! ```text
//! Error at main.pt:14:5
//!   Type mismatch: ...
//!   Hint: ...
//! ```

use crate::token::Span;

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub span: Span,
    pub message: String,
    pub hint: Option<String>,
    /// Set when this isn't really an error but an `exit(code)` request unwinding
    /// the interpreter stack. The top-level runner turns it into that exit code.
    pub exit: Option<i32>,
}

impl Diagnostic {
    pub fn new(span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic { span, message: message.into(), hint: None, exit: None }
    }

    /// A non-error signal that the program asked to stop with a status code.
    pub fn exit(code: i32) -> Diagnostic {
        Diagnostic { span: Span::new(0, 0), message: String::new(), hint: None, exit: Some(code) }
    }

    /// Attach a one-line "Hint:" suggestion. Returns self so it can be chained:
    /// `Diagnostic::new(span, "...").with_hint("try ...")`.
    pub fn with_hint(mut self, hint: impl Into<String>) -> Diagnostic {
        self.hint = Some(hint.into());
        self
    }

    /// Render, resolving the file name from the span's file id via the CLI's
    /// file table (built while loading imports). Falls back to the entry file.
    pub fn render_multi(&self, files: &[String]) -> String {
        let name = files
            .get(self.span.file as usize)
            .or_else(|| files.first())
            .map(String::as_str)
            .unwrap_or("<unknown>");
        self.render(name)
    }

    /// Render the diagnostic for the terminal. `file` is the path shown in the
    /// location line.
    pub fn render(&self, file: &str) -> String {
        let mut out = format!(
            "Error at {}:{}:{}\n  {}",
            file, self.span.line, self.span.col, self.message
        );
        if let Some(hint) = &self.hint {
            out.push_str(&format!("\n  Hint: {}", hint));
        }
        out
    }
}
