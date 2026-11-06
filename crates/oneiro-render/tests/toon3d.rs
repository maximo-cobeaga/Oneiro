//! Phase 7: `toon_3d` characters inside real scenes.

use oneiro_core::lock::Lock;
use oneiro_core::{compile_source, Episode, Project};
use oneiro_render::{hash_pixmap, render_range, Renderer};
use std::path::{Path, PathBuf};
use tiny_skia::Pixmap;

fn demo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/toon3d_demo").canonicalize().unwrap()
}

fn gallery() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/style_gallery").canonicalize().unwrap()
}

fn episode_in(root: &Path, script: &str) -> Episode {
    let mut d = vec![];
    let p = Project::load(root, &mut d);
    assert!(d.is_empty(), "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    let (ep, diags) = compile_source(&p, "script.onr", script, &Lock::default());
    assert!(diags.iter().all(|x| !x.is_error()), "{:?}", diags.iter().map(|x| x.render()).collect::<Vec<_>>());
    ep
}

/// One character in the open, camera in front of it. `facing` is an optional `facing <target>` clause.
fn solo(who: &str, style: &str, facing: &str, extra: &str) -> String {
    solo_cam(who, style, facing, "", extra)
}

/// Like `solo` with extra camera words (for example `from south`).
fn solo_cam(who: &str, style: &str, facing: &str, cam: &str, extra: &str) -> String {
    format!("oneiro 0.1\nproject \"t\"\nuse character {who} style {style}\nuse set diner\nscene 1 \"s\" in diner {{\n  place {who} at diner.floor {facing}\n  camera full on {who} {cam}\n  {extra}\n  wait 3s\n}}\n")
}

/// A scratch copy of the demo project where the hero's cape is pure magenta, a color the diner never has.
fn magenta_demo(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("oneiro_toon3d_{}_{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&root);
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for e in std::fs::read_dir(from).unwrap().flatten() {
            let (p, q) = (e.path(), to.join(e.file_name()));
            if p.is_dir() {
                copy(&p, &q);
            } else {
                std::fs::copy(&p, &q).unwrap();
            }
        }
    }
    copy(&demo(), &root);
    let hero = std::fs::read_to_string(root.join("library/characters/Hero.onchar")).unwrap().replace("#C02030", "#FF00FF");
    std::fs::write(root.join("library/characters/Hero.onchar"), hero).unwrap();
    root
}

fn count(pm: &Pixmap, f: impl Fn(i32, i32, i32) -> bool) -> usize {
    pm.data().chunks(4).filter(|c| f(c[0] as i32, c[1] as i32, c[2] as i32)).count()
}

fn near(c: (i32, i32, i32), t: (i32, i32, i32), tol: i32) -> bool {
    (c.0 - t.0).abs() < tol && (c.1 - t.1).abs() < tol && (c.2 - t.2).abs() < tol
}

fn diff(a: &Pixmap, b: &Pixmap) -> usize {
    a.data().chunks(4).zip(b.data().chunks(4)).filter(|(x, y)| x != y).count()
}

#[test]
fn a_3d_character_is_drawn_with_volume_and_ink() {
    let root = demo();
    let render = |style: &str| Renderer::new(&episode_in(&root, &solo("Marta", style, "", "")), 640, 360).render_at(1.0);
    let (cut, d3) = (render("cutout_2d"), render("toon_3d"));
    assert!(diff(&cut, &d3) > 3000, "the two styles must look different");
    // Skin, coat and beanie are all there.
    for (name, rgb) in [("skin", (242, 201, 160)), ("coat", (45, 93, 168)), ("beanie", (208, 48, 47))] {
        assert!(count(&d3, |r, g, b| near((r, g, b), rgb, 70)) > 150, "{} missing in the 3D render", name);
    }
    // Ink outlines: dark pixels around the character.
    let ink = |pm: &Pixmap| count(pm, |r, g, b| r < 30 && g < 30 && b < 36);
    assert!(ink(&d3) > 400, "outline pixels: {}", ink(&d3));
    // Volume: the coat is shaded in bands, so it has several distinct tones (a flat cutout has one).
    let tones = |pm: &Pixmap| -> usize {
        let mut s = std::collections::BTreeSet::new();
        for c in pm.data().chunks(4) {
            if near((c[0] as i32, c[1] as i32, c[2] as i32), (45, 93, 168), 55) && c[2] > c[0] + 40 {
                s.insert(c[2] / 10);
            }
        }
        s.len()
    };
    assert!(tones(&d3) > tones(&cut), "3D shading gives the coat more tones ({} vs {})", tones(&d3), tones(&cut));
}

#[test]
fn the_character_really_turns_in_3d() {
    // Facing the camera and facing away (the camera stays on the south side) give different pictures, and the
    // face (eye whites, mouth) is only on the front.
    let root = demo();
    let render = |facing: &str| Renderer::new(&episode_in(&root, &solo_cam("Marta", "toon_3d", facing, "from south", "")), 640, 360).render_at(1.5);
    let (front, back) = (render(""), render("facing diner.counter"));
    assert!(diff(&front, &back) > 400);
    // Head region of the frame (the character is centered, the head is in the upper part).
    let head_whites = |pm: &Pixmap| {
        let mut n = 0;
        for y in 40..150u32 {
            for x in 240..400u32 {
                let p = &pm.data()[((y * 640 + x) * 4) as usize..][..4];
                if p[0] > 240 && p[1] > 240 && p[2] > 235 {
                    n += 1;
                }
            }
        }
        n
    };
    assert!(head_whites(&front) > head_whites(&back) + 20, "eye whites: front {} vs back {}", head_whites(&front), head_whites(&back));
}

#[test]
fn the_cape_hangs_behind_the_hero_and_streams_when_he_runs() {
    let root = magenta_demo("cape");
    let magenta = |r: i32, g: i32, b: i32| r > 150 && b > 120 && g < 90 && r > g + 90;
    let render = |facing: &str, cam: &str, extra: &str, t: f64| Renderer::new(&episode_in(&root, &solo_cam("Hero", "toon_3d", facing, cam, extra)), 640, 360).render_at(t);
    // The cape is on the hero's back: seen from behind it covers him, seen from the front it hides behind the body.
    let from_behind = render("facing diner.counter", "from south", "", 1.5);
    let from_front = render("", "from south", "", 1.5);
    assert!(count(&from_behind, magenta) > 200, "the cape covers the back of the hero: {}", count(&from_behind, magenta));
    assert!(count(&from_behind, magenta) > 2 * count(&from_front, magenta), "from the front it is mostly hidden ({} vs {})", count(&from_front, magenta), count(&from_behind, magenta));
    // Running makes it stream: a different silhouette from standing.
    let running = render("", "from east", "Hero runs_to (2.0, -0.5)", 1.2);
    let standing = render("", "from east", "", 1.2);
    assert!(count(&running, magenta) > 50 && diff(&running, &standing) > 1500);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_mouth_follows_speech_and_the_eyes_blink() {
    let root = demo();
    let ep = episode_in(&root, &solo("Marta", "toon_3d", "", "Marta (happy): \"Hello there, how are you doing today my friend?\""));
    let r = Renderer::new(&ep, 640, 360);
    let mut hashes = std::collections::HashSet::new();
    for k in 0..30 {
        hashes.insert(hash_pixmap(&r.render_at(0.2 + k as f64 * 0.06)));
    }
    assert!(hashes.len() > 8, "the 3D face animates while talking ({} distinct frames)", hashes.len());
}

#[test]
fn all_five_looks_share_a_scene_and_stay_reproducible() {
    // cutout, rubber hose, clay, a vector puppet and a 3D character, in one frame.
    let root = gallery();
    let script = "oneiro 0.1\nproject \"t\"\nuse character Marta\nuse character Blob style rubber_hose\nuse character Pip\nuse character Fox style clay\nuse character Wisp style toon_3d\nuse set diner\nscene 1 \"s\" in diner {\n  place Marta at diner.stool_1\n  place Blob at diner.counter\n  place Pip at diner.door\n  place Fox at diner.stool_3\n  place Wisp at diner.floor\n  camera wide from south move dolly_in for 3s\n  Wisp (nervous): \"Hello?\"\n  Pip waves\n  Fox shrugs\n  wait 1s\n}\n";
    let ep = episode_in(&root, script);
    let styles: std::collections::BTreeSet<String> = ep.scenes[0].actors.iter().map(|a| a.def.style.clone()).collect();
    assert_eq!(styles.len(), 4, "{:?}", styles);
    let r = Renderer::new(&ep, 320, 180);
    for f in [0usize, 20, 50] {
        assert_eq!(r.render_frame(f).data(), r.render_frame(f).data(), "frame {} is not reproducible", f);
    }
    let serial: Vec<u64> = (0..16).map(|i| hash_pixmap(&r.render_frame(i))).collect();
    let mut parallel = vec![];
    render_range(&r, 0..16, |_, pm| {
        parallel.push(hash_pixmap(pm));
        Ok(())
    })
    .unwrap();
    assert_eq!(serial, parallel, "parallel rendering must match serial");
}

#[test]
fn a_vector_character_asked_for_3d_still_renders_as_its_puppet() {
    let root = gallery();
    let script = solo("Pip", "toon_3d", "", "");
    let ep = episode_in(&root, &script);
    assert!(ep.scenes[0].actors[0].def.has_svg());
    let pm = Renderer::new(&ep, 320, 180).render_at(1.0);
    assert!(count(&pm, |r, g, b| near((r, g, b), (0x8f, 0xc3, 0xee), 26)) > 100, "Pip is drawn from his parts");
}

#[test]
fn night_scenes_tint_the_3d_characters_too() {
    let root = demo();
    let day = Renderer::new(&episode_in(&root, &solo("Marta", "toon_3d", "", "")), 480, 270).render_at(1.0);
    let night_script = solo("Marta", "toon_3d", "", "").replace("scene 1 \"s\" in diner {", "scene 1 \"s\" in diner {\n  time: night");
    let night = Renderer::new(&episode_in(&root, &night_script), 480, 270).render_at(1.0);
    let red = |pm: &Pixmap| -> f64 {
        let v: Vec<i32> = pm.data().chunks(4).filter(|c| near((c[0] as i32, c[1] as i32, c[2] as i32), (208, 48, 47), 80)).map(|c| c[0] as i32).collect();
        v.iter().sum::<i32>() as f64 / v.len().max(1) as f64
    };
    assert!(red(&night) < red(&day), "the beanie is darker at night: {} vs {}", red(&night), red(&day));
}
