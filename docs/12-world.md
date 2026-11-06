# Phases 9-13: the 3D world

## Phase 9: solid props and light
- `solid: true` / `solid: false` on any prop makes it an obstacle that characters walk around (or not). Counter, tables
  and stools are solid by default; other props use their `width` and `depth`.
- Character shadows follow the time of day (`light_dir`): short and nearly straight down at night, medium by day, long and
  low at dusk. They are stretched, soft shadows for `toon_3d` characters and round contact shadows for the rest.

## Phase 10: 3D props
`look: toon_3d` on a prop, or on the whole set, draws the counter, stool, table and pie case as real meshes with the same
cel shading and ink outlines as 3D characters (`crates/oneiro-toon3d/src/props.rs`). `look: flat` keeps the 2D drawing;
clocks, windows and doors stay flat (they live on the walls). Error `E1120` for an unknown look.

## Phase 11: imported models
```
crate: library.props.model { model: "library/models/crate.obj", at: (1.6, 0.6), height: 0.9, yaw: 20, color: #B5833F, solid: true }
```
Wavefront `.obj` files (vertices and faces of any size, negative indices, up to 60000 triangles) are scaled to `height`,
stand on the floor and are painted with `color`. Normals are smooth across edges flatter than 50 degrees. Errors: `E1121` a
model prop without a file; `E1122` unreadable or invalid file, or a path outside the project. Example: `examples/models_demo`.
Materials, textures and groups in the file are ignored.

## Phase 12: paper dolls
A vector (SVG) or cut-out (PNG) character with `style: toon_3d` used to fall back to a flat drawing. Now it is a paper doll:
a thin card standing in the 3D scene that narrows as it turns away from the camera, shows a dark paper edge on the side it
turns toward and is slightly darker from behind (`crates/oneiro-render/src/paperdoll.rs`). It is a card, not a model: it
has no real depth and no cloth.

## Phase 13: showcase and doctor
- `oneiro doctor` lists what works on this machine: CPU threads, GPU, ffmpeg, Windows voices, API keys, `claude`/`codex`.
- `examples/showcase` uses almost everything at once (3D hero with a cape, ink, rubber hose, a paper doll, 3D props, an
  OBJ crate, dusk shadows, three transitions).
- [QUICKSTART.md](../QUICKSTART.md) is the short path from zero to a video.

## Results
Tests: 198 across the workspace, all green (also with `ONEIRO_3D=cpu` for the renderer). Verified by looking at stills of the demo and the showcase on an RTX 4060
(Vulkan) and by tests. **Not verified:** other GPUs, lighting that changes the shading of characters (only shadows follow
the time of day), OBJ files from many different exporters (only hand-made and generated ones).

Limitations: props on walls are always flat; counters are drawn as one piece, so a character standing exactly inside the
counter would sort incorrectly (path planning keeps them out); the key light for shading is still fixed.
