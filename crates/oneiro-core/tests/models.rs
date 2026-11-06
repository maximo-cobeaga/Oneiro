//! Phase 11: OBJ models as props.

use oneiro_core::nav;
use oneiro_core::obj::ObjModel;
use oneiro_core::Project;
use std::path::{Path, PathBuf};

fn demo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/models_demo").canonicalize().unwrap()
}

#[test]
fn obj_parses_polygons_negative_indices_and_slashes() {
    let m = ObjModel::parse("v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1/1/1 2/2/2 3/3/3 4/4/4\nf -4 -3 -2\n").unwrap();
    assert_eq!(m.verts.len(), 4);
    assert_eq!(m.tris.len(), 3, "a quad is two triangles, plus one more");
    assert_eq!(m.bounds(), ([0.0, 0.0, 0.0], [1.0, 1.0, 0.0]));
}

#[test]
fn bad_obj_files_say_what_is_wrong() {
    assert!(ObjModel::parse("v 0 0 0\nf 1 2 3\n").unwrap_err().contains("does not exist"));
    assert!(ObjModel::parse("v 0 0\n").unwrap_err().contains("three"));
    assert!(ObjModel::parse("# nothing\n").unwrap_err().contains("no faces"));
    assert!(ObjModel::parse("v 0 0 0\nv 1 0 0\nf 1 2\n").unwrap_err().contains("three"));
}

#[test]
fn the_demo_project_loads_its_models_and_the_crate_is_solid() {
    let mut d = vec![];
    let p = Project::load(&demo(), &mut d);
    assert!(d.is_empty(), "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    let room = &p.sets["room"];
    let crate_ = room.props.iter().find(|p| p.name == "crate").unwrap();
    assert_eq!(crate_.mesh.as_ref().unwrap().tris.len(), 12);
    let fp = nav::footprints(room);
    assert!(fp.iter().any(|(n, _)| n == "crate"), "solid: true makes it an obstacle");
    assert!(!fp.iter().any(|(n, _)| n == "ball"), "the ball is not solid");
}

#[test]
fn model_props_report_missing_files_and_paths_outside_the_project() {
    let dir = std::env::temp_dir().join(format!("oneiro_models_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("library/sets")).unwrap();
    std::fs::write(dir.join("oneiro.toml"), "name = \"t\"\n").unwrap();
    let set = |body: &str| format!("set s {{\n  size: 8m x 6m\n  props {{\n    {}\n  }}\n}}\n", body);
    let load = |body: &str| {
        std::fs::write(dir.join("library/sets/s.onset"), set(body)).unwrap();
        let mut d = vec![];
        Project::load(&dir, &mut d);
        d.iter().map(|x| format!("{} {}", x.code, x.message)).collect::<Vec<_>>()
    };
    assert!(load("a: library.props.model { model: \"nope.obj\" }").iter().any(|m| m.starts_with("E1122") && m.contains("cannot read")));
    assert!(load("a: library.props.model { model: \"../x.obj\" }").iter().any(|m| m.starts_with("E1122") && m.contains("inside the project")));
    assert!(load("a: library.props.model { at: (1, 1) }").iter().any(|m| m.starts_with("E1121")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_showcase_compiles_cleanly() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/showcase").canonicalize().unwrap();
    let mut d = vec![];
    let p = Project::load(&root, &mut d);
    let (ep, d2) = oneiro_core::compile(&p, &root.join("script.onr"), &oneiro_core::lock::Lock::default());
    d.extend(d2);
    assert!(d.is_empty(), "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    assert_eq!(ep.scenes[0].transitions.len(), 3);
}

#[test]
fn scaffolded_projects_compile_in_both_templates() {
    use oneiro_core::scaffold::{create_project, folder_name, valid_name, Template};
    for t in Template::ALL {
        let dir = std::env::temp_dir().join(format!("oneiro_scaffold_{}_{:?}", std::process::id(), t));
        let _ = std::fs::remove_dir_all(&dir);
        create_project(&dir, "My Show", t).unwrap();
        assert!(create_project(&dir, "again", t).is_err(), "never overwrites");
        let mut d = vec![];
        let p = Project::load(&dir, &mut d);
        let (ep, d2) = oneiro_core::compile(&p, &dir.join("script.onr"), &oneiro_core::lock::Lock::default());
        d.extend(d2);
        assert!(d.iter().all(|x| !x.is_error()), "{:?}: {:?}", t, d.iter().map(|x| x.render()).collect::<Vec<_>>());
        assert!(ep.duration > 0.5);
        let _ = std::fs::remove_dir_all(&dir);
    }
    assert!(valid_name("Marta_2") && !valid_name("2fast") && !valid_name("a b") && !valid_name(""));
    assert_eq!(folder_name("  My Great Show! "), "My-Great-Show");
}

fn beach() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/beach").canonicalize().unwrap()
}

#[test]
fn an_open_set_loads_its_backdrops_and_sprites() {
    let mut d = vec![];
    let p = Project::load(&beach(), &mut d);
    assert!(d.is_empty(), "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    let s = &p.sets["beach"];
    assert!(s.open && s.backdrops.len() == 4 && s.sprites.len() == 7);
    assert!(s.backdrops.values().all(|b| b.raster.is_some()) && s.sprites.iter().all(|x| x.art.raster.is_some()));
    assert_eq!((s.width, s.depth), (16.0, 12.0));
    assert_eq!(s.sprites.iter().find(|x| x.name == "palm_1").unwrap().anim, "sway");
}

#[test]
fn open_set_mistakes_are_explained() {
    let dir = std::env::temp_dir().join(format!("oneiro_open_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("library/sets")).unwrap();
    std::fs::write(dir.join("oneiro.toml"), "name = \"t\"\n").unwrap();
    let load = |body: &str| {
        std::fs::write(dir.join("library/sets/s.onset"), format!("set s {{\n  kind: open\n{}\n}}\n", body)).unwrap();
        let mut d = vec![];
        Project::load(&dir, &mut d);
        d.iter().map(|x| format!("{} {}", x.code, x.message)).collect::<Vec<_>>()
    };
    assert!(load("  backdrops {\n    up: \"a.svg\"\n  }").iter().any(|m| m.starts_with("E1125") && m.contains("backdrop side")));
    assert!(load("  backdrops {\n    north: \"missing.svg\"\n  }").iter().any(|m| m.starts_with("E1126") && m.contains("cannot read")));
    assert!(load("  backdrops {\n    north: \"../x.svg\"\n  }").iter().any(|m| m.starts_with("E1126") && m.contains("inside the project")));
    assert!(load("  sprites {\n    a: { height: 2 }\n  }").iter().any(|m| m.starts_with("E1123")));
    assert!(load("  sprites {\n    a: { image: \"x.svg\", anim: wobble }\n  }").iter().any(|m| m.starts_with("E1124") && m.contains("wobble")));
    assert!(load("  sky: 12").iter().any(|m| m.starts_with("E1125")));
    let _ = std::fs::remove_dir_all(&dir);
}
