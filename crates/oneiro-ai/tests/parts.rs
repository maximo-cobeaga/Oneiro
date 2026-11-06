//! Tier B: the AI draws a character as SVG parts and the engine validates, saves and renders it.

use oneiro_ai::providers::mock::MockProvider;
use oneiro_ai::tools::Toolbox;
use oneiro_ai::{direct_character_parts, validate_parts, DirectorOpts};
use oneiro_core::lock::Lock;
use oneiro_core::{compile_source, Project};
use serde_json::{json, Value};
use std::collections::BTreeMap;
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
    let root = std::env::temp_dir().join(format!("oneiro_parts_{}_{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&root);
    copy_dir(&gallery(), &root);
    root
}

fn svg(part: &str) -> String {
    std::fs::read_to_string(gallery().join("library/parts/Pip").join(format!("{}.svg", part))).unwrap()
}

const ONCHAR: &str = "character Robo {\n  rig: library.rigs.biped_cutout\n  build {\n    body: library.bodies.kid { height: 1.0 }\n  }\n  parts {\n    head { height: 0.30, pivot: (0.5, 0.97) }\n    body { height: 0.33, pivot: (0.5, 0.97) }\n    arm  { height: 0.30, pivot: (0.5, 0.08) }\n    leg  { height: 0.36, pivot: (0.5, 0.04) }\n    face { mode: auto, x: 0.5, y: 0.56, scale: 1 }\n  }\n  voice { provider: file, pitch: 280 }\n  sheet { age: 5, traits: [shy], notes: \"A tiny robot.\" }\n}";

fn reply(onchar: &str, parts: &[(&str, String)]) -> String {
    let mut s = String::from("A small blue robot.\n```onchar\n");
    s.push_str(onchar);
    s.push_str("\n```\n");
    for (name, text) in parts {
        s.push_str(&format!("```svg {}\n{}\n```\n", name, text));
    }
    s
}

fn good_parts() -> Vec<(&'static str, String)> {
    ["head", "body", "arm", "leg"].iter().map(|p| (*p, svg(p))).collect()
}

fn opts() -> DirectorOpts {
    DirectorOpts { max_rounds: 3, ..Default::default() }
}

#[test]
fn a_valid_reply_is_accepted_saved_and_usable_in_a_script() {
    let root = scratch("ok");
    let mut d = vec![];
    let project = Project::load(&root, &mut d);
    let mock = MockProvider::new(vec![reply(ONCHAR, &good_parts())]);
    let r = direct_character_parts(&mock, &project, "a small blue robot", &[], &opts(), &mut |_| {}).unwrap();
    assert!(r.valid && r.rounds == 1, "{:?}", r.diagnostics.iter().map(|x| x.render()).collect::<Vec<_>>());
    assert_eq!(r.reply, "A small blue robot.");
    assert_eq!(r.svgs.len(), 4);
    let src = r.source.clone().unwrap();
    assert!(src.contains("library/parts/Robo/head.svg") && src.contains("face { mode: auto"), "{}", src);

    let path = r.save(&root).unwrap();
    assert!(path.ends_with("Robo.onchar") && root.join("library/parts/Robo/leg.svg").exists());

    // The saved character loads cleanly and works in a script.
    let mut d = vec![];
    let project = Project::load(&root, &mut d);
    assert!(d.is_empty(), "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    assert!(project.characters["Robo"].has_svg());
    let script = "oneiro 0.1\nproject \"t\"\nuse character Robo style clay\nuse set diner\nscene 1 \"s\" in diner {\n  place Robo at diner.floor\n  camera full on Robo\n  Robo (happy): \"Beep.\"\n}\n";
    let (ep, diags) = compile_source(&project, "script.onr", script, &Lock::default());
    assert!(diags.iter().all(|x| !x.is_error()), "{:?}", diags);
    assert_eq!(ep.scenes[0].actors[0].def.style, "clay");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_engine_rejects_bad_drawings_and_the_model_fixes_them() {
    let root = scratch("fix");
    let mut d = vec![];
    let project = Project::load(&root, &mut d);
    // Round 1: a gradient in the head, an arm that spills outside its viewBox, no leg, a script tag.
    let mut bad = good_parts();
    bad[0].1 = r##"<svg viewBox="0 0 100 90"><rect x="9" y="14" width="82" height="74" fill="url(#sky)"/></svg>"##.into();
    bad[2].1 = r##"<svg viewBox="0 0 24 100"><rect x="-30" y="0" width="90" height="100" fill="#fff"/></svg>"##.into();
    bad.truncate(3);
    bad.push(("body", r##"<svg viewBox="0 0 80 100"><script>x()</script><rect x="6" y="4" width="68" height="92" fill="#f2aa3c"/></svg>"##.into()));
    let mock = MockProvider::new(vec![reply(ONCHAR, &bad), reply(ONCHAR, &good_parts())]);
    let r = direct_character_parts(&mock, &project, "a robot", &[], &opts(), &mut |_| {}).unwrap();
    assert!(r.valid && r.rounds == 2);
    // What the model was told in the second request: every problem, specific enough to act on.
    let reqs = mock.requests.lock().unwrap();
    let feedback = &reqs[1].messages.last().unwrap().text;
    for needle in ["gradients", "spills outside its viewBox", "missing the ```svg leg block", "<script> is not supported"] {
        assert!(feedback.contains(needle), "feedback should mention {:?}:\n{}", needle, feedback);
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn it_gives_up_with_diagnostics_when_the_model_keeps_failing_or_forgets_the_blocks() {
    let root = scratch("fail");
    let mut d = vec![];
    let project = Project::load(&root, &mut d);
    let mock = MockProvider::new(vec!["I cannot draw.".into()]);
    let r = direct_character_parts(&mock, &project, "a robot", &[], &DirectorOpts { max_rounds: 2, ..Default::default() }, &mut |_| {}).unwrap();
    assert!(!r.valid && r.rounds == 2 && r.source.is_none());
    assert!(r.diagnostics.iter().any(|x| x.message.contains("no ```onchar block")));
    assert!(r.save(&root).is_err(), "nothing valid to save");

    let mut one_bad = good_parts();
    one_bad[1].1 = "<svg viewBox=\"0 0 80 100\"></svg>".into();
    let mock = MockProvider::new(vec![reply(ONCHAR, &one_bad)]);
    let r = direct_character_parts(&mock, &project, "a robot", &[], &DirectorOpts { max_rounds: 2, ..Default::default() }, &mut |_| {}).unwrap();
    assert!(!r.valid && r.diagnostics.iter().any(|x| x.message.contains("draws nothing")), "{:?}", r.diagnostics.iter().map(|x| x.render()).collect::<Vec<_>>());
    assert!(!root.join("library/characters/Robo.onchar").exists(), "nothing is saved for an invalid character");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn files_are_always_saved_where_oneiro_expects_them() {
    // A model that invents paths (including a path that tries to escape the project) is overruled.
    let sneaky = ONCHAR.replace("head { height: 0.30,", "head { file: \"../../evil.svg\", height: 0.30,");
    let svgs: BTreeMap<String, String> = good_parts().into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    let (def, src, files) = validate_parts(&sneaky, &svgs).unwrap();
    assert!(files.iter().all(|(p, _)| p.starts_with("library/parts/Robo/")), "{:?}", files.iter().map(|f| &f.0).collect::<Vec<_>>());
    assert!(!src.contains("evil") && def.has_svg());

    // Default part values apply when the model omits the parts block.
    let plain = "character Robo2 {\n  rig: library.rigs.biped_cutout\n}";
    let (def, src, _) = validate_parts(plain, &svgs).unwrap();
    assert!(def.has_svg() && src.contains("library/parts/Robo2/arm.svg"));
    // A bad name is reported.
    let e = validate_parts("character robo {\n  rig: library.rigs.biped_cutout\n}", &svgs).unwrap_err();
    assert!(e.iter().any(|d| d.message.contains("invalid character name")));
}

#[test]
fn the_mcp_tool_validates_and_saves_vector_characters() {
    let root = scratch("tool");
    let tb = Toolbox::open(&root).unwrap();
    assert!(Toolbox::definitions().iter().any(|t| t.name == "write_character_parts"));
    let svgs = json!({"head": svg("head"), "body": svg("body"), "arm": svg("arm"), "leg": svg("leg")});

    let ok = tb.call("write_character_parts", &json!({"source": ONCHAR, "svgs": svgs}));
    assert!(!ok.is_error, "{}", ok.text);
    assert!(root.join("library/characters/Robo.onchar").exists() && root.join("library/parts/Robo/head.svg").exists());

    let mut bad = svgs.clone();
    bad["leg"] = Value::String("<svg viewBox=\"0 0 30 100\"><circle cx=\"15\" cy=\"50\" r=\"4\" fill=\"#000\"/></svg>".into());
    let e = tb.call("write_character_parts", &json!({"source": ONCHAR.replace("Robo", "Robo3"), "svgs": bad}));
    assert!(e.is_error && e.text.contains("too small") && !root.join("library/characters/Robo3.onchar").exists(), "{}", e.text);

    // The guide an agent reads teaches both the format and the styles.
    let g = tb.call("get_guide", &json!({}));
    for needle in ["vector characters", "svg head", "pivot", "rubber_hose", "clay", "drawn from vector parts"] {
        assert!(g.text.contains(needle), "guide lacks {:?}", needle);
    }
    // ...and the library summary tells which characters are drawn.
    let lib = tb.call("list_library", &json!({}));
    assert!(lib.text.contains("Pip") && lib.text.contains("drawn from vector parts"), "{}", lib.text);
    let _ = std::fs::remove_dir_all(&root);
}
