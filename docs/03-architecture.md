# Oneiro Architecture — draft 0.1

## 1. Pipeline

```
.onr files -> parse -> AST -> world model -> solve (layout + camera) -> timeline
          -> per-character style renderers -> compositor -> frames -> ffmpeg -> mp4
                                                       \-> realtime viewport (IDE)
audio:  voice/sfx/music -> lipsync (viseme track) -> timeline
```

The IDE viewport and the exporter use the **same renderer**, so preview = output.

## 2. Rust workspace

**Implemented in Phase 1** (5 crates): `oneiro-lang` (lexer, parser, diagnostics), `oneiro-core` (project/library loading,
world model, compiler, placement and camera solver, timelines, `.onlock`), `oneiro-audio` (WAV I/O, mixer, lipsync,
demo synth), `oneiro-render` (CPU renderer on tiny-skia: perspective sets, cutout characters, parallel deterministic
frames) and `oneiro-cli` (`check`, `render`, `still`, `demo-audio`, `vocab`; ffmpeg export).
The remaining crates below are planned and will be split out of these as they grow.

Planned layout:

| Crate | Responsibility |
|---|---|
| `oneiro-lang` | Lexer, parser, AST, diagnostics (JSON + human) |
| `oneiro-core` | World model, entities, state/continuity, timeline |
| `oneiro-solve` | Placement solver, camera solver, cinematography rules, `.onlock` |
| `oneiro-anim` | Rigs, IK, clips, locomotion, visemes, recording (puppeteering) |
| `oneiro-style` | Style trait + built-in styles (puppet2d, clay post, ink post, toon3d later) |
| `oneiro-render` | Backends: `wgpu` GPU and CPU rasterizer fallback; compositor |
| `oneiro-audio` | Playback, recording, mixing, import, effects, SFX/music generators (Phase 4, see 07-audio.md) |
| `oneiro-voice` | Text-to-speech providers and the script voice planner (Phase 4) |
| `oneiro-lipsync` | Audio -> phoneme/viseme track (local, provider-independent) |
| `oneiro-voice` | Voice provider trait: file, ElevenLabs, local TTS |
| `oneiro-ai` | Provider trait, prompt/context assembly, tool surface |
| `oneiro-mcp` | MCP server exposing engine tools to any agent |
| `oneiro-export` | Frame sequence + ffmpeg muxing |
| `oneiro-cli` | `oneiro check | preview | render | new` |
| `oneiro-ide` | egui app: editor, viewport, timeline, library, studios |

## 3. Rendering

- **Per-character styles (Phase 5)**: each character is drawn by its own drawer, chosen by `def.style` - `cutout_2d`
  (library blocks), `rubber_hose` (noodle limbs on a shared skeleton, `pose.rs`), vector puppets (`puppet.rs`, SVG parts) - and
  `toon_3d` (Phase 7) builds real 3D geometry from the same definition, and `clay` is a post-process on any of them (layer, silhouette-as-height-map lighting, attached texture, stop-motion on twos).
  Runs on CPU.
- **Whole-frame post styles** (`ink`, grain) operate on any frame source and are still planned.
- **toon_3d (Phase 7)**: procedural 3D characters with cel shading and ink outlines, drawn by a CPU rasterizer or by wgpu on the GPU, with cape and tail physics simulated at compile time. See 10-toon3d.md.
- **Compositor**: characters are drawn back to front with the set; `clay` characters go through a transparent layer first.

## 4. AI layer

Providers are pluggable behind one trait. Three kinds:

1. **API providers** — user supplies an API key (Anthropic, OpenAI-compatible, etc.).
2. **Local models** — any OpenAI-compatible local endpoint (Ollama, llama.cpp server, LM Studio).
3. **Agent CLI bridge** — the IDE launches the user's own installed agent CLI (Claude Code / Codex) as a subprocess
   and talks to it over the engine's MCP server. Authentication stays entirely in that CLI under the user's own
   login. Oneiro never reads, stores or forwards subscription tokens. Check each vendor's current terms before release.

Everything the AI can do goes through the same **tool surface** as the IDE (create character, edit script, check,
render preview frame, return screenshot), so the AI can see its own output and self-correct.

## 5. Character creation tiers

| Tier | Phase | Description |
|---|---|---|
| A. Block builder | MVP | Assemble parts from libraries with parameters; AI picks and tunes |
| B. AI-authored vector parts | **Done (Phase 5)** | AI writes four SVG parts; the engine validates them (viewBox, pivots, safe subset) and rigs them into a puppet with a procedural lip-synced face. See 08-styles.md |
| C. Hybrid cutter | **Done (Phase 6)** | From an illustration to a rigged character, with human and AI validation. See 09-cutter.md |

### C — Hybrid cutter (design)

Input: an image of a character (generated or drawn). Human and AI cooperate:

1. Vision model proposes pieces, joints and layer order; the IDE overlays **numbered markers** on the image.
2. The user drags, merges, splits and re-labels pieces and pivots; the AI re-validates after each change.
3. **Validator** checks: every piece is attached to a bone, hierarchy has no cycles, pivots lie inside or near the
   piece, left/right symmetry, overlap/occlusion gaps flagged.
4. Occluded areas are filled by duplicating/painting by hand, or by an optional image-model inpainting plugin.
5. Output is a normal `.onchar` + rig, indistinguishable from tier A/B characters.

## 6. Audio studio

Import, record, trim, multi-track timeline, SFX/music libraries, per-track volume/fade. Voice sources are
interchangeable; lipsync consumes the final audio. Output is mixed into the exported video.

## 7. Performance targets

- Preview of a 2-character 2D scene at 24 fps on integrated graphics / CPU only, 1080p preview at reduced quality.
- Offline render faster than real-time for 2D styles on a mid-range CPU.
- Single binary, no installer dependencies except optional ffmpeg.
