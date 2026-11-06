//! Phase 7: cape and tail physics for `toon_3d` characters.

use oneiro_core::lock::Lock;
use oneiro_core::physics::{self, Frame, CAPE_COLS, CAPE_ROWS, TAIL_POINTS};
use oneiro_core::{compile_source, CharacterDef, Episode, Project};
use std::path::{Path, PathBuf};

fn demo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/toon3d_demo").canonicalize().unwrap()
}

fn episode(script: &str) -> Episode {
    let mut d = vec![];
    let p = Project::load(&demo(), &mut d);
    assert!(d.is_empty(), "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    let (ep, diags) = compile_source(&p, "script.onr", script, &Lock::default());
    assert!(diags.iter().all(|x| !x.is_error()), "{:?}", diags.iter().map(|x| x.render()).collect::<Vec<_>>());
    ep
}

fn demo_episode() -> Episode {
    episode(&std::fs::read_to_string(demo().join("script.onr")).unwrap())
}

/// A hero standing still for a while, then running across the room.
fn run_script() -> String {
    "oneiro 0.1\nproject \"t\"\nuse character Hero\nuse character Squirrel\nuse set diner\nscene 1 \"s\" in diner {\n  place Hero at diner.door\n  place Squirrel at diner.counter\n  camera wide\n  wait 1.5s\n  together {\n    Hero runs_to (2.0, -0.5)\n    Squirrel runs_to (-2.0, 0.5)\n  }\n  wait 1s\n}\n".into()
}

#[test]
fn only_3d_characters_with_extras_are_simulated() {
    let ep = demo_episode();
    let sc = &ep.scenes[0];
    let by = |n: &str| sc.actors.iter().find(|a| a.name == n).unwrap();
    let hero = by("Hero").physics.as_ref().expect("Hero has a cape");
    assert!(!hero.cape.is_empty() && hero.tail.is_empty());
    assert_eq!(hero.cape[0].len(), CAPE_COLS * CAPE_ROWS);
    let sq = by("Squirrel").physics.as_ref().expect("Squirrel has a tail");
    assert!(sq.cape.is_empty() && !sq.tail.is_empty());
    assert_eq!(sq.tail[0].len(), TAIL_POINTS);
    assert!(by("Marta").physics.is_none() && by("Blob").physics.is_none(), "no extras, nothing to simulate");
    // Stored at 60 frames per second over the whole scene.
    let n = hero.cape.len() as f64;
    assert!((n / sc.dur - physics::STORE_HZ).abs() < 3.0, "{} frames for {:.1}s", n, sc.dur);

    // The same character in another style does not pay for physics.
    let cut = episode(&demo_episode_text().replace("use character Hero", "use character Hero style cutout_2d"));
    assert!(cut.scenes[0].actors.iter().find(|a| a.name == "Hero").unwrap().physics.is_none());
}

fn demo_episode_text() -> String {
    std::fs::read_to_string(demo().join("script.onr")).unwrap()
}

#[test]
fn the_cape_hangs_from_the_shoulders_and_never_goes_through_the_body_or_floor() {
    let ep = episode(&run_script());
    let sc = &ep.scenes[0];
    let hero = sc.actors.iter().find(|a| a.name == "Hero").unwrap();
    let ph = hero.physics.as_ref().unwrap();
    let def = &hero.def;
    let hh = def.height();
    let mut worst_e = f64::MAX;
    for (fi, frame) in ph.cape.iter().enumerate() {
        let t = fi as f64 / physics::STORE_HZ;
        let kin = hero.kin(t);
        let body = physics::body_at(def, &kin);
        let fr = Frame::new(kin.pos, kin.yaw);
        for p in frame {
            assert!(p.iter().all(|v| v.is_finite()), "frame {} has a NaN", fi);
            assert!(p[1] >= 0.019, "frame {}: the cape is below the floor ({})", fi, p[1]);
        }
        // Top row: on the shoulder line, behind the torso.
        for c in 0..CAPE_COLS {
            let (x, y, z) = fr.local(frame[c]);
            assert!((y - (body.shoulder - 0.02 * hh)).abs() < 0.02, "frame {} col {}: top edge at {} vs shoulders {}", fi, c, y, body.shoulder);
            assert!(z < 0.0 && x.abs() <= body.torso_w * 0.55, "frame {} col {}: ({}, {})", fi, c, x, z);
        }
        // Every other point stays outside the torso ellipse.
        let (a, b) = (body.torso_w * 0.5, body.torso_d);
        for p in &frame[CAPE_COLS..] {
            let (x, y, z) = fr.local(*p);
            if y > body.hip && y < body.shoulder {
                worst_e = worst_e.min((x / a).powi(2) + (z / b).powi(2));
            }
        }
        // Edge lengths: cloth, not elastic.
        for r in 0..CAPE_ROWS - 1 {
            for c in 0..CAPE_COLS {
                let (p, q) = (frame[r * CAPE_COLS + c], frame[(r + 1) * CAPE_COLS + c]);
                let d = ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt();
                let rest = hh * 0.46 / (CAPE_ROWS - 1) as f64;
                assert!(d > rest * 0.6 && d < rest * 1.35, "frame {} edge ({}, {}) is {:.3} (rest {:.3})", fi, r, c, d, rest);
            }
        }
    }
    assert!(worst_e > 0.9, "the cape went into the torso (ellipse value {})", worst_e);

    // At rest it hangs down: the hem is far below the shoulders.
    let kin = hero.kin(0.5);
    let fr = Frame::new(kin.pos, kin.yaw);
    let rest = ph.cape_at(0.5).unwrap();
    let hem = fr.local(rest[(CAPE_ROWS - 1) * CAPE_COLS + CAPE_COLS / 2]);
    let top = fr.local(rest[CAPE_COLS / 2]);
    assert!(top.1 - hem.1 > 0.3 * hh, "hem {:?} vs top {:?}", hem, top);
}

#[test]
fn moving_makes_the_cape_and_the_tail_trail_behind() {
    let ep = episode(&run_script());
    let sc = &ep.scenes[0];
    let hero = sc.actors.iter().find(|a| a.name == "Hero").unwrap();
    let sq = sc.actors.iter().find(|a| a.name == "Squirrel").unwrap();
    let hem_z = |t: f64| {
        let kin = hero.kin(t);
        let fr = Frame::new(kin.pos, kin.yaw);
        let pts = hero.physics.as_ref().unwrap().cape_at(t).unwrap();
        (CAPE_COLS * (CAPE_ROWS - 1)..CAPE_COLS * CAPE_ROWS).map(|i| fr.local(pts[i]).2).sum::<f64>() / CAPE_COLS as f64
    };
    let (still, running) = (hem_z(1.0), hem_z(2.4));
    assert!(running < still - 0.05 * hero.def.height(), "the hem should stream backwards when running: still {:.3}, running {:.3}", still, running);

    // The tail: attached at the root, segments of constant length, and it lags (differs from its rest shape) while running.
    let ph = sq.physics.as_ref().unwrap();
    let tip_dev = |t: f64| {
        let kin = sq.kin(t);
        let body = physics::body_at(&sq.def, &kin);
        let fr = Frame::new(kin.pos, kin.yaw);
        let pts = ph.tail_at(t).unwrap();
        let (_, y, z) = fr.local(pts[TAIL_POINTS - 1]);
        let seg = body.hh * 0.085;
        // Rest tip: out the back and curled up (see physics.rs).
        let rest_z = -(body.torso_d + 0.01 * body.hh) - seg * (TAIL_POINTS - 1) as f64 * 0.92;
        ((z - rest_z).abs() + (y - (body.hip + 0.3 * (body.shoulder - body.hip) - 0.06 * body.hh + 0.045 * body.hh * (TAIL_POINTS as f64 - 3.0))).abs(), pts)
    };
    let (dev_still, _) = tip_dev(1.0);
    let (dev_run, pts) = tip_dev(2.3);
    assert!(dev_run > dev_still + 0.01, "the tail should lag while running: still {:.3}, running {:.3}", dev_still, dev_run);
    let seg = sq.def.height() * 0.085;
    for w in pts.windows(2) {
        let d = ((w[0][0] - w[1][0]).powi(2) + (w[0][1] - w[1][1]).powi(2) + (w[0][2] - w[1][2]).powi(2)).sqrt();
        assert!(d > seg * 0.7 && d < seg * 1.3, "segment {:.3} vs {:.3}", d, seg);
    }
}

#[test]
fn the_simulation_is_deterministic_and_independent_of_when_you_ask() {
    let a = episode(&run_script());
    let b = episode(&run_script());
    let (pa, pb) = (a.scenes[0].actors[0].physics.as_ref().unwrap(), b.scenes[0].actors[0].physics.as_ref().unwrap());
    assert_eq!(pa.cape, pb.cape, "same script, same cape");
    // Sampling between stored frames interpolates; sampling at the ends is safe.
    let mid = pa.cape_at(1.0 / physics::STORE_HZ * 3.5).unwrap();
    let (f3, f4) = (&pa.cape[3], &pa.cape[4]);
    for i in 0..mid.len() {
        for k in 0..3 {
            assert!((mid[i][k] - (f3[i][k] + f4[i][k]) / 2.0).abs() < 1e-9);
        }
    }
    assert!(pa.cape_at(-1.0).is_some() && pa.cape_at(1e6).is_some());
}

#[test]
fn toon3d_extras_are_read_validated_and_saved() {
    let src = "character Zed {\n  rig: library.rigs.biped_cutout\n  style: toon_3d\n  toon3d { cape: #C02030, tail: #E0782A }\n}\n";
    let (file, mut d) = oneiro_lang::parse(src, "Zed.onchar");
    let c = CharacterDef::from_block(&file.blocks[0], "Zed.onchar", &mut d);
    assert!(d.is_empty(), "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    assert!(c.toon3d.cape.is_some() && c.toon3d.tail.is_some() && c.style == "toon_3d");
    let again = c.to_source();
    assert!(again.contains("toon3d { cape: #C02030, tail: #E0782A }"), "{}", again);

    let bad = "character Zed {\n  rig: library.rigs.biped_cutout\n  toon3d { wings: #FFFFFF, cape: red }\n}\n";
    let (file, mut d) = oneiro_lang::parse(bad, "Zed.onchar");
    let _ = CharacterDef::from_block(&file.blocks[0], "Zed.onchar", &mut d);
    assert_eq!(d.iter().filter(|x| x.code == "E1140").count(), 2, "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    assert!(d.iter().any(|x| x.message.contains("unknown toon3d option 'wings'") && x.suggestion.as_deref().unwrap_or("").contains("cape, tail")));
}
