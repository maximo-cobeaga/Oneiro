//! The documentation the AI reads: language guide, vocabulary, library summary and project notes.
//! The same text is served to external agents through the MCP server (`get_guide`).

use oneiro_core::catalog::*;
use oneiro_core::Project;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Scene,
    Character,
    /// A character drawn as four SVG parts (tier B).
    Parts,
}

pub const LANGUAGE_GUIDE: &str = r#"# Oneiro language guide

Oneiro describes an animated scene as a screenplay. You write *what happens* (who is where, what they do, what they
say, how the camera frames it); the engine places things in 3D and renders cartoon characters. You never write
coordinates unless you must, and never write frames.

## File shape

```oneiro
oneiro 0.1
project "Moon Diner"
style default: cutout_2d

use character Marta
use character Blob
use set diner

scene 1 "Late order" in diner {
  time: night
  place Marta at diner.stool_2 facing diner.counter
  place Blob  at diner.door

  camera establishing wide from south move dolly_in for 4s
  sfx door_bell
  wait 2.5s
  Blob walks_to diner.counter

  cut to medium on Marta
  Marta (annoyed): "Where's my pie?"

  camera over_shoulder Marta -> Blob
  Blob looks_at diner.clock and shrugs
  Blob (nervous): "It's... coming."

  camera over_shoulder Blob -> Marta
  Marta crosses_arms
  Marta (angry): "It's been an hour."

  cut to close on Blob
  Blob emote surprised
  Blob (nervous): "Is it really?"

  camera wide from east move dolly_out for 5s
  Marta stands
  Marta walks_to diner.door
  wait 2s
}
```

## Rules

- One statement per line. Comments start with `#`. Strings use double quotes. Durations: `2s`, `500ms`.
- Every character and set must be declared with `use` before the scene. Only use characters and sets that exist
  in the library summary below. Do not invent new characters inside a scene script; ask for them to be created.
- Place every character at the top of the scene (`place X at <anchor> [facing <target>]`). Anchors are named points of
  the set, written `set.anchor` (for example `diner.counter`). Only use anchors listed for the set.
- Statements run in sequence. Run things at the same time with `and` on one line (`Blob looks_at diner.clock and shrugs`)
  or a `together { ... }` block.
- `camera ...` and `cut to ...` take no time. `for 4s` after a camera move is the length of the move. To let a shot
  run, add `wait`/dialogue/actions after it.
- Dialogue: `Name (emotion): "text"`. The line lasts as long as its audio. If there is no audio file, leave out the
  `audio` clause: the duration and mouth movement are estimated from the text. Only write `audio "path.wav"` for
  files that appear in the project's sound list. Never invent audio files.
- Sound: `sfx NAME [at anchor] [volume 0.7]` plays library/sounds/NAME.wav at that moment. `music NAME [volume 0.4]
  [fade_in 2s] [fade_out 2s] [once]` starts a looping track from library/music/NAME.wav (a new `music` crossfades the
  previous one); `music stop [fade_out 2s]` ends it. Music ducks automatically under dialogue. Use only sounds from the
  list below; when you need one that is missing, create it first (the `generate_sound` tool, or `oneiro sound`).
- Props can be made 3D (`look: toon_3d` on a prop or the whole set), solid (`solid: true`: characters walk around it) or an imported OBJ (`library.props.model { model: "library/models/x.obj", height: 1 }`). Only touch these when asked.
- Styles are per character: `style default: cutout_2d` sets the episode's look, and `use character Blob style rubber_hose`
  draws that one character differently (cutout_2d, rubber_hose, clay, ink, toon_3d). Different styles can share a scene. Only change
  styles when the user asks for a look.
- Floor points `(x, z)` in meters are valid targets: `Blob walks_to (1.5, 0.5)` (x east, z north, origin at the
  center of the set, whose size is given below).
- Characters keep their positions between statements: after `Blob walks_to diner.counter` Blob stays there.
- Characters automatically look at who they talk to and turn their body if needed, so you rarely need `looks_at`.

## Directing guidance

- Open a scene with a wide or establishing shot, then move in (medium, close) as the dialogue starts.
- For two people talking, alternate `camera over_shoulder A -> B` and `camera over_shoulder B -> A`, with
- Between two shots, `transition dissolve for 1s` (also fade_black, fade_white, wipe_left, wipe_right) goes on the line before the `camera`/`cut to` it applies to.
  `cut to close on X` for emotional beats. The engine keeps the shots on one side of the action (180-degree rule).
