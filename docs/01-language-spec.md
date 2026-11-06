# Oneiro Language Specification — v0.1

Status: **implemented in Phase 1** unless a row says "planned". `oneiro vocab` prints the vocabulary the
engine currently accepts; the generated `docs/ai/vocabulary.md` of a project is the source of truth for AI tools.

## 1. Principles

1. **Reads like a screenplay.** English keywords, one statement per line.
2. **Declarative.** Describe what happens, never frames.
3. **Semantic first.** Relations (`facing`, `beside`) and shot names (`medium`) instead of coordinates.
4. **Deterministic.** Same files + same assets = same video (byte-identical frames, any thread count).
5. **AI-first.** One obvious way to say each thing. Strict parsing. Errors are structured, carry a suggestion,
   and are available as JSON (`oneiro check --json`).
6. **Style-agnostic.** The script never mentions how something is drawn, except to choose a style.

## 2. Lexical rules

- UTF-8. `#` starts a comment to end of line. `#RRGGBB` (6 or 3 hex digits) is a color.
- Statements end at newline. Blocks use `{ }`.
- Strings: `"double quoted"`. Numbers: `3`, `2.5`, `-0.5`. Units are suffixes: `m`, `s`, `ms`, `deg`.
- Identifiers: `snake_case` for vocabulary, anchors and props; `Capitalized` for characters.
- Dotted paths address things: `diner.counter`, `library.heads.round`.
- **World coordinates:** meters, x = east, y = up, z = north, origin at the center of the floor.
  Yaw 0 faces north, +90deg faces east. Anchors are `(x, z)` or `(x, y, z)`.

## 3. File header

```
oneiro 0.1
project "Moon Diner"
style default: cutout_2d
use character Marta
use character Blob style rubber_hose
use set diner
```

`use` loads from `library/` of the project. Every character and set must be declared before use.
`style default:` sets the look of the episode; `use character X style S` draws that character in another style
(`cutout_2d`, `rubber_hose`, `clay`), so different styles can share a scene. See [08-styles.md](08-styles.md).

## 4. Scenes

```
scene 1 "Late order" in diner {
  time: night
  place Marta at diner.stool_2 facing diner.counter
  place Blob  at diner.door
  ...
}
```

- Scene properties: `time: day | dusk | night`.
- Statements run **in sequence**. Parallel actions: `and` on one line, or a `together { }` block.
- `wait 2s`, `wait 500ms` pauses.
- `camera` and `cut to` statements take **no time**; `for <dur>` is the length of the camera move.
- Place all characters at the top of the scene (they exist from the first frame).

## 5. Placement

```
place <Entity> at <anchor|Entity> [facing <anchor|Entity|direction>]
place <Entity> <relation> <anchor|Entity> [<n>m away] [facing ...]
```

Relations: `at`, `beside`, `left_of`, `right_of`, `in_front_of`, `behind`, `near` (planned: `on`, `inside`).
Directions: `north south east west`, `camera` (faces the default camera), `away_from_camera`.
Anchors are declared in the `.onset`. Verbs that take a target (`walks_to`, `runs_to`, `turns_to`, `looks_at`, `points_at`) also accept a floor point `(x, z)`. An anchor with `seat: true` seats the character; `solid: true` makes
characters stop short of it (counters, walls); `facing:` is the pose on arrival.
Characters keep a respectful distance from each other automatically.

## 6. Actions

| Group | Verbs | Phase 1 |
|---|---|---|
| Locomotion | `walks_to`, `runs_to`, `turns_to`, `stops` | yes |
| Posture | `sits_on`, `stands` | yes |
| Gesture | `waves`, `points_at`, `shrugs`, `nods`, `shakes_head`, `crosses_arms`, `uncrosses_arms`, `facepalms` | yes |
| Gaze | `looks_at`, `looks_away` | yes |
| Targets | anchors, characters, directions, or a floor point `(x, z)` in meters, e.g. `Blob walks_to (1.5, 0.5)` (what the IDE writes when you record motion) | yes |
| Face | `emote <name> [intensity 0..1]` | yes |
| Manner | `slowly`, `quickly` change speed; other manner words are accepted | yes |
| Duration | `for 2s` overrides the default duration | yes |
| Planned | `walks_along`, `follows`, `lies_on`, `crouches`, `leans_on`, `stares_at`, `picks_up`, `drops`, `hands`, `opens`, `closes`, `pushes`, `hits` | no (E2010) |

