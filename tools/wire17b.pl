undef $/;
sub edit { my ($f, $code) = @_; open(F, "<$f") or die $f; my $s = <F>; close F; local $_ = $s; $code->(); open(F, ">$f"); print F $_; close F; }

edit("crates/oneiro-ai/src/sets.rs", sub {
  s/    if set\.backdrops\.is_empty\(\) \{\r?\n        diags\.push\(err\("E1127", "add a `backdrops` block with at least a `north` picture"\.into\(\)\)\);\r?\n    \}/    let pieces = set.props.iter().filter(|p| p.wall.is_none()).count() + set.solids.len() + set.sprites.len() + set.backdrops.len();\n    if pieces < 6 {\n        diags.push(err("E1127", format!("the scenario has only {} things in it; build it from at least 6 (props, solids, sprites)", pieces)));\n    }\n    if set.solids.len() > 120 || set.props.len() > 120 {\n        diags.push(err("E1127", "too many pieces: use at most 120 props and 120 solids".into()));\n    }/ or die "min";
});

edit("crates/oneiro-ai/src/guide.rs", sub {
  my $new = <<'END';
pub const SET_GUIDE: &str = r##"# Oneiro open sets (a scenario built from real 3D pieces)

A scenario is NOT a closed room: it is a sky, a ground and a world built from 3D PIECES in the toon style (cel shading and ink
outlines), so the camera can film from any side and nothing is a flat wall. You build it from three kinds of things, and
may add pictures for far-away decoration. Reply with ONE `set` block (plus one SVG block per picture, only if you use pictures).

```onset
set village {
  kind: open
  size: 18m x 14m
  sky: [#5BC0F8, #FFEFC4]
  ground: #8ED081
  anchors {
    start: (-6.0, -3.0)
    center: (0.0, -1.8)
    bench: (-3.0, 3.5)
  }
  props {
    house_1: library.props.house { at: (4.0, 5.0), yaw: 180 }
    tree_1: library.props.tree { at: (-9.0, -1.0), height: 4.4 }
    pine_1: library.props.pine { at: (9.0, 4.5) }
    bench_1: library.props.bench { at: (-3.0, 3.9), yaw: 180 }
    lamp_1: library.props.lamp_post { at: (2.0, -1.5) }
    car_1: library.props.car { at: (-4.0, -6.0), yaw: 90, color: #FFC93C }
  }
  solids {
    well_base: { shape: cylinder, at: (0.0, 0.0), size: (1.4, 0.9, 1.4), color: #B5B9C4 }
    well_roof: { shape: pyramid, at: (0.0, 0.0), y: 1.9, size: (1.9, 0.8, 1.9), color: #C0504D }
    hill_1: { shape: sphere, at: (-8.0, 22.0), size: (26.0, 11.0, 14.0), color: #6CBF84 }
    mountain: { shape: cone, at: (0.0, 30.0), size: (22.0, 16.0, 22.0), color: #8E9AC8 }
  }
}
```

Rules (the engine checks them and tells you what is wrong):
- `kind: open`. `size` is the PLAY AREA in meters (14m x 10m to 24m x 18m): where characters walk. x is east, z is north,
  (0, 0) is the center. Anchors are named points inside it (at least `start` and `center`); scripts use
  `place Marta at village.start`, `walks_to village.bench`, `camera wide from east`. Keep the area around the anchors FREE of
  big pieces so people can walk; put big things (houses, trees, hills) at the edges and beyond.
- `sky: [top, horizon]` and `ground: #RRGGBB` are flat colors; everything else is real geometry.
- STOCK PROPS (`library.props.KIND { at: (x, z), yaw: degrees, height/width/depth: meters, color: #RRGGBB }`): tree, pine, bush,
  rock, house, fence, lamp_post, bench, sofa, bed, bookshelf, tv, car, rug, and the old room ones: table, stool, counter. A
  prop's default size is a good one; scale it with `height`, `width`, `depth`. Use `yaw` so houses face the action (a house's
  door is on its local +z side: yaw 180 makes it face south). `color` recolors the main part.
- SOLIDS build anything else from primitives: `shape` is box, cylinder, cone, sphere, roof (a gabled roof), pyramid; `at: (x, z)`,
  `y` = height of its base above the ground, `size: (width, height, depth)` in meters, `yaw` in degrees, `color`. A cylinder
  uses the width as its diameter. A sphere is an ellipsoid (use a flat wide size for a hill, a tall one for a bush).
  `solid: true/false` decides whether people walk around it (boxes and cylinders are obstacles by default).
  Compose them: a shop = a box + a roof + a box door + boxes for windows + a sign; a boat = a flat box hull + a cone sail; a
  castle = boxes and cylinder towers with cone roofs; a lighthouse = a stack of cylinders with a cone top and a yellow sphere.
  Hills and mountains far away (z 20-40) are big spheres and cones: they give depth and a horizon.
- An INTERIOR is the same idea: `kind: open` with a floor color for `ground`, `box` solids as walls (thin and tall) with gaps for
  doors, and furniture props. Leave one side open for the camera or film from outside; it need not be closed.
- Use 12 to 60 pieces; vary sizes, colors and heights; group things (a few trees together, a fence along a path); put small
  props near the anchors and large ones far. Colors: bold and cheerful, like a modern cartoon. Do not stack pieces so that
  they overlap by accident.
- OPTIONAL PICTURES (far decoration, birds, a sun): `backdrops { north: "name" }` (a wide SVG, viewBox ratio 1.8 to 4.5, bottom
  edge on the ground, transparent sky) and `sprites { id: { image: "name", at: (x, z), height: m, y: m, anim: sway } }` with
  `anim` none, sway, bob, pulse or spin. Each name needs an ```svg NAME block (allowed: svg g path rect circle ellipse line
  polyline polygon, flat `#RRGGBB` colors, no gradients/text/filters). Prefer 3D pieces; use pictures only for what 3D cannot
  do well (clouds, birds, a far skyline).
- Names of pieces: letters, digits, underscores. Do not write file paths: Oneiro saves pictures for you.
"##;
END
  s/pub const SET_GUIDE: &str = r##".*?\n"##;\r?\n/$new/s or die "guide";
});
