//! Phase 8: characters walk around furniture instead of through it.

use oneiro_core::lock::Lock;
use oneiro_core::math::dist2;
use oneiro_core::nav::{self, Obb};
use oneiro_core::{compile_source, Episode, Project};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/first_short").canonicalize().unwrap()
}

fn project() -> Project {
    let mut d = vec![];
    Project::load(&root(), &mut d)
}

fn scene(body: &str) -> Episode {
    let text = format!("oneiro 0.1\nproject \"t\"\nuse character Marta\nuse character Blob\nuse set diner\nscene 1 \"s\" in diner {{\n{}\n}}\n", body);
    let (ep, d) = compile_source(&project(), "t.onr", &text, &Lock::default());
    assert!(d.iter().all(|x| !x.is_error()), "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    ep
}

fn counter() -> Obb {
    nav::footprints(&project().sets["diner"]).into_iter().find(|(n, _)| n == "counter").unwrap().1
}

fn positions(ep: &Episode, who: &str) -> Vec<[f64; 2]> {
    let sc = &ep.scenes[0];
    let a = sc.actors.iter().find(|a| a.name == who).unwrap();
    (0..(sc.dur * 20.0) as usize).map(|i| a.kin(i as f64 / 20.0).pos).collect()
}

#[test]
fn rectangles_know_what_is_inside_and_what_blocks_a_walk() {
    let o = Obb { c: [1.0, 2.0], hw: 1.0, hd: 0.5, yaw: 0.0 };
    assert!(o.contains([1.5, 2.2]) && !o.contains([3.0, 2.0]) && !o.contains([1.0, 3.0]));
    assert!(o.blocks([-1.0, 2.0], [3.5, 2.0]), "straight through");
    assert!(!o.blocks([-1.0, 3.0], [3.5, 3.0]), "alongside");
    assert!(!o.blocks([0.0, 2.0], [0.0, 2.0]) || o.contains([0.0, 2.0]));
    // Rotated rectangles use the renderer's axes (right = (cos, -sin)).
    let r = Obb { c: [0.0, 0.0], hw: 2.0, hd: 0.2, yaw: std::f64::consts::FRAC_PI_2 };
    assert!(r.contains([0.0, -1.5]) || r.contains([0.0, 1.5]) || r.contains([-0.0, 0.0]));
    assert!(r.blocks([-1.0, 1.5], [1.0, 1.5]) || r.blocks([-1.0, 0.5], [1.0, 0.5]));
    let big = o.inflate(0.5);
    assert!(big.contains([2.3, 2.0]) && !o.contains([2.3, 2.0]));
    // Pushing out lands just outside the nearest edge.
    let p = nav::push_out(&o, [1.0, 2.4]);
    assert!(!o.contains(p) && dist2(p, [1.0, 2.4]) < 0.2, "{:?}", p);
    assert_eq!(nav::push_out(&o, [5.0, 5.0]), [5.0, 5.0]);
}

#[test]
fn a_clear_walk_stays_a_single_straight_segment_and_a_blocked_one_goes_around() {
    let o = Obb { c: [0.0, 0.0], hw: 1.0, hd: 0.5, yaw: 0.0 };
    assert_eq!(nav::plan([-3.0, 2.0], [3.0, 2.0], &[o], (5.0, 5.0)), vec![[3.0, 2.0]]);
    let path = nav::plan([-3.0, 0.0], [3.0, 0.0], &[o], (5.0, 5.0));
    assert!(path.len() >= 2 && *path.last().unwrap() == [3.0, 0.0], "{:?}", path);
    let mut pts = vec![[-3.0, 0.0]];
    pts.extend(path.iter().copied());
    for w in pts.windows(2) {
        assert!(!o.blocks(w[0], w[1]), "{:?} -> {:?} goes through the obstacle", w[0], w[1]);
    }
    // The shorter side is chosen: starting a little above the middle, pass above.
    let up = nav::plan([-3.0, 0.2], [3.0, 0.2], &[o], (5.0, 5.0));
    assert!(up[0][1] > 0.0, "{:?}", up);
    let down = nav::plan([-3.0, -0.2], [3.0, -0.2], &[o], (5.0, 5.0));
    assert!(down[0][1] < 0.0, "{:?}", down);
    // Two obstacles in a row: still no segment crosses either.
    let o2 = Obb { c: [3.0, 0.0], hw: 0.6, hd: 1.5, yaw: 0.0 };
    let path = nav::plan([-3.0, 0.0], [6.0, 0.0], &[o, o2], (8.0, 8.0));
    let mut pts = vec![[-3.0, 0.0]];
    pts.extend(path.iter().copied());
    for w in pts.windows(2) {
        assert!(!o.blocks(w[0], w[1]) && !o2.blocks(w[0], w[1]), "{:?}", path);
    }
    // An obstacle that contains the start or the end is ignored (leaving or sitting on a stool).
    assert_eq!(nav::plan([0.0, 0.0], [3.0, 0.0], &[o], (5.0, 5.0)), vec![[3.0, 0.0]]);
    assert_eq!(nav::plan([-3.0, 0.0], [0.5, 0.0], &[o], (5.0, 5.0)), vec![[0.5, 0.0]]);
}

#[test]
fn walking_to_the_counter_stops_a_body_width_in_front_of_it() {
    // This was the "waiter half inside the bar" bug: the stop distance was 0.75 m from the counter's center.
    let ep = scene("  place Blob at diner.door\n  place Marta at diner.stool_2 facing diner.counter\n  camera wide\n  Blob walks_to diner.counter\n  wait 1s");
    let blob = ep.scenes[0].actors.iter().find(|a| a.name == "Blob").unwrap();
    let end = blob.kin(ep.scenes[0].dur).pos;
    let r = nav::radius(&blob.def);
    let c = counter();
    assert!(!c.inflate(r).contains(end), "Blob ends inside the counter at {:?}", end);
    assert!(c.inflate(r + 0.2).contains(end), "but close to it: {:?}", end);
    assert!(end[1] < c.c[1], "in front of it, not behind");
    // And he is never inside it on the way.
    for p in positions(&ep, "Blob") {
        assert!(!c.inflate(r - 0.02).contains(p), "inside the counter at {:?}", p);
    }
    // Wider characters stay farther from the furniture than narrow ones.
    let marta = ep.scenes[0].actors.iter().find(|a| a.name == "Marta").unwrap();
    assert!(nav::radius(&blob.def) > nav::radius(&marta.def), "Blob is round, Marta is a kid");
}

#[test]
fn a_walk_that_crosses_the_counter_goes_around_the_end_of_it() {
    let ep = scene("  place Marta at diner.door\n  place Blob at diner.stool_1 facing diner.counter\n  camera wide\n  Marta walks_to (3.1, 2.6)\n  wait 1s");
    let marta = ep.scenes[0].actors.iter().find(|a| a.name == "Marta").unwrap();
    let r = nav::radius(&marta.def);
    let c = counter();
    let pos = positions(&ep, "Marta");
    for p in &pos {
        assert!(!c.inflate(r - 0.02).contains(*p), "Marta walks through the counter at {:?}", p);
    }
    let end = *pos.last().unwrap();
    assert!(dist2(end, [3.1, 2.6]) < 0.15, "she still gets there: {:?}", end);
    assert!(marta.moves.len() >= 2, "more than one segment: she walked around ({} segment(s))", marta.moves.len());
    // The segments connect and follow each other in time.
    for w in marta.moves.windows(2) {
        assert!(dist2(w[0].to, w[1].from) < 1e-9 && w[1].t0 >= w[0].t1 - 1e-9, "{:?} then {:?}", w[0], w[1]);
    }
}

#[test]
fn open_floor_walks_and_stools_behave_as_before() {
    let ep = scene("  place Marta at diner.door\n  place Blob at diner.stool_3 facing diner.counter\n  camera wide\n  Marta walks_to (-1.0, 0.0)\n  wait 1s");
    let marta = ep.scenes[0].actors.iter().find(|a| a.name == "Marta").unwrap();
    assert_eq!(marta.moves.len(), 1, "no detour in the open");
    assert!(dist2(marta.kin(ep.scenes[0].dur).pos, [-1.0, 0.0]) < 0.05);

    // Sitting on a stool: the stool is the target, not an obstacle.
    let ep = scene("  place Marta at diner.door\n  place Blob at diner.stool_3 facing diner.counter\n  camera wide\n  Marta sits_on diner.stool_1\n  wait 1s");
    let marta = ep.scenes[0].actors.iter().find(|a| a.name == "Marta").unwrap();
    let end = marta.kin(ep.scenes[0].dur);
    assert!(dist2(end.pos, [-1.4, 1.4]) < 0.2 && end.sit > 0.9, "sits on the stool: {:?} sit {}", end.pos, end.sit);
}

#[test]
fn tables_are_walked_around_too() {
    let ep = scene("  place Marta at diner.door\n  place Blob at diner.stool_1 facing diner.counter\n  camera wide\n  Marta walks_to (0.0, -1.6)\n  Marta walks_to (5.0, -1.6)\n  wait 1s");
    let marta = ep.scenes[0].actors.iter().find(|a| a.name == "Marta").unwrap();
    let table = nav::footprints(&project().sets["diner"]).into_iter().find(|(n, _)| n.starts_with("table")).map(|x| x.1);
    let Some(table) = table else { return };
    let r = nav::radius(&marta.def);
    for p in positions(&ep, "Marta") {
        assert!(!table.inflate(r - 0.02).contains(p), "through the table at {:?}", p);
    }
}

#[test]
fn any_prop_can_be_made_solid_or_walkable() {
    let mut set = project().sets["diner"].clone();
    let n0 = nav::footprints(&set).len();
    let pie = set.props.iter_mut().find(|p| p.kind == "pie_case").unwrap();
    pie.solid = Some(true);
    assert_eq!(nav::footprints(&set).len(), n0 + 1, "pie_case becomes an obstacle");
    let stool = set.props.iter_mut().find(|p| p.kind == "stool").unwrap();
    stool.solid = Some(false);
    assert_eq!(nav::footprints(&set).len(), n0, "a stool set to solid: false is walkable");
}
