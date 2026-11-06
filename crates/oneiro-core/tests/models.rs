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
