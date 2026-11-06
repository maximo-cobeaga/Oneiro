//! The AI half of the hybrid cutter: a vision model looks at the illustration, proposes the four
//! boxes, and the engine's validator answers with plain-language problems and a picture with the
//! numbered boxes drawn on it, until the layout is acceptable. The person always has the last word
//! (in the IDE the boxes stay draggable).

use crate::director::{add_usage, DirectorOpts};
use crate::types::*;
use oneiro_cutter::{overlay, validate, Image, Issue, Layout, Mask};
use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};

pub struct CutProposal {
    pub layout: Layout,
    pub issues: Vec<Issue>,
    pub valid: bool,
    pub rounds: usize,
    pub reply: String,
    pub usage: (u64, u64),
}

const SYSTEM: &str = "You are the cutting assistant inside Oneiro, a studio for animated series. You are given an illustration of ONE character seen from the front, standing, arms at the sides. You must say where four parts are so that Oneiro can cut the character into a puppet.

Reply with ONLY a JSON object, no prose, with four boxes in NORMALIZED image coordinates (0 = left/top edge of the image, 1 = right/bottom edge), each as [x0, y0, x1, y1]:

{\"head\": [..], \"body\": [..], \"arm\": [..], \"leg\": [..]}

Rules:
- head: the whole head including hair, hat, ears and antennae, ending at the neck.
- body: the torso only, from the neck to the hips, WITHOUT the arms (just the trunk).
- arm: ONE arm, the character's arm that appears on the LEFT side of the image, from the shoulder to the end of the hand. The right arm is generated as its mirror image.
- leg: ONE leg, the one on the LEFT side of the image, from the hip to the sole of the foot. The right leg is generated as its mirror image.
- Boxes may overlap a little at the joints (neck, shoulder, hip) so that nothing is left uncovered. Keep them tight around the character, not around empty background.
- Everything of the character (tail, wings, accessories) should fall inside some box (or inside the mirror of the arm or leg box).";

/// Shrinks big images so that the request stays small.
fn small_png(img: &Image, max_side: u32) -> Result<Vec<u8>, String> {
    let (w, h) = (img.width(), img.height());
    if w.max(h) <= max_side {
        return img.png();
    }
    let s = max_side as f32 / w.max(h) as f32;
    let (nw, nh) = (((w as f32 * s) as u32).max(1), ((h as f32 * s) as u32).max(1));
    let mut pm = Pixmap::new(nw, nh).ok_or("cannot resize the image")?;
    pm.draw_pixmap(0, 0, img.pm.as_ref(), &PixmapPaint { quality: FilterQuality::Bilinear, ..Default::default() }, Transform::from_scale(s, s), None);
    pm.encode_png().map_err(|e| e.to_string())
}

fn issues_text(issues: &[Issue]) -> String {
    issues.iter().map(|i| format!("- {}", i.render())).collect::<Vec<_>>().join("\n")
}

/// Asks `provider` (it must be able to see images) for the layout. `start` is the heuristic proposal,
/// given to the model as a starting point.
pub fn propose_with_ai(provider: &dyn Provider, img: &Image, mask: &Mask, start: Option<&Layout>, opts: &DirectorOpts, progress: &mut dyn FnMut(&str)) -> Result<CutProposal, String> {
    if !provider.supports_images() {
        return Err(format!("{} cannot look at images. Use a vision-capable provider (Claude Code CLI, Codex CLI, the Anthropic API...) or the automatic proposal", provider.name()));
    }
    let png = small_png(img, 1024)?;
    let hint = start.map(|l| format!("\n\nA rough automatic guess, which may be wrong, is: {}\nCorrect it where it does not match the picture.", l.to_json())).unwrap_or_default();
    let mut messages = vec![Message::user(format!("Here is the illustration. Give me the four boxes as JSON.{}", hint)).with_images(vec![png])];
    let cx = Some(mask.axis());
    let mut out = CutProposal { layout: start.cloned().ok_or("a starting layout is required")?, issues: vec![], valid: false, rounds: 0, reply: String::new(), usage: (0, 0) };
    for round in 1..=opts.max_rounds.clamp(1, 6) {
        progress(&format!("asking {} to look at the picture (round {})...", provider.name(), round));
        let resp = provider.complete(&ChatRequest { system: SYSTEM.to_string(), messages: messages.clone(), max_tokens: 1500 })?;
        add_usage(&mut out.usage, &resp);
        out.rounds = round;
        out.reply = resp.text.trim().to_string();
        match Layout::from_json(&resp.text) {
            Err(e) => {
                messages.push(Message::assistant(resp.text.clone()));
                messages.push(Message::user(format!("I could not read your answer: {}. Reply with ONLY the JSON object with head, body, arm and leg.", e)));
            }
            Ok(layout) => {
                let issues = validate(&layout, mask);
                let bad = issues.iter().any(|i| i.is_error());
                out.layout = layout.clone();
                out.issues = issues.clone();
                if !bad {
                    out.valid = true;
                    return Ok(out);
                }
                // Show the model what its boxes look like, with numbered markers.
                let over = overlay::draw(img, &layout, cx);
                let over_img = Image { pm: over };
                messages.push(Message::assistant(resp.text.clone()));
                messages.push(
                    Message::user(format!(
                        "The engine found problems with those boxes:\n{}\n\nAttached is the picture with your boxes drawn on it (1 = head, 2 = body, 3 = arm, 4 = leg; the faint boxes are the mirrored right arm and right leg). Fix the problems and reply with ONLY the corrected JSON.",
                        issues_text(&issues)
                    ))
                    .with_images(vec![small_png(&over_img, 1024)?]),
                );
            }
        }
    }
    Ok(out)
}
