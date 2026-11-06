undef $/;
sub edit { my ($f, $code) = @_; open(F, "<$f") or die $f; my $s = <F>; close F; local $_ = $s; $code->(); open(F, ">$f"); print F $_; close F; }

edit("crates/oneiro-core/src/scaffold.rs", sub {
  s/(const DINER: &str = [^\n]*\n)/$1const VILLAGE: &str = include_str!("..\/..\/..\/examples\/village\/library\/sets\/village.onset");\n/ or die "const";
  s/(pub fn diner_source)/\/\/\/ An open 3D village: houses, trees, a well and far hills.\npub fn village_source(name: &str) -> String {\n    VILLAGE.replacen("set village {", &format!("set {} {{", name), 1)\n}\n\n\/\/\/ An open field: sky, grass, a few trees and rocks, and room to build.\npub fn field_source(name: &str) -> String {\n    format!(\n        "# An open field in 3D. Add props (tree, pine, bush, rock, house, fence, bench...) and solids (box, cylinder, cone, sphere, roof, pyramid).\\nset {name} {{\\n  kind: open\\n  size: 16m x 12m\\n  sky: [#5BC0F8, #FFEFC4]\\n  ground: #8ED081\\n\\n  anchors {{\\n    start: (-5.0, -2.0)\\n    center: (0.0, 0.0)\\n  }}\\n\\n  props {{\\n    tree_1: library.props.tree {{ at: (-7.0, 3.0) }}\\n    tree_2: library.props.tree {{ at: (7.5, 4.0), height: 4.4 }}\\n    pine_1: library.props.pine {{ at: (-9.0, -1.0) }}\\n    bush_1: library.props.bush {{ at: (3.0, 3.0) }}\\n    rock_1: library.props.rock {{ at: (5.0, -3.0) }}\\n  }}\\n\\n  solids {{\\n    hill_1: {{ shape: sphere, at: (-6.0, 20.0), size: (24.0, 9.0, 12.0), color: #6CBF84 }}\\n    hill_2: {{ shape: sphere, at: (10.0, 22.0), size: (26.0, 12.0, 12.0), color: #58B070 }}\\n  }}\\n}}\\n"\n    )\n}\n\n$1/ or die "fns";
});

edit("crates/oneiro-ide/src/welcome.rs", sub {
  s/    pub diner: bool,\r?\n/    \/\/\/ 0 bare room, 1 diner, 2 open field, 3 village.\n    pub kind: usize,\n/ or die "field";
  s/            diner: false,\r?\n/            kind: 0,\n/ or die "init";
  s/                    ui\.radio_value\(&mut q\.diner, false, tr\("Bare room \(8 x 6 m, a window, a door and a table\)"\)\);\r?\n                    ui\.radio_value\(&mut q\.diner, true, tr\("Diner \(counter, stools, clock, pie case\)"\)\);/                    for (i, label) in [tr("Bare room (8 x 6 m, a window, a door and a table)"), tr("Diner (counter, stools, clock, pie case)"), tr("Open field (sky and grass, no walls, build your own 3D world)"), tr("Village (houses, trees, a well and hills in 3D)")].into_iter().enumerate() {\n                        ui.radio_value(&mut q.kind, i, label);\n                    }/ or die "ui";
  s/let text = if q\.diner \{ scaffold::diner_source\(&q\.name\) \} else \{ scaffold::room_source\(&q\.name\) \};/let text = match q.kind {\n                    1 => scaffold::diner_source(\&q.name),\n                    2 => scaffold::field_source(\&q.name),\n                    3 => scaffold::village_source(\&q.name),\n                    _ => scaffold::room_source(\&q.name),\n                };/ or die "create";
});

edit("crates/oneiro-ide/src/i18n.rs", sub {
  s/(    \("API key", "Clave de API"\),)/    ("Open field (sky and grass, no walls, build your own 3D world)", "Campo abierto (cielo y pasto, sin paredes, arma tu propio mundo 3D)"),\n    ("Village (houses, trees, a well and hills in 3D)", "Aldea (casas, árboles, un pozo y colinas en 3D)"),\n$1/ or die "i18n";
});
