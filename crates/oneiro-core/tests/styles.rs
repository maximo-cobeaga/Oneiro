//! Phase 5: per-character styles in the language and vector (SVG) characters in the library.

use oneiro_core::lock::Lock;
use oneiro_core::{compile_source, CharacterDef, Episode, Project};
use oneiro_lang::Diagnostic;
use std::path::{Path, PathBuf};

fn gallery() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/style_gallery").canonicalize().unwrap()
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let (p, q) = (e.path(), to.join(e.file_name()));
        if p.is_dir() {
            copy_dir(&p, &q);
        } else {
            std::fs::copy(&p, &q).unwrap();
        }
    }
}

fn scratch(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("oneiro_styles_{}_{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&root);
    copy_dir(&gallery(), &root);
    root
}

fn load(root: &Path) -> (Project, Vec<Diagnostic>) {
    let mut d = vec![];
    let p = Project::load(root, &mut d);
    (p, d)
}

fn compile(p: &Project, script: &str) -> (Episode, Vec<Diagnostic>) {
    compile_source(p, "script.onr", script, &Lock::default())
}

fn styles_of(ep: &Episode) -> Vec<(String, String)> {
    ep.scenes[0].actors.iter().map(|a| (a.name.clone(), a.def.style.clone())).collect()
}

fn errors(d: &[Diagnostic]) -> Vec<String> {
    d.iter().filter(|x| x.is_error()).map(|x| x.render()).collect()
}

#[test]
fn the_gallery_loads_and_each_character_gets_its_own_style() {
    let root = gallery();
    let (p, d) = load(&root);
    assert!(d.is_empty(), "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    let script = std::fs::read_to_string(root.join("script.onr")).unwrap();
    let (ep, d) = compile(&p, &script);
    assert!(errors(&d).is_empty(), "{:?}", errors(&d));
    let mut s = styles_of(&ep);
    s.sort();
    assert_eq!(s, vec![("Blob".into(), "rubber_hose".into()), ("Marta".into(), "cutout_2d".into()), ("Pip".into(), "cutout_2d".into())]);
    assert!(ep.scenes[0].actors.iter().find(|a| a.name == "Pip").unwrap().def.has_svg(), "Pip is a vector character");
    assert!(!ep.scenes[0].actors.iter().find(|a| a.name == "Marta").unwrap().def.has_svg());

    let clay = std::fs::read_to_string(root.join("clay.onr")).unwrap();
    let (ep, d) = compile(&p, &clay);
    assert!(errors(&d).is_empty(), "{:?}", errors(&d));
    assert!(styles_of(&ep).iter().all(|(_, s)| s == "clay"), "default style applies to everyone: {:?}", styles_of(&ep));
}

#[test]
fn use_overrides_the_character_style_which_overrides_the_default() {
    let root = scratch("resolve");
    // Marta declares her own style in the library; the script default is clay.
    let marta = std::fs::read_to_string(root.join("library/characters/Marta.onchar")).unwrap().replacen("  rig:", "  style: rubber_hose\n  rig:", 1);
    std::fs::write(root.join("library/characters/Marta.onchar"), marta).unwrap();
    let (p, d) = load(&root);
    assert!(d.is_empty(), "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    let base = std::fs::read_to_string(root.join("clay.onr")).unwrap();

    let (ep, _) = compile(&p, &base);
    let get = |ep: &Episode, n: &str| ep.scenes[0].actors.iter().find(|a| a.name == n).unwrap().def.style.clone();
    assert_eq!(get(&ep, "Marta"), "rubber_hose", "the character's own style beats the default");
    assert_eq!(get(&ep, "Blob"), "clay");

    let over = base.replacen("use character Marta", "use character Marta style cutout_2d", 1);
    let (ep, d) = compile(&p, &over);
    assert!(errors(&d).is_empty());
    assert_eq!(get(&ep, "Marta"), "cutout_2d", "`use ... style` beats everything");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn unknown_and_planned_styles_are_reported_with_help() {
    let root = scratch("errors");
    let (p, _) = load(&root);
    let base = std::fs::read_to_string(root.join("script.onr")).unwrap();

    let (_, d) = compile(&p, &base.replacen("style rubber_hose", "style rubberhose", 1));
    let e = d.iter().find(|x| x.code == "E2301").expect("E2301");
    assert!(e.message.contains("unknown style 'rubberhose'") && e.suggestion.as_deref().unwrap_or("").contains("rubber_hose"), "{:?}", e);

    let (_, d) = compile(&p, &base.replacen("style default: cutout_2d", "style default: toon_3d", 1));
    let e = d.iter().find(|x| x.code == "E2301").expect("E2301");
    assert!(e.message.contains("planned") && e.suggestion.as_deref().unwrap_or("").contains("clay"), "{:?}", e);

    // The same check applies to a style written in a .onchar
    let bad = "character Zed {\n  rig: library.rigs.biped_cutout\n  style: clayy\n}\n";
    let (file, mut d) = oneiro_lang::parse(bad, "Zed.onchar");
    let _ = CharacterDef::from_block(&file.blocks[0], "Zed.onchar", &mut d);
    assert!(d.iter().any(|x| x.code == "E2301" && x.suggestion.as_deref().unwrap_or("").contains("clay")), "{:?}", d);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn character_source_round_trips_style_and_parts() {
    let root = gallery();
    let (p, _) = load(&root);
    let pip = &p.characters["Pip"];
    let src = pip.to_source();
    assert!(src.contains("parts {") && src.contains("library/parts/Pip/head.svg") && src.contains("face { mode: auto"), "{}", src);
    let (file, mut d) = oneiro_lang::parse(&src, "Pip.onchar");
    let mut again = CharacterDef::from_block(&file.blocks[0], "Pip.onchar", &mut d);
    assert!(d.is_empty(), "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    again.load_svg_parts(&root, "Pip.onchar", &mut d);
    assert!(d.is_empty() && again.has_svg());
    assert_eq!(again.to_source(), src, "stable round trip");

    let mut c = p.characters["Marta"].clone();
    c.style = "clay".into();
    assert!(c.to_source().contains("  style: clay\n"));
}

#[test]
fn bad_vector_parts_are_rejected_with_actionable_messages() {
    let root = scratch("badparts");
    let write = |name: &str, text: &str| std::fs::write(root.join("library/parts/Pip").join(name), text).unwrap();
    let problems = |root: &Path| -> String {
        let (_, d) = load(root);
        d.iter().map(|x| format!("{} {}", x.code, x.message)).collect::<Vec<_>>().join("\n")
    };

    // 1. A part that spills out of its viewBox.
    write("arm.svg", r##"<svg viewBox="0 0 24 100"><rect x="-40" y="2" width="90" height="90" fill="#fff"/></svg>"##);
    let t = problems(&root);
    assert!(t.contains("E1117") && t.contains("spills outside its viewBox"), "{}", t);

    // 2. A limb with a wide viewBox and a drawing that is too small.
    write("arm.svg", r##"<svg viewBox="0 0 100 24"><circle cx="50" cy="12" r="3" fill="#fff"/></svg>"##);
    let t = problems(&root);
    assert!(t.contains("limbs need a tall viewBox") && t.contains("too small"), "{}", t);

    // 3. Unsafe or unsupported content.
    write("arm.svg", r##"<svg viewBox="0 0 24 100"><script>alert(1)</script><circle cx="12" cy="50" r="10" fill="url(#g)"/></svg>"##);
    let t = problems(&root);
    assert!(t.contains("E1116") && t.contains("<script> is not supported") && t.contains("gradients"), "{}", t);

    // 4. Missing file and a path that tries to leave the project.
    std::fs::remove_file(root.join("library/parts/Pip/leg.svg")).unwrap();
    let pip = std::fs::read_to_string(root.join("library/characters/Pip.onchar")).unwrap().replacen("library/parts/Pip/head.svg", "../outside.svg", 1);
    std::fs::write(root.join("library/characters/Pip.onchar"), pip).unwrap();
    let t = problems(&root);
    assert!(t.contains("E1115") && t.contains("cannot read") && t.contains("inside the project"), "{}", t);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_parts_block_must_be_complete_and_well_formed() {
    let src = "character Zed {\n  rig: library.rigs.biped_cutout\n  parts {\n    head { file: \"h.svg\", height: 3 }\n    tail { file: \"t.svg\" }\n    arm { height: 0.3 }\n    leg: 5\n    face { mode: sideways }\n  }\n}\n";
    let (file, mut d) = oneiro_lang::parse(src, "Zed.onchar");
    let _ = CharacterDef::from_block(&file.blocks[0], "Zed.onchar", &mut d);
    let codes: Vec<&str> = d.iter().map(|x| x.code.as_str()).collect();
    for want in ["E1110", "E1112", "E1113"] {
        assert!(codes.contains(&want), "missing {} in {:?}", want, d.iter().map(|x| x.render()).collect::<Vec<_>>());
    }
    assert!(d.iter().any(|x| x.message.contains("unknown part 'tail'") && x.suggestion.as_deref().unwrap_or("").contains("head, body, arm, leg")));
}
