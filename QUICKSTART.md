# Quickstart

1. Install [Rust](https://rustup.rs) (stable) and, for mp4 export, [ffmpeg](https://ffmpeg.org) on your PATH.
2. Build once: `cargo build --release` (the first build takes a few minutes).
3. Check your machine: `./target/release/oneiro doctor`.
4. Watch what it can do: `./target/release/oneiro render examples/showcase -o out/showcase.mp4`
   (then open `out/showcase.mp4`). Add `--preview` for a quick 640x360 version.
5. Open the studio (editor + live viewport + timeline + AI tab): `cargo run --release -p oneiro-ide -- examples/showcase`
6. Make your own: `./target/release/oneiro new my-show`, edit `my-show/script.onr`, then
   `oneiro check my-show` after each change and `oneiro render my-show -o out/my-show.mp4`.

Other examples: `examples/first_short` (the diner short with voices), `examples/style_gallery` (mixed styles),
`examples/toon3d_demo` (3D characters with capes and tails), `examples/models_demo` (imported .obj props).
Voices: `oneiro voice my-show --provider sapi` (Windows, offline). Language reference: `docs/01-language-spec.md`.
