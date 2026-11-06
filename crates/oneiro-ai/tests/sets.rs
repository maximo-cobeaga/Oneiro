//! The AI paints an open set (backdrops and sprites) and the engine validates, saves and loads it.

use oneiro_ai::providers::mock::MockProvider;
use oneiro_ai::{direct_set, validate_set, DirectorOpts};
use oneiro_core::Project;
use std::collections::BTreeMap;

const WIDE: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1600 600\"><rect x=\"0\" y=\"300\" width=\"1600\" height=\"300\" fill=\"#2BA4D9\"/><circle cx=\"800\" cy=\"300\" r=\"90\" fill=\"#FFD66B\"/><rect x=\"0\" y=\"500\" width=\"1600\" height=\"100\" fill=\"#1E8FC4\"/></svg>";
const TALL: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 200\"><rect x=\"44\" y=\"80\" width=\"12\" height=\"120\" fill=\"#8A5A34\"/><circle cx=\"50\" cy=\"60\" r=\"45\" fill=\"#2FA85A\"/><circle cx=\"30\" cy=\"90\" r=\"25\" fill=\"#38B764\"/></svg>";

fn source() -> String {
    "set lagoon {\n  kind: open\n  size: 14m x 10m\n  sky: [#4CC3FF, #FFF3C9]\n  ground: #F0D58C\n  backdrops {\n    north: \"backdrop_north\"\n  }\n  anchors {\n    start: (-4.0, -2.0)\n    center: (0.0, 0.0)\n  }\n  sprites {\n    tree_1: { image: \"sprite_tree\", at: (-5.0, 3.0), height: 4.0, anim: sway }\n    tree_2: { image: \"sprite_tree\", at: (5.0, 3.5), height: 3.5, anim: sway, flip: true }\n  }\n}".to_string()
}

fn svgs() -> BTreeMap<String, String> {
    [("backdrop_north".to_string(), WIDE.to_string()), ("sprite_tree".to_string(), TALL.to_string())].into_iter().collect()
}

#[test]
fn a_good_open_set_validates_and_names_become_project_paths() {
    let (set, src, files) = validate_set(&source(), &svgs()).unwrap_or_else(|d| panic!("{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>()));
    assert!(set.open && set.name == "lagoon" && set.sprites.len() == 2);
    assert_eq!(files.len(), 2, "a picture shared by two sprites is saved once");
    assert!(files.iter().any(|(p, _)| p == "backgrounds/lagoon_backdrop_north.svg") && files.iter().any(|(p, _)| p == "scenery/lagoon_sprite_tree.svg"));
    assert!(src.contains("\"backgrounds/lagoon_backdrop_north.svg\"") && src.contains("\"scenery/lagoon_sprite_tree.svg\""));
    assert!(!src.contains("\"sprite_tree\""));
}

#[test]
fn bad_sets_are_rejected_with_reasons() {
    let why = |src: String, svgs: BTreeMap<String, String>| validate_set(&src, &svgs).unwrap_err().iter().map(|d| d.message.clone()).collect::<Vec<_>>().join(" | ");
    assert!(why(source().replace("kind: open", "kind: room"), svgs()).contains("kind: open"));
    let mut missing = svgs();
    missing.remove("sprite_tree");
    assert!(why(source(), missing).contains("missing the ```svg sprite_tree"));
    let mut square = svgs();
    square.insert("backdrop_north".into(), WIDE.replace("0 0 1600 600", "0 0 600 600"));
    assert!(why(source(), square).contains("must be wide"));
    let mut text = svgs();
    text.insert("sprite_tree".into(), "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\"><text>hi</text></svg>".into());
    assert!(!why(source(), text).is_empty());
    assert!(why(source().replace("backdrops {\n    north: \"backdrop_north\"\n  }", ""), svgs()).contains("backdrops"));
}

#[test]
fn the_director_retries_until_the_scenery_is_valid_and_it_loads_in_a_project() {
    let reply = |src: &str, tree: &str| format!("A tiny lagoon.\n```onset\n{}\n```\n```svg backdrop_north\n{}\n```\n```svg sprite_tree\n{}\n```", src, WIDE, tree);
    // First attempt forgets to draw anything in the tree; the second is fine.
    let bad = reply(&source(), "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\"><rect x=\"1\" y=\"1\" width=\"5\" height=\"5\" fill=\"#fff\"/></svg>");
    let good = reply(&source(), TALL);
    let mock = MockProvider::new(vec![bad, good]);
    let dir = std::env::temp_dir().join(format!("oneiro_setai_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("oneiro.toml"), "name = \"t\"\n").unwrap();
    let mut d = vec![];
    let project = Project::load(&dir, &mut d);
    let r = direct_set(&mock, &project, "a small tropical lagoon", &[], &DirectorOpts::default(), &mut |_| {}).unwrap();
    assert!(r.valid && r.rounds == 2, "valid {} rounds {} {:?}", r.valid, r.rounds, r.diagnostics.iter().map(|x| x.render()).collect::<Vec<_>>());
    let req = mock.requests.lock().unwrap();
    assert!(req[0].system.contains("scenery painter") && req[1].messages.last().unwrap().text.contains("shape"), "the problem is fed back");
    drop(req);
    r.save(&dir).unwrap();
    let mut d = vec![];
    let p = Project::load(&dir, &mut d);
    assert!(d.is_empty(), "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    let s = &p.sets["lagoon"];
    assert!(s.backdrops["north"].raster.is_some() && s.sprites.iter().all(|x| x.art.raster.is_some()));
    let _ = std::fs::remove_dir_all(&dir);
}
