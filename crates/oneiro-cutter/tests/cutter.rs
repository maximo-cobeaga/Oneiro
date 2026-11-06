//! The hybrid cutter end to end: an illustration goes in, an animated Oneiro character comes out.

use oneiro_core::lock::Lock;
use oneiro_core::{compile_source, Project};
use oneiro_cutter::{cut, extract_mask, propose, validate, write_character, Image, Issue, Layout, RectN, WriteOpts};
use oneiro_render::character::{draw_character_ex, Placement, View};
use oneiro_render::Renderer;
use std::path::{Path, PathBuf};
use tiny_skia::{Color, Pixmap};

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
    let root = std::env::temp_dir().join(format!("oneiro_cutter_{}_{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&root);
    copy_dir(&gallery(), &root);
    root
}

/// A front-facing illustration of one of the gallery characters on a flat backdrop, drawn with the
/// engine itself (so the expected result is known).
fn illustration(who: &str, bg: (u8, u8, u8), transparent: bool) -> Image {
    let mut d = vec![];
    let project = Project::load(&gallery(), &mut d);
    let script = format!("oneiro 0.1\nproject \"t\"\nuse character {who}\nuse set diner\nscene 1 \"s\" in diner {{\n  place {who} at diner.floor\n  camera full on {who}\n  wait 2s\n}}\n");
    let (ep, diags) = compile_source(&project, "script.onr", &script, &Lock::default());
    assert!(diags.iter().all(|x| !x.is_error()));
    let sc = &ep.scenes[0];
    let fr = sc.frames_at(0.5).remove(0);
    let (w, h) = (900u32, 1200u32);
    let mut pm = Pixmap::new(w, h).unwrap();
    if !transparent {
        pm.fill(Color::from_rgba8(bg.0, bg.1, bg.2, 255));
    }
    let def = &sc.actors[0].def;
    let view = View { front: 1.0, side: 0.0, h_front: 1.0, h_side: 0.0, eye_dx: 0.0, eye_dy: 0.0, target_side: 1.0, target_up: 0.0 };
    let pl = Placement { sx: w as f32 / 2.0, sy: h as f32 * 0.93, scale: h as f32 * 0.82 / def.height() as f32 };
    draw_character_ex(&mut pm, def, &fr, &view, &pl, [1.0, 1.0, 1.0], h as f32 / 1080.0, 1.0);
    Image { pm }
}

fn errors(v: &[Issue]) -> Vec<String> {
    v.iter().filter(|i| i.is_error()).map(|i| i.render()).collect()
}

#[test]
fn the_background_is_removed_and_the_character_is_found() {
    for (who, bg) in [("Marta", (232, 232, 238)), ("Blob", (250, 240, 200)), ("Pip", (30, 30, 40))] {
        let img = illustration(who, bg, false);
        let m = extract_mask(&img, None).unwrap_or_else(|e| panic!("{}: {}", who, e));
        let (x0, y0, x1, y1) = m.bounds().unwrap();
        let h = (y1 - y0) as f32 / 1200.0;
        assert!((0.70..0.95).contains(&h), "{}: character height {} of the image", who, h);
        assert!(x0 > 100 && x1 < 800, "{}: the backdrop must not be part of the mask ({}..{})", who, x0, x1);
        // Backdrop corners are transparent, the middle of the body is solid.
        assert_eq!(m.a[0], 0);
        assert!(m.solid(450, 700));
    }
}

#[test]
fn real_transparency_is_used_as_is() {
    let img = illustration("Marta", (0, 0, 0), true);
    let m = extract_mask(&img, None).unwrap();
    let (_, y0, _, y1) = m.bounds().unwrap();
    assert!((y1 - y0) as f32 / 1200.0 > 0.7);
}

#[test]
fn unusable_images_are_explained() {
    let flat = Image { pm: { let mut p = Pixmap::new(200, 200).unwrap(); p.fill(Color::from_rgba8(240, 240, 240, 255)); p } };
    assert!(extract_mask(&flat, None).unwrap_err().contains("could not find a character"));
    // A busy photo-like backdrop: the "character" would fill the whole picture.
    let mut busy = Pixmap::new(200, 200).unwrap();
    for (i, p) in busy.data_mut().chunks_mut(4).enumerate() {
        let v = ((i * 7919) % 251) as u8;
        p.copy_from_slice(&[v, 255 - v, (v / 2).wrapping_add(((i / 200) as u8).wrapping_mul(9)), 255]);
    }
    assert!(extract_mask(&Image { pm: busy }, None).is_err());
    assert!(Image::from_png(b"not a png").is_err());
}

#[test]
fn the_proposal_is_valid_and_the_pieces_are_the_right_shape() {
    for who in ["Marta", "Blob"] {
        let img = illustration(who, (232, 232, 238), false);
        let m = extract_mask(&img, None).unwrap();
        let layout = propose(&m).unwrap();
        let issues = validate(&layout, &m);
        assert!(errors(&issues).is_empty(), "{}: {:?}\n{}", who, errors(&issues), layout.to_json());
        assert!(layout.head.center().1 < layout.body.center().1 && layout.body.center().1 < layout.leg.center().1);
        assert!(layout.arm.center().0 < 0.5 && layout.leg.center().0 < 0.5, "arm and leg are the screen-left ones");
        let c = cut(&img, &m, &layout).unwrap();
        assert_eq!(c.pieces.len(), 4);
        let h = |n: &str| c.pieces.iter().find(|p| p.name == n).unwrap().height;
        let total = h("head") + h("body") + h("leg");
        assert!((0.85..1.2).contains(&total), "{}: head+body+leg = {} of the character height", who, total);
        for p in &c.pieces {
            assert!(p.pm.width().max(p.pm.height()) <= oneiro_cutter::MAX_PIECE_SIDE);
            assert!((0.0..=1.0).contains(&p.pivot.0) && (0.0..=1.0).contains(&p.pivot.1), "{} pivot {:?}", p.name, p.pivot);
        }
    }
}

#[test]
fn a_cut_character_loads_renders_and_looks_like_the_original() {
    let root = scratch("e2e");
    let img = illustration("Marta", (232, 232, 238), false);
    let m = extract_mask(&img, None).unwrap();
    let layout = propose(&m).unwrap();
    let c = cut(&img, &m, &layout).unwrap();
    let path = write_character(&root, "Martita", &c, &WriteOpts { height_m: 1.15, source: "marta.png".into(), ..Default::default() }).unwrap();
    assert!(path.ends_with("Martita.onchar") && root.join("library/parts/Martita/head.png").exists());

    let mut d = vec![];
    let project = Project::load(&root, &mut d);
    assert!(d.is_empty(), "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    assert!(project.characters["Martita"].has_svg(), "the PNG parts load");
    assert!(project.characters["Martita"].to_source().contains("face { mode: none }"), "the illustration keeps its own face");

    let script = |who: &str| format!("oneiro 0.1\nproject \"t\"\nuse character {who}\nuse set diner\nscene 1 \"s\" in diner {{\n  place {who} at diner.floor\n  camera full on {who}\n  {who} (happy): \"Hello there!\"\n  {who} waves\n  wait 1s\n}}\n");
    let render = |who: &str, t: f64| {
        let (ep, diags) = compile_source(&project, "script.onr", &script(who), &Lock::default());
        assert!(diags.iter().all(|x| !x.is_error()), "{:?}", diags);
        Renderer::new(&ep, 640, 360).render_at(t)
    };
    let (orig, cutout) = (render("Marta", 0.4), render("Martita", 0.4));
    // The same colors appear in both (skin, blue coat, red beanie).
    let count = |pm: &Pixmap, t: [i32; 3]| pm.data().chunks(4).filter(|c| (0..3).all(|i| (c[i] as i32 - t[i]).abs() < 30)).count();
    for (name, rgb) in [("skin", [242, 201, 160]), ("coat", [45, 93, 168]), ("beanie", [208, 48, 47])] {
        let (a, b) = (count(&orig, rgb), count(&cutout, rgb));
        assert!(b > 60 && b as f32 > a as f32 * 0.35, "{}: {} pixels in the original, {} in the cut character", name, a, b);
    }
    // It animates: waving changes the picture over time.
    assert_ne!(render("Martita", 0.4).data(), render("Martita", 1.6).data());
    // Seen in a night scene (tint path) and clay (post style) it still renders.
    let night = script("Martita").replace("scene 1 \"s\" in diner {", "scene 1 \"s\" in diner {\n  time: night");
    let (ep, d) = compile_source(&project, "script.onr", &night, &Lock::default());
    assert!(d.iter().all(|x| !x.is_error()));
    let _ = Renderer::new(&ep, 320, 180).render_at(0.5);
    let clay = script("Martita").replace("use character Martita", "use character Martita style clay");
    let (ep, d) = compile_source(&project, "script.onr", &clay, &Lock::default());
    assert!(d.iter().all(|x| !x.is_error()));
    let _ = Renderer::new(&ep, 320, 180).render_at(0.5);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_validator_tells_what_is_wrong() {
    let img = illustration("Marta", (232, 232, 238), false);
    let m = extract_mask(&img, None).unwrap();
    let good = propose(&m).unwrap();
    let codes = |l: &Layout| -> Vec<String> { validate(l, &m).iter().map(|i| format!("{}{}", i.code, if i.is_error() { "!" } else { "" })).collect() };
    assert!(errors(&validate(&good, &m)).is_empty());

    let mut l = good.clone();
    std::mem::swap(&mut l.head, &mut l.body);
    assert!(codes(&l).contains(&"E1132!".to_string()), "head/body swapped: {:?}", codes(&l));

    let mut l = good.clone();
    l.arm = good.arm.mirrored(0.5);
    assert!(codes(&l).contains(&"E1133!".to_string()), "arm on the right side: {:?}", codes(&l));

    let mut l = good.clone();
    l.head = RectN::new(-0.2, 0.0, 0.5, 0.3);
    assert!(codes(&l).contains(&"E1130!".to_string()));

    let mut l = good.clone();
    l.leg = RectN::new(0.0, 0.0, 0.1, 0.1);
    assert!(codes(&l).contains(&"E1131!".to_string()), "a box over the empty corner: {:?}", codes(&l));

    let mut l = good.clone();
    l.arm = RectN::new(0.2, 0.3, 0.21, 0.32);
    assert!(codes(&l).contains(&"E1130!".to_string()), "tiny box");

    // Boxes that cover too little of the character.
    let mut l = good.clone();
    l.body = RectN::new(good.body.x0 + 0.02, good.body.y0, good.body.x0 + 0.04, good.body.y1);
    l.leg = RectN::new(good.leg.x0, good.leg.y0, good.leg.x0 + 0.02, good.leg.y1);
    l.arm = RectN::new(good.arm.x0, good.arm.y0, good.arm.x0 + 0.01, good.arm.y0 + 0.05);
    assert!(codes(&l).contains(&"E1134!".to_string()), "coverage: {:?}", codes(&l));

    // Messages are plain sentences.
    assert!(validate(&l, &m).iter().any(|i| i.message.contains("not inside any box")));
}

#[test]
fn a_layout_is_exchanged_as_json_even_inside_a_reply() {
    let l = Layout { head: RectN::new(0.3, 0.05, 0.7, 0.3), body: RectN::new(0.35, 0.28, 0.65, 0.6), arm: RectN::new(0.2, 0.3, 0.36, 0.62), leg: RectN::new(0.32, 0.58, 0.5, 0.95) };
    let back = Layout::from_json(&format!("Here is my answer:\n```json\n{}\n```\nHope it helps!", l.to_json())).unwrap();
    for ((_, a), (_, b)) in l.rects().iter().zip(back.rects().iter()) {
        assert!((a.x0 - b.x0).abs() < 1e-3 && (a.y1 - b.y1).abs() < 1e-3);
    }
    assert!(Layout::from_json("no json").is_err());
    assert!(Layout::from_json("{\"head\": [0,0,1,1]}").unwrap_err().contains("missing \"body\""));
    assert!(Layout::from_json("{\"head\": [0,0,1], \"body\": [0,0,1,1], \"arm\": [0,0,1,1], \"leg\": [0,0,1,1]}").unwrap_err().contains("four numbers"));
}

#[test]
fn the_overlay_marks_every_box_with_its_number_color() {
    let img = illustration("Marta", (232, 232, 238), false);
    let m = extract_mask(&img, None).unwrap();
    let l = propose(&m).unwrap();
    let pm = oneiro_cutter::overlay::draw(&img, &l, Some(0.5));
    assert_eq!((pm.width(), pm.height()), (img.width(), img.height()));
    let near = |p: &[u8], c: (u8, u8, u8)| (p[0] as i32 - c.0 as i32).abs() < 12 && (p[1] as i32 - c.1 as i32).abs() < 12 && (p[2] as i32 - c.2 as i32).abs() < 12;
    for (i, col) in oneiro_cutter::overlay::COLORS.iter().enumerate() {
        assert!(pm.data().chunks(4).filter(|p| near(p, *col)).count() > 200, "box {} is not drawn", i + 1);
    }
}

#[test]
fn other_image_formats_are_converted_with_ffmpeg() {
    if std::process::Command::new("ffmpeg").arg("-version").output().is_err() {
        eprintln!("ffmpeg not installed: skipped");
        return;
    }
    let dir = std::env::temp_dir().join(format!("oneiro_cutter_jpg_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let img = illustration("Blob", (250, 240, 200), false);
    std::fs::write(dir.join("a.png"), img.png().unwrap()).unwrap();
    let ok = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-i"]).arg(dir.join("a.png")).arg(dir.join("a.jpg")).status().unwrap().success();
    assert!(ok);
    let jpg = Image::load(&dir.join("a.jpg")).unwrap();
    let m = extract_mask(&jpg, None).unwrap();
    assert!(errors(&validate(&propose(&m).unwrap(), &m)).is_empty(), "a jpeg (with compression noise) works too");
    assert!(Image::load(&dir.join("missing.jpg")).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn names_are_checked_before_anything_is_written() {
    let img = illustration("Marta", (232, 232, 238), false);
    let m = extract_mask(&img, None).unwrap();
    let c = cut(&img, &m, &propose(&m).unwrap()).unwrap();
    let root = scratch("names");
    for bad in ["marta", "Mar ta", "../Evil", ""] {
        assert!(write_character(&root, bad, &c, &WriteOpts::default()).unwrap_err().contains("not a valid character name"), "{}", bad);
    }
    assert!(!root.join("library/parts/marta").exists());
    let _ = std::fs::remove_dir_all(&root);
}

/// Not a test: writes the demo illustrations used by the docs and the example (`cargo test -p oneiro-cutter -- --ignored make_demo_art`).
#[test]
#[ignore]
fn make_demo_art() {
    let dir = gallery().join("art");
    std::fs::create_dir_all(&dir).unwrap();
    for (who, bg) in [("Marta", (232, 232, 238)), ("Blob", (250, 240, 200))] {
        std::fs::write(dir.join(format!("{}_illustration.png", who.to_lowercase())), illustration(who, bg, false).png().unwrap()).unwrap();
    }
}

/// Not a test: rasterizes an SVG illustration onto a flat backdrop (`ONEIRO_SVG_IN`, `ONEIRO_PNG_OUT`) so
/// foreign drawings can be fed to the cutter.
#[test]
#[ignore]
fn rasterize_svg_env() {
    let (Ok(inp), Ok(out)) = (std::env::var("ONEIRO_SVG_IN"), std::env::var("ONEIRO_PNG_OUT")) else { return };
    let doc = oneiro_core::svg::parse_svg(&std::fs::read_to_string(inp).unwrap()).unwrap();
    let (w, h) = (900u32, 1200u32);
    let mut pm = Pixmap::new(w, h).unwrap();
    pm.fill(Color::from_rgba8(244, 244, 240, 255));
    let [vx, vy, vw, vh] = doc.vb;
    let s = (w as f32 * 0.9 / vw).min(h as f32 * 0.92 / vh);
    let tf = tiny_skia::Transform::from_translate(w as f32 / 2.0 - vw * s / 2.0 - vx * s, h as f32 / 2.0 - vh * s / 2.0 - vy * s).pre_scale(s, s);
    for sh in &doc.shapes {
        let Some(path) = oneiro_render::puppet::seg_path(&sh.segs) else { continue };
        if let Some(c) = sh.fill {
            let mut p = tiny_skia::Paint::default();
            p.anti_alias = true;
            p.set_color_rgba8(c[0], c[1], c[2], c[3]);
            pm.fill_path(&path, &p, tiny_skia::FillRule::Winding, tf, None);
        }
        if let Some((c, wd)) = sh.stroke {
            let mut p = tiny_skia::Paint::default();
            p.anti_alias = true;
            p.set_color_rgba8(c[0], c[1], c[2], c[3]);
            let st = tiny_skia::Stroke { width: wd, line_cap: tiny_skia::LineCap::Round, line_join: tiny_skia::LineJoin::Round, ..Default::default() };
            pm.stroke_path(&path, &p, &st, tf, None);
        }
    }
    std::fs::write(out, pm.encode_png().unwrap()).unwrap();
}

#[test]
fn a_body_box_that_reaches_past_the_arms_is_rejected() {
    let img = illustration("Marta", (232, 232, 238), false);
    let m = extract_mask(&img, None).unwrap();
    let mut l = propose(&m).unwrap();
    assert!(!validate(&l, &m).iter().any(|i| i.code == "E1137"));
    l.body = RectN::new(l.arm.x0, l.body.y0, 1.0 - l.arm.x0 + 0.1, l.body.y1);
    let issues = validate(&l, &m);
    assert!(issues.iter().any(|i| i.code == "E1137" && i.is_error() && i.message.contains("between the arms")), "{:?}", issues.iter().map(|i| i.render()).collect::<Vec<_>>());
}