Gestures are held until the next gesture when they say so (`crosses_arms`). Speakers automatically look at their
conversation partner and turn their body toward them if needed.
Expressions: `neutral happy sad angry annoyed surprised nervous bored confused scared laughing determined`.

## 7. Dialogue and sound

```
Marta (annoyed): "Where's my pie?" audio "audio/marta_01.wav"
sfx door_bell at diner.door
music "lofi_01" volume 0.4 fade_in 2s
music stop fade_out 2s
```

- The line lasts as long as its audio. **Lipsync is always computed from the final audio**, whatever its source.
  Without an `audio` clause the line is timed from its text and the mouth follows the letters.
- `sfx NAME` looks for `library/sounds/NAME.wav`; `music` for `library/music/NAME.wav`. Music loops until `music stop`, another `music` (crossfade) or the end of the episode (`once` plays it a single time; also `fade_in`/`fade_out`) and ducks under speech.
- `oneiro demo-audio` synthesizes placeholder voices and sounds so a project can be previewed without recordings; `oneiro voice` generates real voices (Windows speech, ElevenLabs, local servers). See [07-audio.md](07-audio.md).

## 8. Cameras

```
camera <shot> [on <Character>] [angle <angle>] [from <side|anchor>] [move <move>] [lens <mm>] [for <dur>]
camera over_shoulder Marta -> Blob
cut to medium on Marta
```

- Shots: `extreme_wide wide establishing full medium medium_close close extreme_close over_shoulder`.
- Angles: `eye_level low high dutch top_down`.
- Moves: `static pan pan_left pan_right tilt tilt_up tilt_down dolly_in dolly_out truck_left truck_right orbit crane_up handheld`.
- `from`: `north south east west` (world), `left right front back` (relative to the subject) or an anchor name.
- Framing is derived from the lens: the solver places the camera at the distance that gives the requested shot size.
- Dialogue shots keep to one side of the line between the two characters (**180-degree rule**), so a
  shot/reverse-shot pair always matches. Subject shots follow the subject if it moves.
- Resolved coordinates are stored in `<script>.onlock` (see `02-formats.md`).
- Planned: transitions (`dissolve`, `wipe`, `fade_to_black`).

## 9. State and continuity

```
Marta.mood = angry
give Marta key_red
wound Marta arm_left bandaged
relation Marta -> Blob = distrust
remember Marta "Blob ate her pie"
```

State accumulated by a scene is written to `state.out` next to the script. (Inheriting state between episodes
and showing wounds visually are planned.)

## 10. Diagnostics

Every diagnostic has a code, `file:line:col`, a message and usually a suggestion.

```
error[E1042] script.onr:20:3  unknown verb 'walk_to'
      suggestion: did you mean "walks_to"?
```

| Range | Meaning |
|---|---|
| E0xxx | Lexing and parsing (E0101 expected X, E0106 missing `}`) |
| E10xx | References and vocabulary (E1040 unknown character, E1041 not placed, E1042 unknown verb, E1044 unknown anchor, E1045 unknown shot/angle/move, E1046 unknown emotion, E1050 unknown set, E1051 unknown character file) |
| E11xx | Library definitions (E1103 unknown part, E1104 unknown wardrobe item, E1110 bad anchor) |
| E2xxx | Not available / assets (E2010 planned verb, E2301 style not available, E2401 audio file missing) |
| W3xxx | Warnings (W3001 pinned shot no longer matches the script) |

`oneiro check --json` prints them as a JSON array for AI consumption.
