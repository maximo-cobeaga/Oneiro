use crate::diag::{Diagnostic, Pos};

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Ident(String),
    Str(String),
    Num(f64, String),
    Color(String),
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Colon,
    Comma,
    Arrow,
    Equals,
    Newline,
    Eof,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub pos: Pos,
}

pub fn describe(t: &Tok) -> String {
    match t {
        Tok::Ident(s) => format!("'{}'", s),
        Tok::Str(s) => format!("string \"{}\"", s),
        Tok::Num(n, u) => format!("number {}{}", n, u),
        Tok::Color(c) => format!("color #{}", c),
        Tok::LBrace => "'{'".into(),
        Tok::RBrace => "'}'".into(),
        Tok::LParen => "'('".into(),
        Tok::RParen => "')'".into(),
        Tok::LBracket => "'['".into(),
        Tok::RBracket => "']'".into(),
        Tok::Colon => "':'".into(),
        Tok::Comma => "','".into(),
        Tok::Arrow => "'->'".into(),
        Tok::Equals => "'='".into(),
        Tok::Newline => "end of line".into(),
        Tok::Eof => "end of file".into(),
    }
}

pub fn lex(src: &str, file: &str, diags: &mut Vec<Diagnostic>) -> Vec<Token> {
    let c: Vec<char> = src.chars().collect();
    let n = c.len();
    let mut i = 0usize;
    let mut line = 1u32;
    let mut col = 1u32;
    let mut out: Vec<Token> = Vec::new();

    let push_nl = |out: &mut Vec<Token>, pos: Pos| {
        if !matches!(out.last().map(|t| &t.tok), Some(Tok::Newline) | None) {
            out.push(Token { tok: Tok::Newline, pos });
        }
    };

    while i < n {
        let ch = c[i];
        let pos = Pos { line, col };
        match ch {
            '\n' => {
                push_nl(&mut out, pos);
                i += 1;
                line += 1;
                col = 1;
            }
            ' ' | '\t' | '\r' => {
                i += 1;
                col += 1;
            }
            '#' => {
                let mut j = i + 1;
                let mut hex = String::new();
                while j < n && c[j].is_ascii_hexdigit() {
                    hex.push(c[j]);
                    j += 1;
                }
                let boundary = j >= n || !(c[j].is_alphanumeric() || c[j] == '_');
                if (hex.len() == 6 || hex.len() == 3) && boundary {
                    out.push(Token { tok: Tok::Color(hex), pos });
                    col += (j - i) as u32;
                    i = j;
                } else {
                    while i < n && c[i] != '\n' {
                        i += 1;
                        col += 1;
                    }
                }
            }
            '"' => {
                let mut j = i + 1;
                let mut s = String::new();
                let mut closed = false;
                while j < n {
                    match c[j] {
                        '"' => {
                            closed = true;
                            j += 1;
                            break;
                        }
                        '\n' => break,
                        '\\' if j + 1 < n => {
                            match c[j + 1] {
                                'n' => s.push('\n'),
                                't' => s.push('\t'),
                                other => s.push(other),
                            }
                            j += 2;
                        }
                        other => {
                            s.push(other);
                            j += 1;
                        }
                    }
                }
                if !closed {
                    diags.push(
                        Diagnostic::error("E0002", file, pos, "unterminated string")
                            .suggest("add a closing double quote (\") on the same line"),
                    );
                }
                out.push(Token { tok: Tok::Str(s), pos });
                col += (j - i) as u32;
                i = j;
            }
            '{' | '}' | '(' | ')' | '[' | ']' | ':' | ',' | '=' => {
                let tok = match ch {
                    '{' => Tok::LBrace,
                    '}' => Tok::RBrace,
                    '(' => Tok::LParen,
                    ')' => Tok::RParen,
                    '[' => Tok::LBracket,
                    ']' => Tok::RBracket,
                    ':' => Tok::Colon,
                    ',' => Tok::Comma,
                    _ => Tok::Equals,
                };
                out.push(Token { tok, pos });
                i += 1;
                col += 1;
            }
            '-' if i + 1 < n && c[i + 1] == '>' => {
                out.push(Token { tok: Tok::Arrow, pos });
                i += 2;
                col += 2;
            }
            _ if ch.is_ascii_digit() || (ch == '-' && i + 1 < n && c[i + 1].is_ascii_digit()) => {
                let start = i;
                if ch == '-' {
                    i += 1;
                }
                while i < n && c[i].is_ascii_digit() {
                    i += 1;
                }
                if i + 1 < n && c[i] == '.' && c[i + 1].is_ascii_digit() {
                    i += 1;
                    while i < n && c[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                let text: String = c[start..i].iter().collect();
                let ustart = i;
                while i < n && c[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let unit: String = c[ustart..i].iter().collect();
                let value: f64 = text.parse().unwrap_or(0.0);
                out.push(Token { tok: Tok::Num(value, unit), pos });
                col += (i - start) as u32;
            }
            _ if ch.is_alphabetic() || ch == '_' => {
                let start = i;
                i += 1;
                loop {
                    if i < n && (c[i].is_alphanumeric() || c[i] == '_') {
                        i += 1;
                    } else if i + 1 < n && c[i] == '.' && (c[i + 1].is_alphabetic() || c[i + 1] == '_') {
                        i += 1;
                    } else {
                        break;
                    }
                }
                let text: String = c[start..i].iter().collect();
                out.push(Token { tok: Tok::Ident(text), pos });
                col += (i - start) as u32;
            }
            _ => {
                diags.push(
                    Diagnostic::error("E0001", file, pos, format!("unexpected character '{}'", ch))
                        .suggest("remove it, or put it inside a string or a # comment"),
                );
                i += 1;
                col += 1;
            }
        }
    }
    let pos = Pos { line, col };
    push_nl(&mut out, pos);
    out.push(Token { tok: Tok::Eof, pos });
    out
}
