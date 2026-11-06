# Roadmap — vertical slices

| Phase | Goal | Status |
|---|---|---|
| 0 | Specification | **Done** (docs 01–03, example script) |
| 1 | **First short (CLI only)** | **Done** — see results below |
| 2 | **IDE v1** | **Done** — see docs/05-ide.md and results below |
| 3 | **AI integration** | **Done** - see docs/06-ai.md |
| 4 | Audio studio | **Done** - see docs/07-audio.md and results below |
| 5 | Style expansion | **Done** - see docs/08-styles.md and results below |
| 6 | Hybrid cutter (tier C) | **Done** - see docs/09-cutter.md and results below |
| 7 | toon3d | **Done** - see docs/10-toon3d.md and results below |

## Phase 1 acceptance results

Measured on the development machine (16 logical cores). Rendering uses only the CPU.

| # | Criterion | Result |
|---|---|---|
| 1 | Example parses with zero diagnostics; broken variants give the expected codes | `cargo test`: `example_compiles_without_diagnostics`, `unknown_verb_suggests_the_right_one`, `bad_references_produce_specific_codes`, … |
| 2 | Two tier-A characters, one scene, ≥ 6 camera setups incl. shot/reverse-shot | 7 setups (establishing, medium, 2 over-the-shoulder, close, medium, wide) — `shot_reverse_shot_stays_on_one_side_of_the_line` |
| 3 | Characters walk to anchors, sit, gesture, change expression | `characters_walk_to_anchors_and_sit`; gestures: wave, cross arms, shrug, shake head; expressions per line/emote |
| 4 | Mouth shapes follow the WAV | `speech_drives_the_mouth_from_the_audio`, `speech_opens_the_mouth_and_silence_closes_it` |
| 5 | Deterministic output | `same_frame_is_byte_identical`, `parallel_render_matches_serial`; identical frames-hash with 1 and 16 threads |
| 6 | 30 s at 1080p24 without a dedicated GPU | 29.7 s / 713 frames: **9.9 s on 1 CPU thread (3.0x realtime)**, 3.8 s on 16 threads (7.7x); full mp4 with audio in ~7 s |

Run it: `cargo run --release -p oneiro-cli -- render examples/first_short -o out/first_short.mp4`.

## Known limitations of the Phase 1 renderer (by design, to be addressed later)

- One style (`cutout_2d`); one rig (`biped_cutout`); characters are procedural cutouts from the parts catalog.
- Characters do not avoid props while walking (they only keep distance from each other and stay inside the set).
- No transitions (`dissolve`, `wipe`), no pick-up/hand-over interactions, no visual wounds.
- Lipsync is a spectral heuristic (loudness → opening, spectral centroid → shape), not phoneme recognition.
- The placeholder voices from `oneiro demo-audio` are formant babble, only meant for previews.

## Phase 2 results (IDE v1)

| Goal | Result |
|---|---|
| Viewport | Live render with the production renderer; shot camera and free orbit camera; click/drag characters |
| Script editor with diagnostics | Highlighting, line numbers, live recompile (350 ms), clickable problems with suggestions, directing palette |
| Timeline | Script / camera / per-character rows, scrubbing, zoom/pan, click-to-jump to script lines |
| Library browser | Episodes, characters with portraits, sets with previews, sounds |
| Block-based character builder | Parts, proportions, colors, wardrobe slots, live preview (turn, expression, mouth, talk, walk), saves `.onchar` |
| Record-motion mode | WASD puppeteering relative to the camera, path simplification, inserted as `walks_to (x, z)` / `runs_to (x, z)` |
| Pinning (no code) | Drag characters, edit/orbit cameras; stored as `pinned: true` in `.onlock` |
| Export | Background mp4 render with progress and cancel |

Tests: 43 automated tests pass (`cargo test --release`), including ground-ray picking, motion recording → script →
playback, pins round-tripping through the `.onlock`, scene-aware insertion and character serialization. The recording flow, the
palette, the diagnostics view and the export were also run inside the real application with the `--demo-*` switches and
inspected through screenshots (`docs/img/`).

