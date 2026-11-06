//! Project discovery and library loading.

use crate::defs::{CharacterDef, SetDef};
use oneiro_lang::{parse, Diagnostic};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Manifest {
    pub name: String,
    pub default_style: String,
    pub fps: u32,
    pub width: u32,
    pub height: u32,
    /// Music level drop under speech, in dB (0 disables ducking).
    pub duck_db: f32,
}

impl Default for Manifest {
    fn default() -> Self {
        Manifest { name: "Untitled".into(), default_style: "cutout_2d".into(), fps: 24, width: 1920, height: 1080, duck_db: -9.0 }
    }
}

#[derive(Clone)]
pub struct Project {
    pub root: PathBuf,
    pub manifest: Manifest,
    pub characters: BTreeMap<String, CharacterDef>,
    pub sets: BTreeMap<String, SetDef>,
}

/// Finds (project root, script path) from a directory or a script file.
pub fn locate(path: &Path) -> Result<(PathBuf, PathBuf), String> {
    if path.is_dir() {
        let script = path.join("script.onr");
        if script.exists() {
            return Ok((path.to_path_buf(), script));
        }
        // episodes/<name>/script.onr
        let eps = path.join("episodes");
        if let Ok(rd) = std::fs::read_dir(&eps) {
            let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
            dirs.sort();
            for d in dirs {
                let s = d.join("script.onr");
                if s.exists() {
                    return Ok((path.to_path_buf(), s));
                }
            }
        }
        return Err(format!("no script.onr found in {}", path.display()));
    }
    if !path.exists() {
        return Err(format!("{} does not exist", path.display()));
    }
    let abs = path.canonicalize().map_err(|e| e.to_string())?;
    let mut cur = abs.parent().map(|p| p.to_path_buf());
    while let Some(d) = cur {
        if d.join("oneiro.toml").exists() {
            return Ok((d, abs));
        }
        cur = d.parent().map(|p| p.to_path_buf());
    }
    Ok((abs.parent().unwrap_or(Path::new(".")).to_path_buf(), abs))
}

fn parse_manifest(text: &str) -> Manifest {
    let mut m = Manifest::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            let v = v.trim().trim_matches('"').to_string();
            match k {
                "name" => m.name = v,
                "default_style" => m.default_style = v,
                "fps" => m.fps = v.parse().unwrap_or(24),
                "width" => m.width = v.parse().unwrap_or(1920),
                "height" => m.height = v.parse().unwrap_or(1080),
                "duck_db" => m.duck_db = v.parse().unwrap_or(-9.0),
                _ => {}
            }
        }
    }
    m
}

impl Project {
    pub fn load(root: &Path, diags: &mut Vec<Diagnostic>) -> Project {
        let manifest = std::fs::read_to_string(root.join("oneiro.toml"))
            .map(|t| parse_manifest(&t))
            .unwrap_or_else(|_| {
                let mut m = Manifest::default();
                if let Some(n) = root.file_name() {
                    m.name = n.to_string_lossy().to_string();
                }
                m
            });
        let mut p = Project { root: root.to_path_buf(), manifest, characters: BTreeMap::new(), sets: BTreeMap::new() };
        p.scan("library/characters", "onchar", diags);
        p.scan("library/sets", "onset", diags);
        p
    }

    fn scan(&mut self, sub: &str, ext: &str, diags: &mut Vec<Diagnostic>) {
        let dir = self.root.join(sub);
        let Ok(rd) = std::fs::read_dir(&dir) else { return };
        let mut files: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().map_or(false, |e| e == ext))
            .collect();
        files.sort();
        for f in files {
            let label = f.strip_prefix(&self.root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
            let Ok(src) = std::fs::read_to_string(&f) else { continue };
            let (file, mut d) = parse(&src, &label);
            diags.append(&mut d);
            for b in &file.blocks {
                match b.kind.as_str() {
                    "character" => {
                        let c = CharacterDef::from_block(b, &label, diags);
                        self.characters.insert(c.name.clone(), c);
                    }
                    "set" => {
                        let s = SetDef::from_block(b, &label, diags);
                        self.sets.insert(s.name.clone(), s);
                    }
                    _ => {}
                }
            }
        }
    }
}
