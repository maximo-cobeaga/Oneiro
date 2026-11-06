# Open sets, painted scenery and the feedback round

## Open sets (`kind: open`)
A set no longer has to be a room. An open set has a sky, a ground, a painted **backdrop on each of the four sides** and
**sprites** (flat pictures that stand in the world and face the camera), like classic cartoons: painted backgrounds with a
few animated things. Because the four sides are different pictures, the camera can film from any side (`camera wide from east`).

```
set beach {
  kind: open
  size: 16m x 12m              # the play area; the backdrops stand beyond it
  sky: [#4CC3FF, #FFF3C9]      # top, horizon
  ground: #F0D58C
  backdrops { north: "backgrounds/north.svg"  east: "..."  south: "..."  west: "..." }
  anchors { start: (-5.0, -2.0)  shore: (0.0, 4.0) }
  sprites {
    palm_1: { image: "scenery/palm.svg", at: (-6.5, 3.5), height: 5.0, anim: sway, speed: 0.8 }
    gull_1: { image: "scenery/gull.svg", at: (-3.0, 7.0), height: 0.9, y: 4.5, anim: bob }
  }
}
```
- Pictures are SVG (the same subset as vector characters: shapes, flat colors, outlines, no gradients) or PNG. Backdrops must be
  wide (ratio 1.8 to 4.5); their bottom edge meets the ground, and the empty (transparent) part shows the sky gradient.
- Sprites: `height` in meters, `y` lift above the ground, `flip`, `anim` = `none | sway | bob | pulse | spin`, `speed`, `amount`.
- Everything is a pure function of time (frames stay reproducible) and works with night/dusk tinting and transitions.
- Errors: `E1123` sprite without image, `E1124` unknown animation, `E1125` bad kind/sky/backdrop side, `E1126` unreadable picture
  or a path outside the project.
- Example: `examples/beach` (four hand-made backdrops and seven sprites).

## The AI paints scenarios
AI tab > **Paint a scenario**: describe a place ("a beach at sunset", "a haunted forest", "a busy street"). The AI answers with
a `set` block plus one SVG per picture; the engine checks them (shapes, ratio, allowed SVG), feeds problems back, and shows
the result; **Add to the library** saves `library/sets/NAME.onset`, `backgrounds/` and `scenery/`. Then `use set NAME` in a script.

## Fixes from the first real use
- Windows no longer open terminal windows when the IDE talks to `claude`/`codex`, ffmpeg or PowerShell.
- **+ New character** worked only with a capitalized name and hid why; now any name starting with a letter works (it is capitalized).
- The AI prompt box scrolls instead of overflowing the panel.
- Vector (AI-drawn) characters: the face is now a decal on the head (it can no longer float off when the head turns); the neck,
  shoulders and arms follow the real drawing (margins in the SVG no longer leave gaps); the AI guide demands a neck, shoulders and
  `face { mode: auto }` so expressions and lip-sync work. The builder explains that blocks (head/body/wardrobe) do not apply to
  drawn characters.
- Automatic camera: it never films a face through another character or through furniture taller than the camera.
- Voices: each character's own voice (provider and voice id) is used by default, and **missing voices are generated automatically**
  when you press Play or Render (can be turned off in the Audio tab).
- Voice cloning from the Audio tab (ElevenLabs): upload recordings of a person who agreed to it, give it a name, optionally assign it
  to a character. Not tested with a real ElevenLabs account.
- The interface is available in Spanish (View > Language; it follows Windows the first time).
- A logo (`docs/img/logo.svg`, `.png`), used for the window icon and the start screen.

## Not done / limits
- Sprites do not cast shadows and are not hidden behind characters' bodies except by drawing order (nearer ones draw on top).
- The Spanish translation covers fixed interface texts; messages that include data (file names, counts) are still English.
- Characters do not avoid each other while walking, only furniture.
- A backdrop is a flat picture: moving the camera across the play area does not change what is seen in it.
