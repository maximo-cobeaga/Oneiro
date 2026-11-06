//! `oneiro cut`: the hybrid cutter from the command line.

use oneiro_core::project::locate;
use oneiro_cutter::{cut, extract_mask, overlay, propose, validate, write_character, Image, Layout, WriteOpts};
use std::path::Path;

struct Args {
    image: String,
    project: String,
    name: Option<String>,
    layout: Option<String>,
    overlay: Option<String>,
    height: f64,
    style: String,
    tolerance: Option<f32>,
    ai: bool,
    cfg: oneiro_ai::AiConfig,
    json: bool,
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut a = Args { image: String::new(), project: ".".into(), name: None, layout: None, overlay: None, height: 1.15, style: String::new(), tolerance: None, ai: false, cfg: oneiro_ai::AiConfig::load(), json: false };
    let mut pos: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let s = args[i].clone();
        let mut val = |n: &str| -> Result<String, String> {
            i += 1;
            args.get(i).cloned().ok_or(format!("{} needs a value", n))
        };
        match s.as_str() {
            "--name" => a.name = Some(val("--name")?),
            "--layout" => a.layout = Some(val("--layout")?),
            "--overlay" => a.overlay = Some(val("--overlay")?),
            "--height" => a.height = val("--height")?.parse().map_err(|_| "--height must be a number (meters)".to_string())?,
            "--style" => a.style = val("--style")?,
            "--tolerance" => a.tolerance = Some(val("--tolerance")?.parse().map_err(|_| "--tolerance must be a number".to_string())?),
            "--ai" => a.ai = true,
            "--json" => a.json = true,
            "--provider" => {
                let v = val("--provider")?;
                a.cfg.kind = oneiro_ai::ProviderKind::from_id(&v).ok_or(format!("unknown provider '{}'", v))?;
            }
            "--model" => a.cfg.model = val("--model")?,
            "--base-url" => a.cfg.base_url = val("--base-url")?,
            "--key-env" => a.cfg.api_key_env = val("--key-env")?,
            x if x.starts_with("--") => return Err(format!("unknown option {}", x)),
            _ => pos.push(s),
        }
        i += 1;
    }
    a.image = pos.first().cloned().ok_or("usage: oneiro cut <image> [project] [--name Name] [--layout file.json|JSON] [--ai] [--overlay out.png] [--height 1.15] [--style clay]")?;
    if let Some(p) = pos.get(1) {
        a.project = p.clone();
    }
    Ok(a)
}

/// Layout from a file path or an inline JSON string.
fn read_layout(spec: &str) -> Result<Layout, String> {
    let text = if spec.trim_start().starts_with('{') { spec.to_string() } else { std::fs::read_to_string(spec).map_err(|e| format!("{}: {}", spec, e))? };
    Layout::from_json(&text)
}

pub fn cmd_cut(args: &[String]) -> Result<bool, String> {
    let a = parse(args)?;
    let img = Image::load(Path::new(&a.image))?;
    let mask = extract_mask(&img, a.tolerance)?;
    let (x0, y0, x1, y1) = mask.bounds().unwrap();
    eprintln!("image {}x{}, character found at ({}, {})-({}, {})", img.width(), img.height(), x0, y0, x1, y1);

    let mut layout = match &a.layout {
        Some(spec) => read_layout(spec)?,
        None => propose(&mask)?,
    };
    if a.ai {
        let provider = a.cfg.build_provider(None)?;
        let opts = oneiro_ai::DirectorOpts { max_rounds: a.cfg.max_rounds, ..Default::default() };
        eprintln!("provider: {}", provider.name());
        let r = oneiro_ai::cutter_ai::propose_with_ai(provider.as_ref(), &img, &mask, Some(&layout), &opts, &mut |m| eprintln!("  {}", m))?;
        eprintln!("{} model call(s){}", r.rounds, if r.valid { "" } else { " (the model's last proposal still has problems)" });
        layout = r.layout;
    }

    let issues = validate(&layout, &mask);
    if a.json {
        println!("{}", layout.to_json());
    }
    for i in &issues {
        eprintln!("{}", i.render());
    }
    if let Some(out) = &a.overlay {
        let cx = Some(mask.axis());
        let png = overlay::draw(&img, &layout, cx).encode_png().map_err(|e| e.to_string())?;
        std::fs::write(out, png).map_err(|e| format!("{}: {}", out, e))?;
        eprintln!("wrote {} (1 head, 2 body, 3 arm, 4 leg)", out);
    }
    let errors = issues.iter().filter(|i| i.is_error()).count();
    let Some(name) = &a.name else {
        if !a.json {
            println!("{}", layout.to_json());
        }
        eprintln!("{} error(s), {} warning(s). Add --name <Name> to cut and save the character.", errors, issues.len() - errors);
        return Ok(errors == 0);
    };
    if errors > 0 {
        eprintln!("not saved: fix the {} error(s) (edit the layout and pass it with --layout)", errors);
        return Ok(false);
    }
    let (root, _) = locate(Path::new(&a.project))?;
    let pieces = cut(&img, &mask, &layout)?;
    let opts = WriteOpts { height_m: a.height, style: a.style.clone(), source: Path::new(&a.image).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default() };
    let path = write_character(&root, name, &pieces, &opts)?;
    eprintln!("saved {} and 4 PNG parts in library/parts/{}/", path.display(), name);
    eprintln!("use it with: use character {}", name);
    Ok(true)
}
