//! `oneiro new <dir>`: scaffolds a project that compiles and renders out of the box.

use oneiro_core::scaffold::{create_project, Template};
use std::path::Path;

const USAGE: &str = "usage: oneiro new <dir> [--name NAME] [--template starter|empty]";

pub fn cmd_new(args: &[String]) -> Result<bool, String> {
    let (mut dir, mut name, mut template) = (None::<&str>, None::<String>, Template::Starter);
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--name" => {
                i += 1;
                name = Some(args.get(i).ok_or("--name needs a value")?.clone());
            }
            "--template" => {
                i += 1;
                template = match args.get(i).map(|s| s.as_str()) {
                    Some("starter") => Template::Starter,
                    Some("empty") => Template::Empty,
                    _ => return Err(format!("--template is starter or empty\n{}", USAGE)),
                };
            }
            a if !a.starts_with('-') && dir.is_none() => dir = Some(a),
            a => return Err(format!("unexpected argument '{}'\n{}", a, USAGE)),
        }
        i += 1;
    }
    let dir = Path::new(dir.ok_or(USAGE)?);
    let name = name.unwrap_or_else(|| dir.file_name().map(|n| n.to_string_lossy().replace(['-', '_'], " ")).unwrap_or_else(|| "My Series".into()));
    create_project(dir, &name, template)?;
    println!("created project '{}' in {}", name, dir.display());
    println!("next: oneiro check {d}   |   oneiro render {d}", d = dir.display());
    Ok(true)
}
