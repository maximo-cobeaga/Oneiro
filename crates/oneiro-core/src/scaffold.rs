//! Creating projects, sets and episodes from templates (shared by `oneiro new` and the IDE).

use std::path::{Path, PathBuf};

const MARTA: &str = include_str!("../../../examples/first_short/library/characters/Marta.onchar");
const BLOB: &str = include_str!("../../../examples/first_short/library/characters/Blob.onchar");
const DINER: &str = include_str!("../../../examples/first_short/library/sets/diner.onset");
const VILLAGE: &str = include_str!("../../../examples/village/library/sets/village.onset");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Template {
    /// Two characters, a diner and a short scene that renders out of the box.
    Starter,
    /// A bare room and an empty scene: you make the characters yourself.
    Empty,
}

impl Template {
    pub const ALL: [Template; 2] = [Template::Starter, Template::Empty];
    pub fn label(self) -> &'static str {
        match self {
            Template::Starter => "Starter - two characters, a diner and a short scene",
            Template::Empty => "Empty - a bare room, you create the characters",
        }
    }
}

/// A name usable as a file name and an identifier in scripts.
pub fn valid_name(n: &str) -> bool {
    let mut c = n.chars();
    matches!(c.next(), Some(f) if f.is_ascii_alphabetic()) && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && n.len() <= 40
}

/// Folder name for a project title: "My Great Show" -> "My-Great-Show".
pub fn folder_name(title: &str) -> String {
    let s: String = title.trim().chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '-' }).collect();
    s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-")
}

pub fn room_source(name: &str) -> String {
    format!(
        "# A bare room. Coordinates are meters: x east, z north, origin at the center of the floor.\nset {name} {{\n  size: 8m x 6m\n  height: 3m\n\n  palette {{\n    wall: #DCE6EE\n    wall_alt: #CFDBE6\n    floor: #B9A88C\n    floor_alt: #A8977B\n    trim: #4A6FA5\n  }}\n\n  anchors {{\n    start: (-3.0, -1.0)\n    center: (0.0, 0.0)\n  }}\n\n  props {{\n    window_1: library.props.window {{ wall: north, at: -1.5 }}\n    door:     library.props.door   {{ wall: west, at: -1.0 }}\n    table_1:  library.props.table  {{ at: (2.2, 0.8) }}\n  }}\n}}\n"
    )
}

/// An open 3D village: houses, trees, a well and far hills.
pub fn village_source(name: &str) -> String {
    VILLAGE.replacen("set village {", &format!("set {} {{", name), 1)
}

/// An open field: sky, grass, a few trees and rocks, and room to build.
pub fn field_source(name: &str) -> String {
    format!(
        "# An open field in 3D. Add props (tree, pine, bush, rock, house, fence, bench...) and solids (box, cylinder, cone, sphere, roof, pyramid).\nset {name} {{\n  kind: open\n  size: 16m x 12m\n  sky: [#5BC0F8, #FFEFC4]\n  ground: #8ED081\n\n  anchors {{\n    start: (-5.0, -2.0)\n    center: (0.0, 0.0)\n  }}\n\n  props {{\n    tree_1: library.props.tree {{ at: (-7.0, 3.0) }}\n    tree_2: library.props.tree {{ at: (7.5, 4.0), height: 4.4 }}\n    pine_1: library.props.pine {{ at: (-9.0, -1.0) }}\n    bush_1: library.props.bush {{ at: (3.0, 3.0) }}\n    rock_1: library.props.rock {{ at: (5.0, -3.0) }}\n  }}\n\n  solids {{\n    hill_1: {{ shape: sphere, at: (-6.0, 20.0), size: (24.0, 9.0, 12.0), color: #6CBF84 }}\n    hill_2: {{ shape: sphere, at: (10.0, 22.0), size: (26.0, 12.0, 12.0), color: #58B070 }}\n  }}\n}}\n"
    )
}

pub fn diner_source(name: &str) -> String {
    DINER.replacen("set diner {", &format!("set {} {{", name), 1)
}

/// A new episode script that uses the given characters and set (empty cast is fine).
pub fn episode_source(title: &str, project: &str, characters: &[String], set: &str) -> String {
    let mut s = format!("oneiro 0.1\nproject \"{}\"\nstyle default: cutout_2d\n\n", project.replace('"', "'"));
    for c in characters {
        s.push_str(&format!("use character {}\n", c));
    }
    s.push_str(&format!("use set {}\n\n", set));
    s.push_str(&format!("scene 1 \"{}\" in {} {{\n  time: day\n", title.replace('"', "'"), set));
    for (i, c) in characters.iter().enumerate() {
        s.push_str(&format!("  place {} at {}.{}\n", c, set, if i == 0 { "start" } else { "center" }));
    }
    s.push_str("  camera wide\n");
    if let Some(c) = characters.first() {
        s.push_str(&format!("  {} (happy): \"Hello, world.\"\n", c));
    }
    s.push_str("  wait 1s\n}\n");
    s
}

fn manifest(name: &str) -> String {
    format!("# Oneiro project manifest\nname = \"{}\"\ndefault_style = \"cutout_2d\"\nfps = 24\nwidth = 1920\nheight = 1080\n", name.replace('"', "'"))
}

/// Writes a new project into `dir` (created if needed). Refuses to touch an existing project.
pub fn create_project(dir: &Path, name: &str, template: Template) -> Result<PathBuf, String> {
    if dir.join("oneiro.toml").exists() {
        return Err(format!("{} already holds an Oneiro project", dir.display()));
    }
    let mut files: Vec<(&str, String)> = vec![("oneiro.toml", manifest(name))];
    match template {
        Template::Starter => {
            files.push(("script.onr", starter_script(name)));
            files.push(("library/characters/Marta.onchar", MARTA.to_string()));
            files.push(("library/characters/Blob.onchar", BLOB.to_string()));
            files.push(("library/sets/diner.onset", DINER.to_string()));
        }
        Template::Empty => {
            files.push(("script.onr", episode_source("Opening", name, &[], "room")));
            files.push(("library/sets/room.onset", room_source("room")));
        }
    }
    for (rel, text) in files {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| format!("{}: {}", p.display(), e))?;
        std::fs::write(&p, text).map_err(|e| format!("{}: {}", p.display(), e))?;
    }
    Ok(dir.to_path_buf())
}

fn starter_script(name: &str) -> String {
    format!(
        "oneiro 0.1\nproject \"{}\"\nstyle default: cutout_2d\n\nuse character Marta\nuse character Blob\nuse set diner\n\n# Your first scene. Press Ctrl+S to save; errors show up as you type.\nscene 1 \"Opening\" in diner {{\n  time: night\n  place Marta at diner.stool_2 facing diner.counter\n  place Blob at diner.door\n\n  camera establishing wide from south move dolly_in for 3s\n  Blob walks_to diner.counter\n\n  transition dissolve for 0.8s\n  cut to medium on Marta\n  Marta (annoyed): \"Is this thing on?\"\n  wait 0.5s\n}}\n",
        name.replace('"', "'")
    )
}