- Give the audience time: `wait 1s` after a punchline, a held shot before a camera move.
- Use gestures and expressions sparingly and on purpose (a shrug, crossed arms, `emote surprised`).
- Use `time: night | day | dusk` once at the top of a scene.
- Keep scene length reasonable (20-60 seconds unless asked otherwise).

## Common mistakes

- Using a verb, shot, angle, move, emotion or anchor that is not in the vocabulary (the checker will tell you).
- Forgetting `place` for a character who then acts.
- Writing `camera medium Marta` instead of `camera medium on Marta` or `cut to medium on Marta`.
- Putting several statements on one line without `and`.
- Declaring a character or set that does not exist in the library.
"#;

pub const CHARACTER_GUIDE: &str = r#"# Oneiro character format

A character is assembled from library blocks. Reply with ONE `character` block:

```onchar
character Luna {
  rig: library.rigs.biped_cutout
  build {
    head:  library.heads.round    { size: 1.0, skin_tone: #F2C9A0, hair: #5A3A22 }
    eyes:  library.eyes.ovals     { spacing: 0.4 }
    mouth: library.mouths.visemes_basic
    body:  library.bodies.kid     { height: 1.0 }
  }
  wardrobe {
    default: [red_beanie, blue_coat, jeans, black_shoes]
  }
  voice { provider: file, pitch: 235 }
  sheet {
    age: 9
    traits: [curious, stubborn]
    notes: "One or two sentences about personality and visual rules."
  }
  styles { cutout_2d: auto }
}
```

Rules:
- The name starts with a capital letter and has only letters, digits and underscores.
- `head` is round, square or oval (`size` 0.6-1.5; `skin_tone` and `hair` are #RRGGBB; `bald: 1` removes hair).
- `eyes` is dots, ovals or wide (`spacing` 0.2-0.7). `mouth` is always visemes_basic.
- `body` is kid, adult or round (`height` 0.6-1.6, optional `width` 0.7-1.5).
- `wardrobe` takes at most one item per slot (hat, top, bottom, shoes, accessory), only from the list below.
- `voice pitch` is in Hz (70-400): low for big/older characters, high for small/young ones.
- Optional `style: toon_3d` makes the character a real 3D toon figure (other styles: cutout_2d, rubber_hose, clay, ink), and
  `toon3d { cape: #C02030, tail: #E0782A }` adds a cloth cape and/or a springy tail (physics, 3D style only).
- Be faithful to the user's description; make unspecified choices that fit the character's personality.
"#;

/// How to author a character from vector parts (tier B).
pub const PARTS_GUIDE: &str = r##"# Oneiro vector characters (drawn parts)

Instead of library blocks you can DRAW a character: four SVG parts that Oneiro assembles into a puppet, animates
(walking, gestures, turning) and gives a face (eyes, eyebrows, lip-synced mouth). Reply with ONE `character` block
and FOUR svg blocks, each marked with its part name.

```onchar
character Pip {
  rig: library.rigs.biped_cutout
  build {
    body: library.bodies.kid { height: 1.0 }
  }
  parts {
    head { height: 0.30, pivot: (0.5, 0.97) }
    body { height: 0.33, pivot: (0.5, 0.97) }
    arm  { height: 0.30, pivot: (0.5, 0.08) }
    leg  { height: 0.36, pivot: (0.5, 0.04) }
    face { mode: auto, x: 0.5, y: 0.56, scale: 1 }
  }
  voice { provider: file, pitch: 300 }
  sheet { age: 3, traits: [curious], notes: "One or two sentences." }
}
```

```svg head
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 90">
  <rect x="9" y="14" width="82" height="74" rx="22" fill="#8fc3ee" stroke="#1b2a3a" stroke-width="3.5"/>
</svg>
```
(and the same for `svg body`, `svg arm`, `svg leg`)

Part rules (the engine checks every one and tells you what is wrong):
- Each part is a standalone SVG with a `viewBox` and the drawing FILLING that viewBox (use at least 50% of its height,
  keep everything inside it, margins small). The renderer scales the whole viewBox to the part's `height`, which is a
  FRACTION of the character's total height (head 0.22-0.32, body 0.28-0.38, arm 0.25-0.35, leg 0.30-0.42; head + body +
  leg should add up to about 1.0).
- `head`: drawn WITHOUT eyes, eyebrows or mouth when `face { mode: auto }` (Oneiro draws them at `x`,`y` = where the
  middle of the face is, 0..1 from the top-left of the head; `scale` 0.5-1.5 sizes the face). Use `face { mode: none }`
  only if the head drawing already contains its own face. Draw the head facing the viewer, hair and ears included.
