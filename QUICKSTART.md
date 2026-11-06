# Quickstart

## The easy way (Windows, no commands)
1. Install [ffmpeg](https://ffmpeg.org) and put it on your PATH (only needed to export mp4).
2. Build the program once. In PowerShell, from the project folder:
   `powershell -ExecutionPolicy Bypass -File tools\package.ps1 -Shortcuts`
   This builds it, creates `dist\Oneiro\` and puts **Oneiro Studio** on your Desktop and Start Menu.
3. From now on just double-click **Oneiro Studio**. Pick *New project*, *Open project* or an example.
4. Follow [docs/13-first-story.md](docs/13-first-story.md) to make a story from scratch and try every tool.

You can copy the `dist\Oneiro` folder anywhere (it is self-contained: program, command-line tool, examples, docs).

## The developer way
`cargo build --release`, then `.\target\release\oneiro.exe doctor`, `render examples\showcase -o out\showcase.mp4`,
`new my-show [--template empty]`, and `cargo run --release -p oneiro-ide -- examples\showcase`.

Other examples: `examples/first_short` (the diner short with voices), `examples/style_gallery` (mixed styles),
`examples/toon3d_demo` (3D characters with capes and tails), `examples/models_demo` (imported .obj props),
`examples/showcase` (nearly everything at once). Language reference: `docs/01-language-spec.md`.
