# 3D worlds in the animation style (no closed rooms)

A set can now be a real 3D place in the same toon look as 3D characters (cel shading and ink outlines), with no walls or
ceiling: `kind: open`. The camera can go anywhere and every side shows real geometry, not a drawing on a wall.

## Three ways to build one (mix them freely)

**1. Stock props** (`library.props.KIND { at: (x, z), yaw: degrees, height/width/depth: meters, color: #RRGGBB }`):
`tree`, `pine`, `bush`, `rock`, `house`, `fence`, `lamp_post`, `bench`, `sofa`, `bed`, `bookshelf`, `tv`, `car`, `rug`, plus the
older `table`, `stool`, `counter`, `pie_case`. Their default size is a good one; scale them with `height`, `width`, `depth`.
Houses, cars, furniture, trees and rocks are obstacles that characters walk around; bushes, fences, rugs and TVs are not.

**2. Solids**: primitives to build anything else.
```
solids {
  tower:  { shape: cylinder, at: (4, 8), size: (3, 9, 3), color: #C9CED8 }
  roof:   { shape: cone, at: (4, 8), y: 9, size: (4, 2.5, 4), color: #C0504D }
  hill:   { shape: sphere, at: (-8, 22), size: (26, 11, 14), color: #6CBF84 }
  shed:   { shape: box, at: (-3, 5), size: (3, 2, 2), yaw: 20, color: #B5833F }
  shed_r: { shape: roof, at: (-3, 5), y: 2, size: (3.4, 0.9, 2.4), yaw: 20, color: #8A3B35 }
}
```
Shapes: `box`, `cylinder` (width = diameter), `cone`, `sphere` (an ellipsoid: flat and wide for a hill), `roof` (gabled), `pyramid`.
`y` is the height of the base, `solid: true|false` decides if people walk around it (boxes and cylinders are obstacles by default).
Errors: `E1150` bad entry, `E1151` unknown shape, `E1152` bad size.

**3. Pictures** (optional, for far decoration): painted backdrops and animated sprites, see [14-open-sets.md](14-open-sets.md).

## Sky and ground
`sky: [top, horizon]` and `ground: #RRGGBB`. Far hills and mountains are just big spheres and cones at z 20 to 40: they give depth
and a horizon from every side.

## In the IDE
- Library > **+ New set** now offers *Open field* and *Village* next to the room and the diner; the village is a complete example
  to edit.
- AI tab > **Paint a scenario** now builds 3D scenery from props and solids (and pictures only for what 3D cannot do well).
  The engine checks it (at least 6 pieces, valid shapes and sizes) and feeds problems back.
- Example: `examples/village` (14 props and 8 solids, rendered at about 8x real time in preview).

## Limits
- Pieces are simple primitives: it is a stylized diorama, not a modeling tool (imported `.obj` models still exist for props).
- Pieces sort against characters by their center, so a character standing exactly inside a very large piece may sort wrongly (walking
  paths already keep characters out of obstacles).
- Lighting is fixed (one key light), only the shadows of characters follow the time of day.
- The wheels of the car are simple discs; no doors that open or windows with glass.