- BODY MUST CONNECT: the body drawing includes a short NECK (a narrow rounded rectangle rising from the top-center of the
  torso, about 12% of the body height; the head is drawn over it) and rounded SHOULDERS that reach the full width of the
  torso at its top edge. Draw the torso so it touches the left, right and top edges of its viewBox (no empty margins), and
  the head so its chin touches the bottom of its viewBox. The arms hang from the shoulder corners, so a torso narrower than
  its viewBox leaves them floating. Animals: the snout/muzzle belongs to the head drawing; the mouth is still drawn by
  Oneiro (`face { mode: auto, x, y }` pointing at the muzzle), so it can talk and show expressions.
- ALWAYS use `face { mode: auto }`. With `mode: none` the character cannot lip-sync or change expression.
- `body`: the torso seen from the front, wider than the neck; its bottom edge is the hips.
- `arm`: ONE arm hanging straight down, a TALL viewBox (for example `0 0 24 100`), hand at the bottom. The same drawing
  is used for both arms. Pivot = the shoulder (top, `pivot: (0.5, 0.08)`).
- `leg`: ONE leg with the shoe/foot at the bottom, a tall viewBox (for example `0 0 30 100`). Pivot = the hip.
- `pivot` is (x, y) from 0 to 1 inside the viewBox: the joint the part turns around (head and body: bottom center,
  `(0.5, 0.97)`).
- Allowed SVG: `svg g path rect circle ellipse line polyline polygon` with `fill`, `stroke`, `stroke-width`,
  `opacity`, `transform`. Colors as `#RRGGBB`. NOT allowed: gradients, filters, text, images, `use`, scripts, CSS, clip
  paths, masks. Keep it simple and bold: flat colors and a dark outline (stroke 3-4 units) read best on screen.
- Do not write `file:` in `parts`: Oneiro saves the SVGs for you. Names: a capital letter, then letters, digits or
  underscores.
- `style: clay` or `style: rubber_hose` on the character is optional (see the vocabulary for styles).
"##;

pub fn vocabulary_text() -> String {
    let join = |v: &[&str]| v.join(", ");
    let mut s = String::from("## Vocabulary (the only words the engine accepts)\n\n");
    s.push_str(&format!("- Actions (verbs): {}\n", join(IMPLEMENTED_VERBS)));
    s.push_str(&format!("- Verbs that are NOT available yet (do not use): {}\n", join(PLANNED_VERBS)));
    s.push_str(&format!("- Shots: {}\n", join(SHOTS)));
    s.push_str(&format!("- Camera angles: {}\n", join(ANGLES)));
    s.push_str(&format!("- Camera moves: {}\n", join(MOVES)));
    s.push_str(&format!("- Camera `from` sides: {} (or an anchor name)\n", join(SIDES)));
    s.push_str(&format!("- Placement relations: {}\n", join(RELATIONS)));
    s.push_str(&format!("- Emotions / expressions: {}\n", join(EXPRESSIONS)));
    s.push_str(&format!("- Times of day: {}\n", join(TIMES)));
    s.push_str(&format!("- Manner words: {}\n", join(MANNER_WORDS)));
    s.push_str(&format!("- Heads: {}; eyes: {}; bodies: {}\n", join(HEADS), join(EYES), join(BODIES)));
    s.push_str(&format!("- Wardrobe items: {}\n", wardrobe_names().join(", ")));
    s.push_str(&format!(
        "- Visual styles (chosen per character: `use character X style S` in the script, or `style: S` in the character file): {}\n",
        STYLES.join(", ")
    ));
    s
}

fn list_sounds(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for sub in ["library/sounds", "library/music", "audio"] {
        if let Ok(rd) = std::fs::read_dir(root.join(sub)) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().map_or(false, |x| x == "wav") {
                    out.push(format!("{}/{}", sub, p.file_name().unwrap().to_string_lossy()));
                }
            }
        }
    }
    out.sort();
    out
}

