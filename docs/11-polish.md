# Phase 8: polish

Five small things that make finished shorts look and feel better.

## Walking around furniture

The counter, tables and stools have a footprint (an oriented rectangle, `crates/oneiro-core/src/nav.rs`). A character keeps
its own radius away from furniture (wider bodies stay farther), stops in front of a solid anchor such as `diner.counter` at
a distance that depends on its width, and walks *around* an obstacle instead of through it (shortest detour through the
corners, one movement segment per leg). Stools are targets, not obstacles, so `sits_on` still works. This fixed the
waiter standing half inside the bar at the end of `first_short`.

## Transitions

```
transition dissolve for 1s
cut to close on Blob
```

`transition KIND [for DUR]` goes on the line before the `camera` / `cut to` it applies to and is centred on the cut.
Kinds: `dissolve`, `fade_black`, `fade_white`, `wipe_left`, `wipe_right`. The default duration is 0.8 s. Both shots keep
playing across the blend (the outgoing one holds its last camera pose, the incoming one its first), and frames are still a
pure function of time. Diagnostics: `E1060` unknown kind; `W1060` a transition with no camera after it, or replaced by another.

## `ink` style

`style: ink` (or `use character X style ink`) re-inks a character as a pen drawing: three washes of one ink colour on paper,
diagonal hatching in the shadows and a heavy silhouette line. Like `clay`, it works on top of whatever base drawing the
character has and mixes with the other styles in one scene.

## `oneiro new`

`oneiro new my-show [--name "My Show"]` writes a manifest, a starter script and two characters plus the diner set. It checks
and renders as is, and refuses to overwrite an existing project.

## 3D shadows

`toon_3d` characters cast a soft, stretched shadow on the floor away from the key light instead of the round contact blob.

## Results

Tests: 187 automated tests across the workspace, all green. New ones cover the footprints and path planning, the waiter fix,
transitions (fade to black and white, dissolve, wipes, determinism), the `ink` look and `oneiro new`.

**Not verified:** transitions and `ink` were checked by tests and a still of the scaffolded project, not by watching long
exported videos; 3D shadows were not compared against other lighting setups.

Limitations: path planning only knows the counter, tables and stools (not walls or arbitrary props); a shadow direction is
fixed rather than following the set's lights; transitions blend whole frames, so they cannot be limited to part of the screen.
