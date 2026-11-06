use crate::ast::*;
use crate::diag::{Diagnostic, Pos};
use crate::lexer::{describe, lex, Tok, Token};

/// Parses a whole Oneiro file. Never fails: problems are returned as diagnostics and the
/// parser recovers at the next line so several errors can be reported at once.
pub fn parse(src: &str, file: &str) -> (File, Vec<Diagnostic>) {
    let mut diags = Vec::new();
    let toks = lex(src, file, &mut diags);
    let mut p = Parser { toks, i: 0, file: file.to_string(), diags };
    let f = p.parse_file();
    (f, p.diags)
}

struct Parser {
    toks: Vec<Token>,
    i: usize,
    file: String,
    diags: Vec<Diagnostic>,
}

type R<T> = Option<T>;

impl Parser {
    // ---------------------------------------------------------------- helpers
    fn peek(&self) -> &Tok {
        &self.toks[self.i].tok
    }
    fn peek_at(&self, k: usize) -> &Tok {
        let idx = (self.i + k).min(self.toks.len() - 1);
        &self.toks[idx].tok
    }
    fn pos(&self) -> Pos {
        self.toks[self.i].pos
    }
    fn bump(&mut self) -> Token {
        let t = self.toks[self.i].clone();
        if self.i < self.toks.len() - 1 {
            self.i += 1;
        }
        t
    }
    fn err(&mut self, code: &str, pos: Pos, msg: impl Into<String>, sugg: Option<String>) {
        let mut d = Diagnostic::error(code, &self.file, pos, msg);
        if let Some(s) = sugg {
            d = d.suggest(s);
        }
        self.diags.push(d);
    }
    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Tok::Newline) {
            self.bump();
        }
    }
    fn skip_to_eol(&mut self) {
        while !matches!(self.peek(), Tok::Newline | Tok::Eof) {
            self.bump();
        }
    }
    fn at_eol(&self) -> bool {
        matches!(self.peek(), Tok::Newline | Tok::Eof | Tok::RBrace)
    }
    fn word(&self) -> Option<String> {
        if let Tok::Ident(s) = self.peek() {
            Some(s.clone())
        } else {
            None
        }
    }
    fn is_word(&self, w: &str) -> bool {
        matches!(self.peek(), Tok::Ident(s) if s == w)
    }
    fn expect_word(&mut self, what: &str) -> R<String> {
        if let Some(w) = self.word() {
            self.bump();
            Some(w)
        } else {
            let pos = self.pos();
            let found = describe(self.peek());
            self.err("E0101", pos, format!("expected {}, found {}", what, found), None);
            None
        }
    }
    fn expect_tok(&mut self, t: Tok, what: &str) -> R<()> {
        if *self.peek() == t {
            self.bump();
            Some(())
        } else {
            let pos = self.pos();
            let found = describe(self.peek());
            self.err("E0101", pos, format!("expected {}, found {}", what, found), None);
            None
        }
    }
    fn expect_str(&mut self, what: &str) -> R<String> {
        if let Tok::Str(s) = self.peek().clone() {
            self.bump();
            Some(s)
        } else {
            let pos = self.pos();
            let found = describe(self.peek());
            self.err(
                "E0101",
                pos,
                format!("expected {} in double quotes, found {}", what, found),
                None,
            );
            None
        }
    }
    fn expect_dur(&mut self) -> R<f64> {
        if let Tok::Num(n, u) = self.peek().clone() {
            let pos = self.pos();
            self.bump();
            match dur_seconds(n, &u) {
                Some(s) => Some(s),
                None => {
                    self.err(
                        "E0105",
                        pos,
                        format!("unknown time unit '{}'", u),
                        Some("use s (seconds) or ms (milliseconds), e.g. 2s or 500ms".into()),
                    );
                    None
                }
            }
        } else {
            let pos = self.pos();
            let found = describe(self.peek());
            self.err(
                "E0101",
                pos,
                format!("expected a duration like 2s, found {}", found),
                None,
            );
            None
        }
    }
    fn end_stmt(&mut self) {
        match self.peek() {
            Tok::Newline | Tok::Eof | Tok::RBrace => {}
            other => {
                let pos = self.pos();
                let d = describe(other);
                self.err(
                    "E0104",
                    pos,
                    format!("unexpected {} at the end of the statement", d),
                    Some("each statement goes on its own line; join parallel actions with 'and'".into()),
                );
                self.skip_to_eol();
            }
        }
    }

    // ---------------------------------------------------------------- file level
    fn parse_file(&mut self) -> File {
        let mut f = File::default();
        loop {
            self.skip_newlines();
            let pos = self.pos();
            match self.peek().clone() {
                Tok::Eof => break,
                Tok::Ident(w) => match w.as_str() {
                    "oneiro" => {
                        self.bump();
                        match self.peek().clone() {
                            Tok::Num(n, _) => {
                                self.bump();
                                f.version = Some(format!("{}", n));
                            }
                            Tok::Str(s) => {
                                self.bump();
                                f.version = Some(s);
                            }
                            _ => {
                                self.err(
                                    "E0103",
                                    pos,
                                    "expected a version after 'oneiro'",
                                    Some("write: oneiro 0.1".into()),
                                );
                            }
                        }
                        self.end_stmt();
                    }
                    "project" => {
                        self.bump();
                        if let Some(s) = self.expect_str("a project name") {
                            f.project = Some(s);
                        }
                        self.end_stmt();
                    }
                    "style" if self.is_default_style() => {
                        self.bump();
                        self.bump(); // default
                        self.bump(); // :
                        if let Some(s) = self.expect_word("a style name") {
                            f.default_style = Some(s);
                        }
                        self.end_stmt();
                    }
                    "use" => {
                        self.bump();
                        let kind = self.expect_word("a kind (character, set, style)");
                        let name = self.expect_word("a name");
                        let mut style = None;
                        if self.word().as_deref() == Some("style") {
                            self.bump();
                            style = self.expect_word("a style name (cutout_2d, rubber_hose, clay)");
                        }
                        if let (Some(kind), Some(name)) = (kind, name) {
                            f.uses.push(Use { kind, name, style, pos });
                        }
                        self.end_stmt();
                    }
                    "scene" => {
                        if let Some(s) = self.parse_scene() {
                            f.scenes.push(s);
                        }
                    }
                    _ => {
                        if let Some(b) = self.parse_block() {
                            f.blocks.push(b);
                        }
                    }
                },
                other => {
                    self.err(
                        "E0102",
                        pos,
                        format!("unexpected {} at top level", describe(&other)),
                        Some("top-level items are: use, scene, character, set, style".into()),
                    );
                    self.skip_to_eol();
                }
            }
        }
        f
    }

    fn is_default_style(&self) -> bool {
        matches!(self.peek_at(1), Tok::Ident(s) if s == "default") && matches!(self.peek_at(2), Tok::Colon)
    }

    // ---------------------------------------------------------------- generic blocks
    fn parse_block(&mut self) -> R<Block> {
        let pos = self.pos();
        let kind = self.expect_word("a block keyword")?;
        let name = match self.peek().clone() {
            Tok::Ident(s) => {
                self.bump();
                Some(s)
            }
            Tok::Str(s) => {
                self.bump();
                Some(s)
            }
            Tok::Num(n, _) => {
                self.bump();
                Some(format!("{}", n))
            }
            _ => None,
        };
        self.skip_newlines();
        if self.expect_tok(Tok::LBrace, "'{'").is_none() {
            self.skip_to_eol();
            return None;
        }
        let entries = self.parse_entries();
        Some(Block { kind, name, entries, pos })
    }

    /// Parses entries up to and including the closing '}'.
    fn parse_entries(&mut self) -> Vec<Entry> {
        let mut out = Vec::new();
        loop {
            while matches!(self.peek(), Tok::Newline | Tok::Comma) {
                self.bump();
            }
            let pos = self.pos();
            match self.peek().clone() {
                Tok::RBrace => {
                    self.bump();
                    break;
                }
                Tok::Eof => {
                    self.err("E0106", pos, "missing closing '}'", Some("add a '}' to close the block".into()));
                    break;
                }
                Tok::Ident(key) => {
                    self.bump();
                    match self.peek().clone() {
                        Tok::Colon => {
                            self.bump();
                            if let Some(v) = self.parse_value() {
                                out.push(Entry { key, name: None, body: EntryBody::Value(v), pos });
                            } else {
                                self.skip_to_eol();
                            }
                        }
                        Tok::LBrace => {
                            self.bump();
                            let inner = self.parse_entries();
                            out.push(Entry { key, name: None, body: EntryBody::Block(inner), pos });
                        }
                        Tok::Ident(_) | Tok::Num(_, _) | Tok::Str(_)
                            if matches!(self.peek_at(1), Tok::LBrace) =>
                        {
                            let name = match self.bump().tok {
                                Tok::Ident(s) | Tok::Str(s) => s,
                                Tok::Num(n, _) => format!("{}", n),
                                _ => String::new(),
                            };
                            self.bump(); // {
                            let inner = self.parse_entries();
                            out.push(Entry { key, name: Some(name), body: EntryBody::Block(inner), pos });
                        }
                        other => {
                            self.err(
                                "E0107",
                                pos,
                                format!("expected ':' or '{{' after '{}', found {}", key, describe(&other)),
                                Some(format!("write '{}: value' or '{} {{ ... }}'", key, key)),
                            );
                            self.skip_to_eol();
                        }
                    }
                }
                other => {
                    self.err("E0107", pos, format!("expected a key, found {}", describe(&other)), None);
                    self.skip_to_eol();
                }
            }
        }
        out
    }

    fn parse_value(&mut self) -> R<Value> {
        let pos = self.pos();
        match self.peek().clone() {
            Tok::Str(s) => {
                self.bump();
                Some(Value::Str(s))
            }
            Tok::Color(c) => {
                self.bump();
                Some(Value::Color(c))
            }
            Tok::Num(n, u) => {
                self.bump();
                let first = Value::Num(n, u);
                if self.is_word("x") && matches!(self.peek_at(1), Tok::Num(_, _)) {
                    let mut dims = vec![first];
                    while self.is_word("x") && matches!(self.peek_at(1), Tok::Num(_, _)) {
                        self.bump();
                        if let Tok::Num(n, u) = self.bump().tok {
                            dims.push(Value::Num(n, u));
                        }
                    }
                    Some(Value::Dim(dims))
                } else {
                    Some(first)
                }
            }
            Tok::LParen => {
                self.bump();
                Some(Value::Tuple(self.parse_seq(Tok::RParen)?))
            }
            Tok::LBracket => {
                self.bump();
                Some(Value::List(self.parse_seq(Tok::RBracket)?))
            }
            Tok::LBrace => {
                self.bump();
                Some(Value::Map(self.parse_entries()))
            }
            Tok::Ident(s) => {
                self.bump();
                if matches!(self.peek(), Tok::LBrace) {
                    self.bump();
                    Some(Value::Call(s, self.parse_entries()))
                } else {
                    Some(Value::Ident(s))
                }
            }
            other => {
                self.err(
                    "E0110",
                    pos,
                    format!("expected a value, found {}", describe(&other)),
                    Some("values are strings, numbers (2m, 0.5), colors (#RRGGBB), names, (a, b), [a, b] or { k: v }".into()),
                );
                None
            }
        }
    }

    fn parse_seq(&mut self, close: Tok) -> R<Vec<Value>> {
        let mut out = Vec::new();
        loop {
            while matches!(self.peek(), Tok::Newline | Tok::Comma) {
                self.bump();
            }
            if *self.peek() == close {
                self.bump();
                return Some(out);
            }
            if matches!(self.peek(), Tok::Eof) {
                let pos = self.pos();
                self.err("E0106", pos, "unclosed bracket", None);
                return None;
            }
            out.push(self.parse_value()?);
        }
    }

    // ---------------------------------------------------------------- scenes
    fn parse_scene(&mut self) -> R<Scene> {
        let pos = self.pos();
        self.bump(); // scene
        let number = match self.peek().clone() {
            Tok::Num(n, _) => {
                self.bump();
                n as u32
            }
            _ => {
                self.err("E0101", pos, "expected a scene number", Some("write: scene 1 \"Title\" in set_name {".into()));
                self.skip_to_eol();
                return None;
            }
        };
        let title = if let Tok::Str(s) = self.peek().clone() {
            self.bump();
            s
        } else {
            String::new()
        };
        if !self.is_word("in") {
            let p = self.pos();
            self.err("E0101", p, "expected 'in <set>' after the scene title", Some("write: scene 1 \"Title\" in diner {".into()));
            self.skip_to_eol();
            return None;
        }
        self.bump();
        let set = self.expect_word("a set name")?;
        self.skip_newlines();
        self.expect_tok(Tok::LBrace, "'{'")?;
        let stmts = self.parse_stmts();
        Some(Scene { number, title, set, stmts, pos })
    }

    fn parse_stmts(&mut self) -> Vec<Stmt> {
        let mut v = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek() {
                Tok::RBrace => {
                    self.bump();
                    break;
                }
                Tok::Eof => {
                    let pos = self.pos();
                    self.err("E0106", pos, "missing closing '}' for the scene", Some("add a '}' after the last statement".into()));
                    break;
                }
                _ => {
                    if let Some(s) = self.parse_stmt() {
                        v.push(s);
                    }
                }
            }
        }
        v
    }

    fn parse_stmt(&mut self) -> Option<Stmt> {
        let pos = self.pos();
        let first = match self.peek().clone() {
            Tok::Ident(s) => s,
            other => {
                self.err("E0102", pos, format!("expected a statement, found {}", describe(&other)), None);
                self.skip_to_eol();
                return None;
            }
        };
        let kind = match first.as_str() {
            "together" if matches!(self.peek_at(1), Tok::LBrace) => {
                self.bump();
                self.bump();
                let inner = self.parse_stmts();
                return Some(Stmt { pos, kind: StmtKind::Together(inner) });
            }
            "place" => self.parse_place(),
            "camera" => {
                self.bump();
                self.parse_camera().map(StmtKind::Camera)
            }
            "cut" => {
                self.bump();
                if !self.is_word("to") {
                    let p = self.pos();
                    self.err("E0101", p, "expected 'to' after 'cut'", Some("write: cut to medium on Marta".into()));
                    None
                } else {
                    self.bump();
                    self.parse_camera().map(StmtKind::Camera)
                }
            }
            "sfx" => {
                self.bump();
                self.parse_sfx()
            }
            "music" => {
                self.bump();
                self.parse_music()
            }
            "wait" => {
                self.bump();
                self.expect_dur().map(StmtKind::Wait)
            }
            "give" => {
                self.bump();
                let who = self.expect_word("a character");
                let item = self.expect_word("an item");
                match (who, item) {
                    (Some(who), Some(item)) => Some(StmtKind::Give { who, item }),
                    _ => None,
                }
            }
            "wound" => {
                self.bump();
                let who = self.expect_word("a character");
                let part = self.expect_word("a body part");
                let state = self.expect_word("a state (e.g. bandaged)");
                match (who, part, state) {
                    (Some(who), Some(part), Some(state)) => Some(StmtKind::Wound { who, part, state }),
                    _ => None,
                }
            }
            "relation" => {
                self.bump();
                let from = self.expect_word("a character");
                if self.expect_tok(Tok::Arrow, "'->'").is_none() {
                    None
                } else {
                    let to = self.expect_word("a character");
                    if self.expect_tok(Tok::Equals, "'='").is_none() {
                        None
                    } else {
                        let value = self.expect_word("a relation word");
                        match (from, to, value) {
                            (Some(from), Some(to), Some(value)) => Some(StmtKind::Relation { from, to, value }),
                            _ => None,
                        }
                    }
                }
            }
            "remember" => {
                self.bump();
                let who = self.expect_word("a character");
                let text = self.expect_str("a memory");
                match (who, text) {
                    (Some(who), Some(text)) => Some(StmtKind::Remember { who, text }),
                    _ => None,
                }
            }
            _ => match self.peek_at(1).clone() {
                Tok::Colon => {
                    self.bump();
                    self.bump();
                    self.parse_value().map(|v| StmtKind::Prop(first.clone(), v))
                }
                Tok::LParen => self.parse_dialogue(),
                Tok::Equals => {
                    self.bump();
                    self.bump();
                    self.parse_value().map(|v| StmtKind::Assign { target: first.clone(), value: v })
                }
                _ => return self.parse_action_line(pos),
            },
        };
        match kind {
            Some(k) => {
                self.end_stmt();
                Some(Stmt { pos, kind: k })
            }
            None => {
                self.skip_to_eol();
                None
            }
        }
    }

    fn parse_place(&mut self) -> Option<StmtKind> {
        self.bump(); // place
        let entity = self.expect_word("a character or prop name")?;
        let rel = self.expect_word("'at' or a relation like 'beside'")?;
        let target = self.expect_word("an anchor or entity")?;
        let mut dist = None;
        let mut facing = None;
        loop {
            match self.peek().clone() {
                Tok::Num(n, _) => {
                    self.bump();
                    dist = Some(n);
                    if self.is_word("away") {
                        self.bump();
                    }
                }
                Tok::Ident(w) if w == "facing" => {
                    self.bump();
                    facing = Some(self.expect_word("a target to face")?);
                }
                _ => break,
            }
        }
        Some(StmtKind::Place(Place { entity, rel, target, dist, facing }))
    }

    fn parse_camera(&mut self) -> R<CameraStmt> {
        let mut c = CameraStmt::default();
        while !self.at_eol() {
            let pos = self.pos();
            match self.peek().clone() {
                Tok::Ident(w) => {
                    self.bump();
                    match w.as_str() {
                        "on" => c.on = Some(self.expect_word("a character after 'on'")?),
                        "from" => c.from = Some(self.expect_word("a side or anchor after 'from'")?),
                        "angle" => c.angle = Some(self.expect_word("an angle after 'angle'")?),
                        "move" => c.mv = Some(self.expect_word("a camera move after 'move'")?),
                        "lens" => {
                            if let Tok::Num(n, _) = self.peek().clone() {
                                self.bump();
                                c.lens = Some(n);
                            } else {
                                self.err("E0101", pos, "expected a number after 'lens' (millimeters)", Some("write: lens 35".into()));
                                return None;
                            }
                        }
                        "for" => c.dur = Some(self.expect_dur()?),
                        "over_shoulder" => {
                            c.words.push(w.clone());
                            let a = self.expect_word("a character after 'over_shoulder'")?;
                            self.expect_tok(Tok::Arrow, "'->'")?;
                            let b = self.expect_word("the character being looked at")?;
                            c.ots = Some((a, b));
                        }
                        _ => c.words.push(w),
                    }
                }
                other => {
                    self.err("E0104", pos, format!("unexpected {} in camera statement", describe(&other)), None);
                    return None;
                }
            }
        }
        Some(c)
    }

    fn parse_sfx(&mut self) -> Option<StmtKind> {
        let name = match self.peek().clone() {
            Tok::Ident(s) | Tok::Str(s) => {
                self.bump();
                s
            }
            other => {
                let pos = self.pos();
                self.err("E0101", pos, format!("expected a sound name, found {}", describe(&other)), None);
                return None;
            }
        };
        let mut at = None;
        let mut volume = None;
        while !self.at_eol() {
            let pos = self.pos();
            match self.word().as_deref() {
                Some("at") => {
                    self.bump();
                    at = Some(self.expect_word("an anchor")?);
                }
                Some("volume") => {
                    self.bump();
                    if let Tok::Num(n, _) = self.peek().clone() {
                        self.bump();
                        volume = Some(n);
                    } else {
                        self.err("E0101", pos, "expected a number after 'volume'", None);
                        return None;
                    }
                }
                _ => {
                    self.err("E0104", pos, format!("unexpected {} in sfx statement", describe(self.peek())), None);
                    return None;
                }
            }
        }
        Some(StmtKind::Sfx(Sfx { name, at, volume }))
    }

    fn parse_music(&mut self) -> Option<StmtKind> {
        let name = match self.peek().clone() {
            Tok::Ident(s) | Tok::Str(s) => {
                self.bump();
                s
            }
            other => {
                let pos = self.pos();
                self.err("E0101", pos, format!("expected a music name, found {}", describe(&other)), None);
                return None;
            }
        };
        let stop = name == "stop";
        let name = if stop { String::new() } else { name };
        let mut volume = None;
        let mut fade_in = None;
        let mut fade_out = None;
        let mut once = false;
        while !self.at_eol() {
            let pos = self.pos();
            match self.word().as_deref() {
                Some("volume") => {
                    self.bump();
                    if let Tok::Num(n, _) = self.peek().clone() {
                        self.bump();
                        volume = Some(n);
                    } else {
                        self.err("E0101", pos, "expected a number after 'volume'", None);
                        return None;
                    }
                }
                Some("fade_in") => {
                    self.bump();
                    fade_in = Some(self.expect_dur()?);
                }
                Some("fade_out") => {
                    self.bump();
                    fade_out = Some(self.expect_dur()?);
                }
                Some("once") => {
                    self.bump();
                    once = true;
                }
                _ => {
                    self.err("E0104", pos, format!("unexpected {} in music statement", describe(self.peek())), Some("music accepts: volume N, fade_in Ns, fade_out Ns, once".into()));
                    return None;
                }
            }
        }
        Some(StmtKind::Music(Music { name, volume, fade_in, fade_out, once, stop }))
    }

    fn parse_dialogue(&mut self) -> Option<StmtKind> {
        let who = self.expect_word("a character")?;
        self.expect_tok(Tok::LParen, "'('")?;
        let emotion = self.expect_word("an emotion")?;
        self.expect_tok(Tok::RParen, "')'")?;
        self.expect_tok(Tok::Colon, "':'")?;
        let text = self.expect_str("the spoken line")?;
        let mut audio = None;
        if self.is_word("audio") {
            self.bump();
            audio = Some(self.expect_str("an audio file path")?);
        }
        Some(StmtKind::Dialogue(Dialogue { who, emotion, text, audio }))
    }

    fn parse_action_line(&mut self, pos: Pos) -> Option<Stmt> {
        let mut subject = self.expect_word("a character")?;
        let mut parts: Vec<Stmt> = Vec::new();
        loop {
            let vpos = self.pos();
            let verb = match self.word() {
                Some(v) => {
                    self.bump();
                    v
                }
                None => {
                    self.err(
                        "E0101",
                        vpos,
                        format!("expected an action verb after '{}', found {}", subject, describe(self.peek())),
                        Some("write e.g.: Marta walks_to diner.counter".into()),
                    );
                    self.skip_to_eol();
                    return None;
                }
            };
            let mut args = Vec::new();
            let mut dur = None;
            while !self.at_eol() && !self.is_word("and") {
                let apos = self.pos();
                match self.peek().clone() {
                    Tok::Ident(w) if w == "for" && matches!(self.peek_at(1), Tok::Num(_, _)) => {
                        self.bump();
                        match self.expect_dur() {
                            Some(d) => dur = Some(d),
                            None => {
                                self.skip_to_eol();
                                return None;
                            }
                        }
                    }
                    Tok::Ident(w) => {
                        self.bump();
                        args.push(Arg::Word(w));
                    }
                    Tok::Num(n, u) => {
                        self.bump();
                        args.push(Arg::Num(n, u));
                    }
                    Tok::Str(s) => {
                        self.bump();
                        args.push(Arg::Str(s));
                    }
                    Tok::LParen => {
                        self.bump();
                        let x = match self.peek().clone() {
                            Tok::Num(n, _) => {
                                self.bump();
                                n
                            }
                            _ => {
                                self.err("E0101", apos, "expected a number after '('", Some("write a floor point like (1.5, 2.0)".into()));
                                self.skip_to_eol();
                                return None;
                            }
                        };
                        if self.expect_tok(Tok::Comma, "','").is_none() {
                            self.skip_to_eol();
                            return None;
                        }
                        let z = match self.peek().clone() {
                            Tok::Num(n, _) => {
                                self.bump();
                                n
                            }
                            _ => {
                                self.err("E0101", apos, "expected a second number in the point", Some("write a floor point like (1.5, 2.0)".into()));
                                self.skip_to_eol();
                                return None;
                            }
                        };
                        if self.expect_tok(Tok::RParen, "')'").is_none() {
                            self.skip_to_eol();
                            return None;
                        }
                        args.push(Arg::Point(x, z));
                    }
                    other => {
                        self.err("E0104", apos, format!("unexpected {} in action", describe(&other)), None);
                        self.skip_to_eol();
                        return None;
                    }
                }
            }
            parts.push(Stmt {
                pos: vpos,
                kind: StmtKind::Action(Action { subject: subject.clone(), verb, args, dur }),
            });
            if self.is_word("and") {
                self.bump();
                // 'and Blob shrugs' introduces a new subject; 'and shrugs' reuses the previous one.
                if let Some(w) = self.word() {
                    let upper = w.chars().next().map_or(false, |c| c.is_uppercase());
                    if upper && matches!(self.peek_at(1), Tok::Ident(_)) {
                        self.bump();
                        subject = w;
                    }
                }
                continue;
            }
            break;
        }
        self.end_stmt();
        if parts.len() == 1 {
            parts.pop()
        } else {
            Some(Stmt { pos, kind: StmtKind::Together(parts) })
        }
    }
}