pub fn library_summary(project: &Project) -> String {
    let mut s = String::from("## Project library\n\n### Characters\n");
    if project.characters.is_empty() {
        s.push_str("(none yet)\n");
    }
    for c in project.characters.values() {
        s.push_str(&format!(
            "- {}: {} body, height {:.2} m, wardrobe [{}], traits [{}]{}{}{}\n",
            c.name,
            c.body.kind,
            c.height(),
            c.wardrobe.join(", "),
            c.traits.join(", "),
            if c.style.is_empty() { String::new() } else { format!(", own style {}", c.style) },
            if c.has_svg() { ", drawn from vector parts".to_string() } else { String::new() },
            if c.notes.is_empty() { String::new() } else { format!(", notes: {}", c.notes) }
        ));
    }
    s.push_str("\n### Sets (coordinates in meters: x east, z north, origin at the center)\n");
    if project.sets.is_empty() {
        s.push_str("(none yet)\n");
    }
    for set in project.sets.values() {
        s.push_str(&format!("- {}: {} x {} m, ceiling {} m\n", set.name, set.width, set.depth, set.height));
        for (n, a) in &set.anchors {
            let mut flags = vec![];
            if a.seat {
                flags.push("seat".to_string());
            }
            if a.solid {
                flags.push("solid: characters stop in front of it".to_string());
            }
            if let Some(f) = a.facing {
                flags.push(format!("faces {:.0} deg", f.to_degrees()));
            }
            s.push_str(&format!(
                "    - anchor {}.{} at ({:.2}, {:.2}){}{}\n",
                set.name,
                n,
                a.pos[0],
                a.pos[2],
                if a.pos[1] > 0.0 { format!(" height {:.2}", a.pos[1]) } else { String::new() },
                if flags.is_empty() { String::new() } else { format!(" [{}]", flags.join("; ")) }
            ));
        }
        let props: Vec<String> = set.props.iter().map(|p| format!("{} ({})", p.name, p.kind)).collect();
        if !props.is_empty() {
            s.push_str(&format!("    - props: {}\n", props.join(", ")));
        }
    }
    s.push_str("\n### Sounds available for `sfx NAME` / `audio \"path\"`\n");
    let sounds = list_sounds(&project.root);
    if sounds.is_empty() {
        s.push_str("(none)\n");
    }
    for p in sounds {
        s.push_str(&format!("- {}\n", p));
    }
    s
}

/// Reads `docs/ai/*.md` (series bible, style guide...) except the generated vocabulary.
pub fn docs_context(root: &Path) -> String {
    let mut out = String::new();
    if let Ok(rd) = std::fs::read_dir(root.join("docs").join("ai")) {
        let mut files: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().map_or(false, |e| e == "md")).collect();
        files.sort();
        for f in files {
            let name = f.file_name().unwrap().to_string_lossy().to_string();
            if name == "vocabulary.md" || name == "guide.md" {
                continue;
            }
            if let Ok(t) = std::fs::read_to_string(&f) {
                out.push_str(&format!("\n## Project notes: {}\n\n{}\n", name, t.chars().take(12_000).collect::<String>()));
            }
        }
    }
    out
}

/// The complete system prompt for an AI session on `project`.
pub fn system_prompt(project: &Project, mode: Mode) -> String {
    let mut s = String::new();
    match mode {
        Mode::Scene => {
            s.push_str("You are the director's assistant inside Oneiro, a language and studio for animated series. You write and edit Oneiro scripts.\n\n");
            s.push_str(LANGUAGE_GUIDE);
        }
        Mode::Character => {
            s.push_str("You are the character designer inside Oneiro, a language and studio for animated series. You create characters from library blocks.\n\n");
            s.push_str(CHARACTER_GUIDE);
        }
        Mode::Parts => {
            s.push_str("You are the character illustrator inside Oneiro, a language and studio for animated series. You DRAW characters as SVG parts.\n\n");
            s.push_str(PARTS_GUIDE);
        }
    }
    s.push('\n');
    s.push_str(&vocabulary_text());
    s.push('\n');
    s.push_str(&library_summary(project));
    s.push_str(&docs_context(&project.root));
    s.push_str("\n## Output protocol\n\n");
    match mode {
        Mode::Scene => s.push_str(
            "Reply with at most three short sentences explaining what you did, then exactly ONE fenced block marked ```oneiro containing the COMPLETE script file (never a fragment, never `...`). If the user only asks a question, answer without a block.\n",
        ),
        Mode::Character => s.push_str(
            "Reply with at most two short sentences describing the design, then exactly ONE fenced block marked ```onchar containing the complete `character` definition.\n",
        ),
        Mode::Parts => s.push_str(
            "Reply with at most two short sentences describing the design, then exactly ONE fenced block marked ```onchar with the `character` definition and FOUR fenced blocks marked ```svg head, ```svg body, ```svg arm and ```svg leg. Nothing else.\n",
        ),
    }
    s
}

/// The guide as a standalone document (written to `docs/ai/guide.md` by `oneiro guide`).
pub fn guide_document(project: &Project) -> String {
    let mut s = String::from("# Oneiro AI guide (generated by `oneiro guide`; do not edit)\n\n");
    s.push_str(LANGUAGE_GUIDE);
    s.push('\n');
    s.push_str(CHARACTER_GUIDE);
    s.push('\n');
    s.push_str(&vocabulary_text());
    s.push('\n');
    s.push_str(&library_summary(project));
    s
}
