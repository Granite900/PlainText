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
}

impl Diagnostic {
    pub fn new(span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic { span, message: message.into(), hint: None }
    }

    /// Attach a one-line "Hint:" suggestion. Returns self so it can be chained:
    /// `Diagnostic::new(span, "...").with_hint("try ...")`.
    pub fn with_hint(mut self, hint: impl Into<String>) -> Diagnostic {
        self.hint = Some(hint.into());
        self
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