Not in Phase 2 (by plan): audio playback and recording (Phase 4), AI panel (Phase 3), more styles (Phase 5).
Mouse interaction in the viewport and timeline was verified by unit-testing its logic and by screenshots, not by automated clicks.

## Phase 3 results (AI integration)

| Goal | Result |
|---|---|
| Provider trait, pluggable | Claude Code CLI, Codex CLI, Anthropic API, OpenAI-compatible/local (Ollama, LM Studio...), custom command |
| Claude first, via your own subscription | `claude -p` bridge (tools disabled, scratch folder); verified with real requests |
| Agent-CLI bridge | Claude Code and Codex run under the user's own login; no tokens read or stored |
| AI never has the last word | Director loop: engine validation, diagnostics fed back, rounds limit, diff + Apply/Reject/Undo in the IDE |
| Self-check via screenshots | Contact sheet of the shots attached for vision-capable providers; also exposed as MCP images |
| MCP server | `oneiro-mcp` with 10 tools (guide, read/write/check script, timeline, still, contact sheet, video, library, character); verified with Claude Code as a real MCP client |
| AI documentation | Generated guide + vocabulary + library summary + project notes (`oneiro guide`, `get_guide`) |
| Characters by AI | Same validate-and-retry loop for `.onchar`; opens in the builder as a draft |
| Human + agent in the same project | The IDE reloads the script when an agent changes it on disk, and warns on conflicts |

Tests: 82 automated tests pass across the workspace. Mock servers cover the HTTP providers; real runs covered the Claude/Codex bridges and the MCP interop.
Not verified with real services: Anthropic API, OpenAI-compatible servers, vision self-check with a real model, Codex as MCP client.

## Phase 4 results (audio studio)

| Goal | Result |
|---|---|
| Record | Microphone recording in the IDE with level meter, microphone selector, automatic cleanup and Listen/Keep/Discard; silent takes are detected |
| Import | wav directly; mp3/ogg/flac/m4a/aac/opus/video audio through ffmpeg; processed by kind (voice, sfx, music, raw) |
| SFX and music | 24 synthesized sound effects and seamless music loops in 5 moods, no external assets or licenses |
| Language | `music` loops, `fade_in`, `fade_out`, `once`, `music stop`; crossfade between tracks |
| Mix | One mixer (`Episode::render_audio`) for export, IDE playback, CLI and MCP: voice/sfx/music buses, ducking, soft limiter, level report |
| TTS | Windows speech (offline), ElevenLabs, OpenAI-compatible servers, custom command (Piper...), demo babble; per-character `voice { provider, voice, speed }`; emotion-aware ElevenLabs settings |
| Script integration | `oneiro voice` / IDE "Generate" fill missing voices and write the `audio` clauses; idempotent; errors reported per line without touching the script |
| IDE | Audio tab (mix timeline, dialogue, library, generators), synchronized playback with sound, voice settings window, voice preview in the character builder |
| AI | Guide teaches the audio statements; MCP tools `generate_sound`, `generate_voices`, `mix_audio` |

Tests: audio engine (13 + 2 device tests that use the real speaker and microphone), compiler audio semantics (6),
voice providers and planner (13, including real Windows speech and mock HTTP servers for ElevenLabs/OpenAI-style
servers) and the new MCP tools.
Verified in the real application: the Audio tab (screenshots), timeline playback through the speakers (playhead and
audio position within 5 ms after 1.6 s), voice generation from the IDE with Windows speech, `oneiro voice/mix/sound`
end to end.

**Not verified against real services:** ElevenLabs (no API key was available: the request format, the PCM response and
the error handling are tested against a local mock server), OpenAI-compatible TTS servers (same), Piper/espeak
commands (tested with a stand-in command), microphone capture of actual speech (the device opened and delivered
samples, but the default Windows microphone on the development machine is a virtual one that records silence).

