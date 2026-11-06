//! Structured diagnostics. Every diagnostic has a code, a location, a message and
//! (when possible) a suggestion, and can be rendered for humans or as JSON for AI tools.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pos {
    pub line: u32,
    pub col: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub file: String,
    pub line: u32,
    pub col: u32,
    pub message: String,
    pub suggestion: Option<String>,
}

impl Diagnostic {
    pub fn error(code: &str, file: &str, pos: Pos, message: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Error,
            code: code.to_string(),
            file: file.to_string(),
            line: pos.line,
            col: pos.col,
            message: message.into(),
            suggestion: None,
        }
    }

    pub fn warning(code: &str, file: &str, pos: Pos, message: impl Into<String>) -> Self {
        let mut d = Self::error(code, file, pos, message);
        d.severity = Severity::Warning;
        d
    }

    pub fn suggest(mut self, s: impl Into<String>) -> Self {
        self.suggestion = Some(s.into());
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }

    pub fn render(&self) -> String {
        let sev = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        let mut s = format!(
            "{}[{}] {}:{}:{}  {}",
            sev, self.code, self.file, self.line, self.col, self.message
        );
        if let Some(sg) = &self.suggestion {
            s.push_str(&format!("\n      suggestion: {}", sg));
        }
        s
    }

    pub fn to_json(&self) -> String {
        let sev = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        let sugg = match &self.suggestion {
            Some(s) => format!("\"{}\"", esc(s)),
            None => "null".to_string(),
        };
        format!(
            "{{\"severity\":\"{}\",\"code\":\"{}\",\"file\":\"{}\",\"line\":{},\"col\":{},\"message\":\"{}\",\"suggestion\":{}}}",
            sev,
            esc(&self.code),
            esc(&self.file),
            self.line,
            self.col,
            esc(&self.message),
            sugg
        )
    }
}

pub fn to_json_array(ds: &[Diagnostic]) -> String {
    let items: Vec<String> = ds.iter().map(|d| d.to_json()).collect();
    format!("[{}]", items.join(","))
}

fn esc(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o
}
