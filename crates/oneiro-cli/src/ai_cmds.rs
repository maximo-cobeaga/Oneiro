//! `oneiro ai`, `oneiro ai-test` and `oneiro guide`.

use oneiro_core::lock::Lock;
use oneiro_core::project::locate;
use oneiro_core::Project;
use std::path::Path;
use std::time::Instant;

struct AiArgs {
    project: String,
    request: String,
    cfg: oneiro_ai::AiConfig,
    apply: bool,
    character: bool,
}

fn parse_ai_args(args: &[String], need_request: bool) -> Result<AiArgs, String> {
    let mut a = AiArgs { project: String::new(), request: String::new(), cfg: oneiro_ai::AiConfig::load(), apply: false, character: false };
    let mut positional: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let s = args[i].clone();
        let mut val = |name: &str| -> Result<String, String> {
            i += 1;
            args.get(i).cloned().ok_or(format!("{} needs a value", name))
        };
        match s.as_str() {
            "--provider" => {
                let v = val("--provider")?;
                a.cfg.kind = oneiro_ai::ProviderKind::from_id(&v)
                    .ok_or(format!("unknown provider '{}' (use claude-cli, codex-cli, anthropic, openai, custom-cli)", v))?;
            }
            "--model" => a.cfg.model = val("--model")?,
            "--base-url" => a.cfg.base_url = val("--base-url")?,
            "--key-env" => a.cfg.api_key_env = val("--key-env")?,
            "--command" => a.cfg.custom_command = val("--command")?,
            "--rounds" => a.cfg.max_rounds = val("--rounds")?.parse().map_err(|_| "--rounds must be a number".to_string())?,
            "--vision" => a.cfg.vision = true,
            "--self-check" => a.cfg.self_check = true,
            "--apply" => a.apply = true,
            "--character" => a.character = true,
            "--save-config" => {
                a.cfg.save()?;
                eprintln!("saved {}", oneiro_ai::config::config_path().display());
            }
            x if x.starts_with("--") => return Err(format!("unknown option {}", x)),
            _ => positional.push(s.clone()),
        }
        i += 1;
    }
    if let Some(p) = positional.first() {
        a.project = p.clone();
    }
    if need_request {
        a.request = positional.get(1).cloned().ok_or("missing the request text, e.g. oneiro ai examples/first_short \"add a funny ending\"")?;
    }
    if a.project.is_empty() {
        a.project = ".".into();
    }
    Ok(a)
}

pub fn cmd_ai_test(args: &[String]) -> Result<bool, String> {
    let a = parse_ai_args(args, false)?;
    let provider = a.cfg.build_provider(None)?;
    eprintln!("testing {} ...", provider.name());
    let t = Instant::now();
    let r = provider.complete(&oneiro_ai::ChatRequest {
        system: "You are a connection test. Follow the instruction exactly.".into(),
        messages: vec![oneiro_ai::Message::user("Reply with the single word OK and nothing else.")],
        max_tokens: 20,
    })?;
    println!("reply: {}", r.text.trim());
    eprintln!("ok in {:.1}s{}", t.elapsed().as_secs_f64(), r.usage.map(|(i, o)| format!(" ({} in / {} out tokens)", i, o)).unwrap_or_default());
    Ok(true)
}

pub fn cmd_ai(args: &[String]) -> Result<bool, String> {
    let a = parse_ai_args(args, true)?;
    let (root, script) = locate(Path::new(&a.project))?;
    let mut diags = Vec::new();
    let project = Project::load(&root, &mut diags);
    let lock = Lock::load(&script.with_extension("onlock"));
    let provider = a.cfg.build_provider(None)?;
    let opts = oneiro_ai::DirectorOpts { max_rounds: a.cfg.max_rounds, self_check: a.cfg.self_check, ..Default::default() };
    let mut progress = |m: &str| eprintln!("  {}", m);
    eprintln!("provider: {}", provider.name());
    if a.character {
        let r = oneiro_ai::direct_character(provider.as_ref(), &project, &a.request, &[], &opts, &mut progress)?;
        println!("{}", r.reply);
        match (&r.def, &r.source) {
            (Some(def), Some(src)) => {
                println!("\n{}", src);
                if a.apply {
                    let path = root.join("library").join("characters").join(format!("{}.onchar", def.name));
                    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
                    std::fs::write(&path, src).map_err(|e| e.to_string())?;
                    eprintln!("saved {}", path.display());
                } else {
                    eprintln!("(not saved: add --apply to write it to the library)");
                }
                Ok(true)
            }
            _ => {
                for d in &r.diagnostics {
                    eprintln!("{}", d.render());
                }
                eprintln!("no valid character after {} round(s)", r.rounds);
                Ok(false)
            }
        }
    } else {
        let current = std::fs::read_to_string(&script).map_err(|e| e.to_string())?;
        let label = script.strip_prefix(&root).unwrap_or(&script).to_string_lossy().replace('\\', "/");
        let ctx = oneiro_ai::ProjectCtx { project: &project, lock: &lock, script_label: label };
        let r = oneiro_ai::direct_scene(provider.as_ref(), &ctx, &current, &[], &a.request, &opts, &mut progress)?;
        println!("{}", r.reply);
        for n in &r.notes {
            eprintln!("note: {}", n);
        }
        eprintln!(
            "{} model call(s){}",
            r.rounds,
            if r.usage.0 + r.usage.1 > 0 { format!(", {} in / {} out tokens", r.usage.0, r.usage.1) } else { String::new() }
        );
        match &r.script {
            None => Ok(true),
            Some(s) => {
                for d in &r.diagnostics {
                    eprintln!("{}", d.render());
                }
                if !r.valid {
                    eprintln!("the proposed script still has errors; not applying");
                    return Ok(false);
                }
                if a.apply {
                    let _ = std::fs::write(script.with_extension("onr.bak"), &current);
                    std::fs::write(&script, s).map_err(|e| e.to_string())?;
                    eprintln!("script updated ({}); previous version saved as .onr.bak", script.display());
                } else {
                    println!("\n```oneiro\n{}\n```", s);
                    eprintln!("(not applied: add --apply to write it to {})", script.display());
                }
                Ok(true)
            }
        }
    }
}

pub fn cmd_guide(args: &[String]) -> Result<bool, String> {
    let path = args.first().cloned().unwrap_or_else(|| ".".into());
    let (root, _) = locate(Path::new(&path))?;
    let mut diags = Vec::new();
    let project = Project::load(&root, &mut diags);
    let dir = root.join("docs").join("ai");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let out = dir.join("guide.md");
    std::fs::write(&out, oneiro_ai::guide::guide_document(&project)).map_err(|e| e.to_string())?;
    eprintln!("wrote {}", out.display());
    Ok(true)
}
