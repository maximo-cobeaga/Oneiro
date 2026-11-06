//! AST for Oneiro files. Definition files (characters, sets, styles, locks) use the generic
//! `Block`/`Entry`/`Value` tree; scripts additionally have `Scene`s with typed statements.

use crate::diag::Pos;

#[derive(Clone, Debug)]
pub enum Value {
    Str(String),
    Num(f64, String),
    Color(String),
    Ident(String),
    List(Vec<Value>),
    Tuple(Vec<Value>),
    /// `8m x 6m`
    Dim(Vec<Value>),
    Map(Vec<Entry>),
    /// `library.heads.round { size: 1.1 }`
    Call(String, Vec<Entry>),
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub key: String,
    pub name: Option<String>,
    pub body: EntryBody,
    pub pos: Pos,
}

#[derive(Clone, Debug)]
pub enum EntryBody {
    Value(Value),
    Block(Vec<Entry>),
}

#[derive(Clone, Debug)]
pub struct Block {
    pub kind: String,
    pub name: Option<String>,
    pub entries: Vec<Entry>,
    pub pos: Pos,
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) | Value::Ident(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_num(&self) -> Option<f64> {
        match self {
            Value::Num(n, _) => Some(*n),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Ident(s) if s == "true" => Some(true),
            Value::Ident(s) if s == "false" => Some(false),
            _ => None,
        }
    }
    /// Numbers of a tuple/list/dim.
    pub fn nums(&self) -> Option<Vec<f64>> {
        match self {
            Value::Tuple(v) | Value::List(v) | Value::Dim(v) => {
                v.iter().map(|x| x.as_num()).collect()
            }
            Value::Num(n, _) => Some(vec![*n]),
            _ => None,
        }
    }
    pub fn words(&self) -> Vec<String> {
        match self {
            Value::List(v) | Value::Tuple(v) => {
                v.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()
            }
            v => v.as_str().map(|s| vec![s.to_string()]).unwrap_or_default(),
        }
    }
}

impl Entry {
    pub fn value(&self) -> Option<&Value> {
        match &self.body {
            EntryBody::Value(v) => Some(v),
            _ => None,
        }
    }
    pub fn block(&self) -> Option<&[Entry]> {
        match &self.body {
            EntryBody::Block(b) => Some(b),
            _ => None,
        }
    }
}

pub fn entry<'a>(entries: &'a [Entry], key: &str) -> Option<&'a Entry> {
    entries.iter().find(|e| e.key == key)
}

pub fn dur_seconds(n: f64, unit: &str) -> Option<f64> {
    match unit {
        "" | "s" => Some(n),
        "ms" => Some(n / 1000.0),
        _ => None,
    }
}

// ---------------------------------------------------------------- script

#[derive(Clone, Debug, Default)]
pub struct File {
    pub version: Option<String>,
    pub project: Option<String>,
    pub default_style: Option<String>,
    pub uses: Vec<Use>,
    pub blocks: Vec<Block>,
    pub scenes: Vec<Scene>,
}

#[derive(Clone, Debug)]
pub struct Use {
    pub kind: String,
    pub name: String,
    /// `use character Marta style rubber_hose`: the style of this character in this episode.
    pub style: Option<String>,
    pub pos: Pos,
}

#[derive(Clone, Debug)]
pub struct Scene {
    pub number: u32,
    pub title: String,
    pub set: String,
    pub stmts: Vec<Stmt>,
    pub pos: Pos,
}

#[derive(Clone, Debug)]
pub struct Stmt {
    pub pos: Pos,
    pub kind: StmtKind,
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    Prop(String, Value),
    Place(Place),
    Camera(CameraStmt),
    Action(Action),
    Dialogue(Dialogue),
    Sfx(Sfx),
    Music(Music),
    Wait(f64),
    /// `transition dissolve for 1s`: applies to the next camera change.
    Transition { kind: String, dur: Option<f64> },
    Together(Vec<Stmt>),
    Give { who: String, item: String },
    Wound { who: String, part: String, state: String },
    Relation { from: String, to: String, value: String },
    Remember { who: String, text: String },
    Assign { target: String, value: Value },
}

#[derive(Clone, Debug)]
pub struct Place {
    pub entity: String,
    /// `at` or a relation such as `beside`.
    pub rel: String,
    pub target: String,
    pub dist: Option<f64>,
    pub facing: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct CameraStmt {
    pub words: Vec<String>,
    pub on: Option<String>,
    pub from: Option<String>,
    pub angle: Option<String>,
    pub mv: Option<String>,
    pub lens: Option<f64>,
    pub dur: Option<f64>,
    pub ots: Option<(String, String)>,
}

#[derive(Clone, Debug)]
pub enum Arg {
    Word(String),
    Num(f64, String),
    Str(String),
    /// A floor coordinate `(x, z)` in meters.
    Point(f64, f64),
}

#[derive(Clone, Debug)]
pub struct Action {
    pub subject: String,
    pub verb: String,
    pub args: Vec<Arg>,
    pub dur: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct Dialogue {
    pub who: String,
    pub emotion: String,
    pub text: String,
    pub audio: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Sfx {
    pub name: String,
    pub at: Option<String>,
    pub volume: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct Music {
    /// Track name; empty for `music stop`.
    pub name: String,
    pub volume: Option<f64>,
    pub fade_in: Option<f64>,
    /// Fade used when this track is stopped or replaced.
    pub fade_out: Option<f64>,
    /// `once`: play a single time instead of looping.
    pub once: bool,
    /// `music stop`: fades out whatever music is playing.
    pub stop: bool,
}
