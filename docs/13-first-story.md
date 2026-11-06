# Your first story, from zero (a test route through every tool)

Do these in order. Each step says what to look at and what "working" means. Everything is done inside Oneiro Studio; the
terminal is never needed. Takes about an hour. Write down anything that fails, that is the useful output of this test.

## 0. Open the program
Double-click **Oneiro Studio** (the desktop shortcut, or `dist\Oneiro\Oneiro Studio.exe`).
You should see the start screen with *New project*, *Open project* and the examples.
- Click an example (`showcase`), press **Play** in the top bar. If you see animation, rendering works.
  Quick check of your machine: Help is not needed, `Oneiro.exe doctor` in the `dist\Oneiro` folder lists GPU, ffmpeg and voices.

## 1. Create the project
**New project...** -> title "Mi Historia", template **Empty**, **Create**.
Working: the studio opens with a bare room in the viewport and "0 errors" at the bottom right.

## 2. Create two characters (Character builder tab)
In the left library press **+ New character** (it opens the **Character builder** tab), type the name in the builder (it must start with a capital letter) and press **+ New character** there.
1. Name it `Luna`. Pick a head (`round`), eyes, a body (`kid`), skin and hair colors, wardrobe (e.g. `red_beanie`).
2. Add a second one called `Tomas` with an `adult` body and a different wardrobe.
3. Press **Save to library** after editing each one. They appear in the library with thumbnails.
Working: the thumbnails change as you edit; both names show under *Characters*.
Try next (optional): set a **Style** for one of them (`rubber_hose`, `clay`, `ink`, `toon_3d`). A style applies per character.

## 3. Build the world
Library -> **+ New set**: name `kitchen`, choose *Diner* (it has a counter, stools and a clock to use as anchors) or *Bare room*.
Open **edit source** to see the set text: anchors (`door`, `counter`...) are the places you can send characters to.
Working: a thumbnail of the set appears.

## 4. Create the episode and write the story
Library -> **+ New episode**: name `episode_1`, pick the set and tick both characters. A script opens in the editor. Replace the
scene with something like (use your own anchor names, shown in the set source):

```
scene 1 "Late night" in kitchen {
  time: night
  place Luna at kitchen.stool_1 facing kitchen.counter
  place Tomas at kitchen.door

  camera establishing wide from south move dolly_in for 4s
  Tomas walks_to kitchen.counter
  Luna (annoyed): "You are late."
  Tomas (nervous): "The bus broke down."

  transition dissolve for 0.8s
  cut to close on Luna
  Luna (happy): "I saved you some pie."
  Luna waves
  wait 1s
}
```
Working: errors appear in red under the editor with a suggestion as you type (try a typo like `walks_too`); fix them and the
status says "valid". The viewport shows the first shot. Press **Play** (or Space) and watch it; Left/Right step one frame.
Things to try: `Luna sits_on kitchen.stool_2`, `Tomas looks_at kitchen.clock and shrugs`, `camera over_shoulder Luna -> Tomas`,
`sfx door_bell at kitchen.door` (needs the sound, see step 6), another `time: dusk`.

## 5. Direct with the mouse (no typing)
- **Timeline** (bottom): click a block to jump to its script line; drag the ruler to scrub.
- **Free camera** (viewport toolbar): orbit with right-drag to look around, then **Shot camera** to return.
- **Inspector** tab: select a character in the viewport to pin a placement or camera; the studio writes it to the `.onlock` file.
- **Record motion**: records a walk by dragging a character, then inserts it as script lines.

## 6. Sound (Audio tab)
1. **Voices:** pick a provider per character (Windows speech works offline) and generate the lines of the episode.
   Working: each dialogue line gets `audio "audio/...wav"`; Play now speaks. ElevenLabs/OpenAI need your own key
   (Environment variables `ELEVENLABS_API_KEY` / `OPENAI_API_KEY`); they were not tested with real keys.
2. **Effects and music:** generate an SFX preset (e.g. `door_bell`) and a music mood (`calm`), then add
   `music calm_bgm volume 0.3 fade_in 1s` at the top of the scene. Music ducks under dialogue by itself.
3. Record your own voice with the microphone in the same tab (the default Windows microphone must be a real one).

## 7. Other ways to make a character
- **Cutter tab:** load a front-view illustration (PNG on a plain or transparent background); the studio proposes boxes for
  head, body, arm and leg; adjust them, check the validation messages, and create the character.
- **AI tab:** needs `claude` or `codex` installed and logged in, or an API key. Ask in plain language:
  "make Tomas look sad when he arrives and add a close-up". The AI edits the script; review the diff and accept or undo.
  Also "create a character: an old fisherman" generates a vector character.

## 8. Render
**File -> Render mp4 -> Full HD** (or Preview for a fast one) and choose where to save. A progress window shows; when it ends the
file is selected in Explorer. Needs ffmpeg on the PATH; without it the studio tells you.

## Common problems
- A character "has not been placed in this scene yet": add `place NAME at SET.anchor` before using it.
- Unknown anchor/prop: the message lists valid names.
- Two things at once: join them with `and` on one line (`Luna waves and looks_at Tomas`) or use `together { ... }`.
- Nothing plays: the script has errors; fix the first red one.
- Where are my files? `Documents\Oneiro\Mi-Historia`: `library\characters`, `library\sets`, `episodes`, `audio`, `out`.
