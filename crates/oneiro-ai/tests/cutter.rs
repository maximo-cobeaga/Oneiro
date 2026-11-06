//! The AI half of the hybrid cutter and its MCP tools.

use oneiro_ai::cutter_ai::propose_with_ai;
use oneiro_ai::providers::mock::MockProvider;
use oneiro_ai::tools::Toolbox;
use oneiro_ai::DirectorOpts;
use oneiro_core::Project;
use oneiro_cutter::{extract_mask, propose, Image, Layout};
use serde_json::{json, Value};
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
    let root = std::env::temp_dir().join(format!("oneiro_cutai_{}_{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&root);
    copy_dir(&gallery(), &root);
    root
}

fn art() -> PathBuf {
    gallery().join("art/marta_illustration.png")
}

fn opts() -> DirectorOpts {
    DirectorOpts { max_rounds: 3, ..Default::default() }
}

#[test]
fn the_model_proposes_boxes_the_engine_rejects_them_and_the_model_fixes_them() {
    let img = Image::load(&art()).unwrap();
    let mask = extract_mask(&img, None).unwrap();
    let good = propose(&mask).unwrap();
    // Round 1: head and body swapped, the arm on the right side. Round 2: the right answer, wrapped in prose.
    let mut bad = good.clone();
    std::mem::swap(&mut bad.head, &mut bad.body);
    bad.arm = good.arm.mirrored(0.5);
    let mock = MockProvider::new(vec![bad.to_json(), format!("Sorry, corrected:\n```json\n{}\n```", good.to_json())]).with_images();
    let r = propose_with_ai(&mock, &img, &mask, Some(&good), &opts(), &mut |_| {}).unwrap();
    assert!(r.valid && r.rounds == 2, "{:?}", r.issues.iter().map(|i| i.render()).collect::<Vec<_>>());

    let reqs = mock.requests.lock().unwrap();
    // The first request carries the picture and the heuristic guess...
    assert_eq!(reqs[0].messages[0].images.len(), 1);
    assert!(reqs[0].messages[0].text.contains("rough automatic guess"));
    // ...the second one tells the model what is wrong and shows it its own boxes.
    let fb = reqs[1].messages.last().unwrap();
    assert!(fb.text.contains("head box must be above the body box") && fb.text.contains("LEFT side"), "{}", fb.text);
    assert_eq!(fb.images.len(), 1, "the overlay with the numbered boxes is attached");
    assert!(reqs[0].system.contains("ONLY a JSON object") && reqs[0].system.contains("NORMALIZED"));
}

#[test]
fn unreadable_answers_are_retried_and_a_blind_provider_is_refused() {
    let img = Image::load(&art()).unwrap();
    let mask = extract_mask(&img, None).unwrap();
    let good = propose(&mask).unwrap();
    let mock = MockProvider::new(vec!["I think the head is at the top.".into(), good.to_json()]).with_images();
    let r = propose_with_ai(&mock, &img, &mask, Some(&good), &opts(), &mut |_| {}).unwrap();
    assert!(r.valid && r.rounds == 2);
    assert!(mock.requests.lock().unwrap()[1].messages.last().unwrap().text.contains("could not read your answer"));

    let blind = MockProvider::new(vec![good.to_json()]);
    let e = propose_with_ai(&blind, &img, &mask, Some(&good), &opts(), &mut |_| {}).err().unwrap();
    assert!(e.contains("cannot look at images"), "{}", e);

    // A model that never gets it right: the last proposal and its problems are returned (the person fixes it by dragging).
    let mut hopeless = good.clone();
    hopeless.leg = oneiro_cutter::RectN::new(0.0, 0.0, 0.05, 0.05);
    let mock = MockProvider::new(vec![hopeless.to_json()]).with_images();
    let r = propose_with_ai(&mock, &img, &mask, Some(&good), &DirectorOpts { max_rounds: 2, ..Default::default() }, &mut |_| {}).unwrap();
    assert!(!r.valid && r.rounds == 2 && r.issues.iter().any(|i| i.is_error()));
}

#[test]
fn the_mcp_tools_propose_validate_and_save_a_cut_character() {
    let root = scratch("tools");
    let tb = Toolbox::open(&root).unwrap();
    for t in ["propose_cut", "cut_character"] {
        assert!(Toolbox::definitions().iter().any(|d| d.name == t), "{} missing", t);
    }
    let image = art().to_string_lossy().to_string();

    // Step 1: layout JSON, problems and the picture with numbered boxes.
    let p = tb.call("propose_cut", &json!({"image": image}));
    assert!(!p.is_error, "{}", p.text);
    assert_eq!(&p.images[0][..4], &[0x89, b'P', b'N', b'G']);
    assert!(p.text.contains("\"head\"") && p.text.contains("Validation:"));
    let layout = Layout::from_json(&p.text).unwrap();

    // The agent edits the layout and gets the new verdict.
    let mut edited = layout.clone();
    edited.arm = edited.arm.mirrored(0.5);
    let p2 = tb.call("propose_cut", &json!({"image": image, "layout": edited.to_json()}));
    assert!(p2.text.contains("LEFT side") && p2.text.contains("Fix the errors"), "{}", p2.text);

    // Step 2: nothing is saved with errors or a bad name...
    let e = tb.call("cut_character", &json!({"image": image, "name": "Newbie", "layout": edited.to_json()}));
    assert!(e.is_error && !root.join("library/characters/Newbie.onchar").exists());
    let e = tb.call("cut_character", &json!({"image": image, "name": "newbie"}));
    assert!(e.is_error && e.text.contains("not a valid character name"), "{}", e.text);
    // ...and a valid cut is saved, then loads as a normal character.
    let ok = tb.call("cut_character", &json!({"image": image, "name": "Newbie", "height_m": 1.0, "style": "clay"}));
    assert!(!ok.is_error, "{}", ok.text);
    let mut d = vec![];
    let project = Project::load(&root, &mut d);
    assert!(d.is_empty(), "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    let c = &project.characters["Newbie"];
    assert!(c.has_svg() && c.style == "clay" && (c.height() - 1.0 * 1.0).abs() < 0.2);
    assert!(c.svg.as_ref().unwrap().parts.values().all(|p| p.file.ends_with(".png") && p.raster.is_some()));

    // Missing or broken images are explained.
    let m = tb.call("propose_cut", &json!({"image": "nope.png"}));
    assert!(m.is_error);
    let g: Value = json!(tb.call("get_guide", &json!({})).text.contains("propose_cut"));
    assert_eq!(g, json!(true), "the guide tells agents about the cutter");
    let _ = std::fs::remove_dir_all(&root);
}