Known limitations: the soundtrack is mono (no panning for `sfx at ANCHOR`); generated music is simple and loop-based;
the demo voice is still babble; lipsync is still the spectral heuristic; ducking is a fixed envelope, not side-chained
per speaker; the script is not saved automatically after voices are generated (Ctrl+S).

## Phase 5 results (styles and vector characters)

| Goal | Result |
|---|---|
| Per-character styles | `style default:`, `use character X style S`, `style:` in the `.onchar`; resolution order and `E2301` suggestions tested |
| `rubber_hose` | Noodle limbs, pie eyes, gloves, bounce/squash, 12 fps boil, on a skeleton shared with vector characters |
| `clay` | Plasticine shading over any character drawing, texture attached to the body, stop-motion on twos with jitter |
| Hybrid episodes | One scene can mix cutout, rubber hose, clay and vector characters; deterministic, parallel == serial |
| Tier B (AI-authored SVG) | Safe SVG subset parser (no scripts, images, `use`, gradients, external references), rig with pivots, procedural lip-synced face, engine validation with 8 error codes; `oneiro ai --character --svg`, IDE option, MCP `write_character_parts` |
| IDE | Style selector in the character builder, vector-character note, AI tab option with "Save to the library and open" |

Example: `examples/style_gallery` (hybrid and clay episodes, a hand-drawn vector robot `Pip`, and `Wisp`, a ghost drawn by Claude).

Tests: 141 automated tests across the workspace, including: SVG parser/validator (rejections, arcs, transforms), style resolution
and errors, style rendering (distinct looks, 12 fps holds, gestures x styles x characters), shading maths, hybrid
determinism, the AI loop with a mock provider (invalid drawings corrected on retry) and the MCP tool.
Verified for real: stills and the IDE of every style, the IDE with a vector character, the `claude-cli` bridge drawing a new
character (one round, about 10 s, rendered and animated), render speed at 1080p on 16 threads (hybrid 7.2x real time, clay 4.3x).

**Not verified:** AI drawing quality across models and prompts (one real run with Claude Code; the Anthropic API and OpenAI-compatible
providers use the same code path but were not run for this), vector characters in sets other than the diner, and `clay` on a machine without
many cores (single-thread 1080p clay is well below real time; the preview resolution is fine).

Known limitations: the diner is drawn identically in every style; vector characters have rigid arms and rotate (not fold) their legs when
sitting; `clay` in the IDE viewport is shaded but not "on twos" while dragging; no user-defined styles (`.onstyle`) yet.


## Phase 6 results (hybrid cutter)

| Goal | Result |
|---|---|
| Background removal | Learned from the image border and flood-filled, or the PNG's own alpha; keeps a floating head; explains failures |
| Layout proposal | Heuristic (neck, hips, torso, symmetry axis from the head) and a vision-model loop with the engine's validator and a numbered-box picture |
| Validator | 8 checks (`E1130`-`E1137`) in plain language: boxes, order, side, coverage, cut-off, proportions, body between the arms |
| Cut and rig | PNG pieces with pivots and relative heights, mirrored right arm and leg, written as a normal `.onchar` (raster `parts`); works with every style and night tint |
| IDE | Cutter tab: open, propose (automatic or AI), drag boxes, live checker, assembled-puppet preview, create |
| CLI / MCP | `oneiro cut`; tools `propose_cut` and `cut_character` |
| Providers | `claude-cli` can now see images (Read tool only, scratch folder) |

