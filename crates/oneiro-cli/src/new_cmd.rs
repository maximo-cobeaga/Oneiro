//! `oneiro new <dir>`: scaffolds a project that compiles and renders out of the box.

use std::path::Path;

const MARTA: &str = include_str!("../../../examples/first_short/library/characters/Marta.onchar");
const BLOB: &str = include_str!("../../../examples/first_short/library/characters/Blob.onchar");
const DINER: &str = include_str!("../../../examples/first_short/library/sets/diner.onset");

fn script(name: &str) -> String {
    format!(
        "oneiro 0.1\nproject \"{name}\"\nstyle default: cutout_2d\n\nuse character Marta\nuse character Blob\nuse set diner\n\n# Your first scene. Run `oneiro check .` after every change and `oneiro render .` to get a video.\nscene 1 \"Opening\" in diner {{\n  time: night\n  place Marta at diner.stool_2 facing diner.counter\n  place Blob at diner.door\n\n  camera establishing wide from south move dolly_in for 3s\n  Blob walks_to diner.counter\n\n  transition dissolve for 0.8s\n  cut to medium on Marta\n  Marta (annoyed): \"Is this thing on?\"\n  wait 0.5s\n}}\n"
    )
}

pub fn cmd_new(args: &[String]) -> Result<bool, String> {
    let mut dir: Option<&str> = None;
    let mut name: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--name" => {
                i += 1;
                name = Some(args.get(i).ok_or("--name needs a value")?.clone());
            }
            a if !a.starts_with('-') && dir.is_none() => dir = Some(a),
            a => return Err(format!("unexpected argument '{}'\nusage: oneiro new <dir> [--name \"My Series\"]", a)),
        }
        i += 1;
    }
    let dir = Path::new(dir.ok_or("usage: oneiro new <dir> [--name \"My Series\"]")?);
    if dir.join("oneiro.toml").exists() {
        return Err(format!("{} already holds an Oneiro project", dir.display()));
    }
    let name = name.unwrap_or_else(|| dir.file_name().map(|n| n.to_string_lossy().replace(['-', '_'], " ")).unwrap_or_else(|| "My Series".into())).replace('"', "'");
    for (rel, text) in [
        ("oneiro.toml", format!("# Oneiro project manifest\nname = \"{name}\"\ndefault_style = \"cutout_2d\"\nfps = 24\nwidth = 1920\nheight = 1080\n")),
        ("script.onr", script(&name)),
        ("library/characters/Marta.onchar", MARTA.to_string()),
        ("library/characters/Blob.onchar", BLOB.to_string()),
        ("library/sets/diner.onset", DINER.to_string()),
    ] {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| format!("{}: {}", p.display(), e))?;
        std::fs::write(&p, text).map_err(|e| format!("{}: {}", p.display(), e))?;
    }
    println!("created project '{}' in {}", name, dir.display());
    println!("next: oneiro check {d}   |   oneiro render {d}", d = dir.display());
    Ok(true)
}
