//! Phase 17: open 3D sets built from stock props and `solids`.

use oneiro_core::lock::Lock;
use oneiro_core::{compile, nav, scaffold, Project};
use std::path::{Path, PathBuf};

fn village() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/village").canonicalize().unwrap()
}

#[test]
fn the_village_loads_compiles_and_its_pieces_are_obstacles() {
    let mut d = vec![];
    let p = Project::load(&village(), &mut d);
    assert!(d.is_empty(), "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    let s = &p.sets["village"];
    assert!(s.open && s.backdrops.is_empty() && s.props.len() == 14 && s.solids.len() == 8);
    let fp = nav::footprints(s);
    assert!(fp.iter().any(|(n, _)| n == "house_1") && fp.iter().any(|(n, _)| n == "car_1"), "houses and cars block people");
    assert!(fp.iter().any(|(n, _)| n == "well_base"), "a solid box/cylinder blocks people");
    assert!(!fp.iter().any(|(n, _)| n == "bush_1" || n == "post_a" || n == "well_roof"), "bushes, thin posts and roofs do not");
    let (_, ds) = compile(&p, &village().join("script.onr"), &Lock::default());
    assert!(ds.iter().all(|x| !x.is_error()), "{:?}", ds.iter().map(|x| x.render()).collect::<Vec<_>>());
}

#[test]
fn bad_solids_are_explained() {
    let dir = std::env::temp_dir().join(format!("oneiro_solids_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("library/sets")).unwrap();
    std::fs::write(dir.join("oneiro.toml"), "name = \"t\"\n").unwrap();
    let load = |body: &str| {
        std::fs::write(dir.join("library/sets/s.onset"), format!("set s {{\n  kind: open\n  solids {{\n{}\n  }}\n}}\n", body)).unwrap();
        let mut d = vec![];
        Project::load(&dir, &mut d);
        d.iter().map(|x| format!("{} {}", x.code, x.message)).collect::<Vec<_>>()
    };
    assert!(load("    a: { shape: donut }").iter().any(|m| m.starts_with("E1151") && m.contains("donut")));
    assert!(load("    a: { shape: box, size: (1, 2) }").iter().any(|m| m.starts_with("E1152")));
    assert!(load("    a: { shape: box, size: (1, 900, 1) }").iter().any(|m| m.starts_with("E1152")));
    assert!(load("    a: 5").iter().any(|m| m.starts_with("E1150")));
    assert!(load("    a: { shape: cone, at: (1, 2), size: (2, 3, 2), color: #112233 }").is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn set_templates_for_the_ide_are_valid_sets() {
    let dir = std::env::temp_dir().join(format!("oneiro_tpl_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("library/sets")).unwrap();
    std::fs::write(dir.join("oneiro.toml"), "name = \"t\"\n").unwrap();
    for (name, text) in [("a_room", scaffold::room_source("a_room")), ("a_diner", scaffold::diner_source("a_diner")), ("a_field", scaffold::field_source("a_field")), ("a_village", scaffold::village_source("a_village"))] {
        std::fs::write(dir.join(format!("library/sets/{}.onset", name)), text).unwrap();
    }
    let mut d = vec![];
    let p = Project::load(&dir, &mut d);
    assert!(d.is_empty(), "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    assert!(p.sets["a_field"].open && p.sets["a_village"].open && !p.sets["a_room"].open);
    let _ = std::fs::remove_dir_all(&dir);
}
