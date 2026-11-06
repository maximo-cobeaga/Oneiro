# Roadmap — vertical slices

| Phase | Goal | Status |
|---|---|---|
| 0 | Specification | **Done** (docs 01–03, example script) |
| 1 | **First short (CLI only)** | **Done** — see results below |
| 2 | **IDE v1** | **Done** — see docs/05-ide.md and results below |
| 3 | **AI integration** | **Done** - see docs/06-ai.md |
| 4 | Audio studio | **Done** - see docs/07-audio.md and results below |
| 5 | Style expansion | rubber_hose, clay post, per-character styles, hybrid episodes; character tier B (AI-authored SVG) |
| 6 | Hybrid cutter (tier C) | Assisted segmentation + validator in IDE |
| 7 | toon3d | wgpu 3D toon renderer; then physics and cloth for 3D only |

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