Tests: 157 automated tests across the workspace; the cutter has 12 tests (masks, proposals, validator,
JSON, overlay, jpeg via ffmpeg, end-to-end cut -> load -> render, names), the AI loop and MCP tools 3, and the IDE drag maths 2.
Verified for real: a fox drawn by Claude (cut by the heuristic, and by Claude's own vision in two rounds), the IDE tab, stills.

**Not verified:** photographs or painted/shaded illustrations (only flat-color cartoons were tried), characters not seen from the
front, Codex or Anthropic API vision for this flow (same code path, not run), and big batches of illustrations.

Known limitations: front-facing symmetric characters only; no inpainting of hidden areas; tails and accessories are not parts;
the illustration's face does not lip-sync; rigid limbs.


## Phase 7 results (toon_3d and physics)

| Goal | Result |
|---|---|
| `toon_3d` style | Real 3D characters (procedural meshes from the character definition), cel shading with soft bands, toon highlight, rim light and ink outlines; same skeleton, expressions, gaze and lip-sync as the other styles |
| Per-character and hybrid | Mixes with cutout, rubber hose, clay and vector characters in one scene; layers use the 2D set's camera and are composited by depth |
| Two renderers | A CPU software rasterizer (identical everywhere, no GPU needed) and a headless wgpu backend (same shading in WGSL, 4x MSAA); automatic choice with fallback; `--3d` / `ONEIRO_3D` to force |
| Physics | Verlet cloth cape (collides with torso and floor) and a spring-chain tail, simulated once per scene at compile time so frames stay a pure function of time |
| Format | `style: toon_3d` and `toon3d { cape: #RRGGBB, tail: #RRGGBB }` in `.onchar`; builder UI for both; AI guide and vocabulary updated |
| Example | `examples/toon3d_demo` (a hero with a cape, a squirrel with a tail, voices, music) |

Tests: 177 automated tests across the workspace (all green with the GPU and with `ONEIRO_3D=cpu`). New ones cover the meshes
(outward normals, winding, spans), the rasterizer (coverage, cel bands, outline width, depth order, clipping), GPU vs CPU
agreement on a sphere/capsule and on a real character with a cape, 3D characters inside real scenes (volume and ink, turning,
face only from the front, cape on the back and streaming, lip-sync, night tint, all looks in one reproducible scene) and the
physics (attachment, no penetration of the torso or floor, cloth edge lengths, trailing when running, determinism, extras parsing).
Verified for real: stills of every shot of the demo on the RTX 4060 (Vulkan) and with the CPU renderer, close-ups of faces, the render
speed (1080p, 16 threads: GPU 2.5x real time, CPU 1.6-2.3x; one thread at 720p: GPU 2.0x, CPU 0.7x).

**Not verified:** other graphics adapters and drivers (only an NVIDIA RTX 4060 on Vulkan; DirectX 12 and Metal use the same code
but were not run), machines without a GPU (the CPU path is the fallback and is tested), long scenes beyond a minute of physics,
and 3D characters in sets other than the diner.

Known limitations: only block characters become real 3D (vector and cut-out characters stay flat puppets); the set and props are
still 2D (no 3D shadows between them and characters); physics is limited to the cape and the tail; simple procedural meshes, no
imported models or textures.

## Phase 8 results (polish)

Done: walking around furniture (fixes the waiter inside the bar), `transition` between shots (dissolve, fade_black, fade_white,
wipe_left, wipe_right), the `ink` style, `oneiro new`, and soft 3D character shadows. Details, tests (187 green) and limitations
in [11-polish.md](11-polish.md).

## Phases 9-13 results (3D world)

Phase 9 solid props and time-of-day shadows, 10 3D props, 11 imported OBJ models, 12 paper-doll characters in `toon_3d`,
13 `oneiro doctor`, `examples/showcase` and QUICKSTART. Details and limitations in [12-world.md](12-world.md).

## Phase 14 results (launcher)

Oneiro Studio is now a double-clickable program: a start screen (new project, open project, recent projects, examples),
`File > New project` with two templates (starter, empty), `+ New set` and `+ New episode` in the library, no console window,
an icon, and `tools\package.ps1` that builds `dist\Oneiro` and creates Desktop and Start Menu shortcuts. A step-by-step test
route for a story from zero is in [13-first-story.md](13-first-story.md).

**Not verified:** the native folder-picker dialogs (Browse, Open project) were not clicked by me, only the screens and the
creation logic were tested; the exe has no embedded file icon (the window icon is set in code); it is unsigned, so Windows
SmartScreen may warn the first time.
