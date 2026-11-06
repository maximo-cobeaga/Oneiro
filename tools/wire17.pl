undef $/;
sub edit { my ($f, $code) = @_; open(F, "<$f") or die $f; my $s = <F>; close F; local $_ = $s; $code->(); open(F, ">$f"); print F $_; close F; }

# toon3d: module, supported(), mesh dispatch
edit("crates/oneiro-toon3d/src/lib.rs", sub { s/pub mod props;/pub mod props;\npub mod stock;/ or die "lib"; });
edit("crates/oneiro-toon3d/src/props.rs", sub {
  s/\|\| \(pr\.kind == "model" && pr\.mesh\.is_some\(\)\)\)/|| crate::stock::stock(&pr.kind).is_some() || (pr.kind == "model" && pr.mesh.is_some()))/ or die "supported";
  s/(        "model" => \{\r?\n            if let Some\(obj\) = &pr\.mesh \{)/        k if crate::stock::stock(k).is_some() => {\n            if let Some(sm) = crate::stock::stock_mesh(pr) {\n                m = sm;\n            }\n        }\n$1/ or die "dispatch";
});

# core: prop kinds, 3D always for stock kinds, footprints
edit("crates/oneiro-core/src/catalog.rs", sub {
  s/"pie_case", "model"\];/"pie_case", "model", "tree", "pine", "bush", "rock", "house", "fence", "lamp_post", "bench", "sofa", "bed", "bookshelf", "tv", "car", "rug"];\n\n\/\/\/ Props that exist only as 3D geometry (always drawn with the toon 3D look).\npub const STOCK_3D_PROPS: &[&str] = &["tree", "pine", "bush", "rock", "house", "fence", "lamp_post", "bench", "sofa", "bed", "bookshelf", "tv", "car", "rug"];/ or die "kinds";
});
edit("crates/oneiro-core/src/defs.rs", sub {
  s/p\.kind == "model" \|\| p\.look/p.kind == "model" || crate::catalog::STOCK_3D_PROPS.contains(&p.kind.as_str()) || p.look/ or die "is3d";
});
edit("crates/oneiro-core/src/nav.rs", sub {
  s/(        \/\/ Anything else only blocks when asked to)/        "house" => (p.w.unwrap_or(5.0), p.d.unwrap_or(4.0)),\n        "car" => (p.w.unwrap_or(1.8), p.d.unwrap_or(4.2)),\n        "sofa" => (p.w.unwrap_or(2.0), p.d.unwrap_or(0.9)),\n        "bed" => (p.w.unwrap_or(1.5), p.d.unwrap_or(2.0)),\n        "bookshelf" => (p.w.unwrap_or(1.2), p.d.unwrap_or(0.35)),\n        "bench" => (p.w.unwrap_or(1.6), p.d.unwrap_or(0.55)),\n        "rock" => (p.w.unwrap_or(1.2) * 0.8, p.d.unwrap_or(0.9) * 0.8),\n        "tree" | "pine" => (0.5, 0.5),\n        "lamp_post" => (0.35, 0.35),\n$1/ or die "nav stock";
  s/pub fn footprints\(set: &SetDef\) -> Vec<\(String, Obb\)> \{\r?\n    set\.props\.iter\(\)\.filter_map\(\|p\| footprint\(p\)\.map\(\|o\| \(p\.name\.clone\(\), o\)\)\)\.collect\(\)\r?\n\}/pub fn footprints(set: &SetDef) -> Vec<(String, Obb)> {\n    let mut out: Vec<(String, Obb)> = set.props.iter().filter_map(|p| footprint(p).map(|o| (p.name.clone(), o))).collect();\n    for s in set.solids.iter().filter(|s| s.solid) {\n        let (hw, hd) = if s.shape == "cylinder" { (s.size[0] \/ 2.0, s.size[0] \/ 2.0) } else { (s.size[0] \/ 2.0, s.size[2] \/ 2.0) };\n        out.push((s.name.clone(), Obb { c: s.at, hw, hd, yaw: s.yaw }));\n    }\n    out\n}/ or die "footprints";
});

# render: solids as items; draw meshes
edit("crates/oneiro-render/src/lib.rs", sub {
  s/(        for \(i, sp\) in sc\.set\.sprites\.iter\(\)\.enumerate\(\) \{)/        for (i, sd) in sc.set.solids.iter().enumerate() {\n            let c = cam.to_cam([sd.at[0], sd.y + sd.size[1] * 0.5, sd.at[1]]);\n            items.push((c[2], Item::Solid(i)));\n        }\n$1/ or die "items";
  s/    Sprite\(usize\),\r?\n\}/    Sprite(usize),\n    Solid(usize),\n}/ or die "enum";
  s/(                Item::Sprite\(i\) => )/                Item::Solid(i) => {\n                    let mesh = oneiro_toon3d::stock::solid_mesh(&sc.set.solids[i]);\n                    draw_mesh3d(\&mut *p.pm, mesh, tint, px, \&cam);\n                }\n$1/ or die "arm";
  s/fn draw_prop3d\(pm: &mut Pixmap, set: &oneiro_core::SetDef, prop: usize, piece: usize, tint: \[f64; 3\], px: f32, cam: &Cam\) \{\r?\n    let mesh = oneiro_toon3d::props::prop_mesh\(set, &set\.props\[prop\], piece\);\r?\n/fn draw_prop3d(pm: &mut Pixmap, set: &oneiro_core::SetDef, prop: usize, piece: usize, tint: [f64; 3], px: f32, cam: &Cam) {\n    let mesh = oneiro_toon3d::props::prop_mesh(set, &set.props[prop], piece);\n    draw_mesh3d(pm, mesh, tint, px, cam);\n}\n\n\/\/\/ A world-space mesh drawn through the 3D toon renderer into a layer the size of its screen bounds.\nfn draw_mesh3d(pm: &mut Pixmap, mesh: oneiro_toon3d::Mesh, tint: [f64; 3], px: f32, cam: &Cam) {\n/ or die "split";
});
