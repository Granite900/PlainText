<div align="center">

# PlainText

**The programming language that reads like plain English.**

Build desktop apps and 2D games without the cryptic syntax. Statically typed, garbage
collected, batteries included. Files end in `.pt`.

[![Download](https://img.shields.io/github/v/release/Granite900/PlainText?color=2ea44f&label=download)](https://github.com/Granite900/PlainText/releases)
&nbsp;![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20macOS%20%7C%20Linux-blue)
&nbsp;![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange)

</div>

---

```plaintext
make function called greet(name: Text) {
    print("Hello, {name}!")
}

greet("world")
```

PlainText favors real words over symbols (`and`/`or`/`not`, `is not nothing`), infers your
types so you rarely write them, and includes native APIs for 2D games and desktop UIs — all
backed by a Rust interpreter wrapping [Raylib](https://www.raylib.com/).

## Why you might like it

- **Reads like English.** `make function called`, `for every item in items`, `repeat 3 times`.
- **Catches mistakes early.** A real type checker, but inference means you almost never write a type.
- **Games built in.** `game { on update … on draw … }`, drawn natively, with a scrolling **camera**, **sprite-sheet** animation, and particle bursts. `import gamekit` adds gravity, solid bodies, hitboxes, and character-grid **tilemaps** — with a `plaintext edit_tilemap` painter to lay out levels visually. Sound effects and streamed music (volume / pitch / pan / fade) included.
- **Real desktop UIs.** `window { column { button … text_field … } }` with scroll areas, lists, dropdowns, checkboxes, sliders, and multiline text — bound straight to your variables.
- **Batteries included.** Math, lists (map/filter/fold-style tools), dictionaries, text, files, time, timers, console input, and `save` / `load` for structured progress that survives restarts.
- **Train a neural network.** `import ai`, then `neural_network(...)`, `.train(...)`, `.predict(...)` — even watch it learn live in a game window, or train on a GPU (`device: auto`, covering NVIDIA / AMD / Apple).
- **Ship a single app.** `plaintext build game.pt` bundles your program into a standalone executable (Windows, macOS, or Linux).
- **No memory management.** Garbage collected — you never think about it.

## Get started

**New here?** Start with **[GETTING-STARTED.md](GETTING-STARTED.md)**.

1. Download the latest **[release](https://github.com/Granite900/PlainText/releases)** for your OS
   (`plaintext-windows-x64.zip`, `plaintext-macos-arm64.zip`, `plaintext-macos-x64.zip`, or
   `plaintext-linux-x64.zip`).
2. Unzip it and run the setup script for your OS from that folder —
   `bash scripts/install-macos.sh`, `bash scripts/install-linux.sh`, or
   `powershell -ExecutionPolicy Bypass -File scripts\install-windows.ps1` — to put `plaintext` on your PATH.
3. Run something:

```
plaintext run examples/basics.pt     # run a program
plaintext check examples/basics.pt   # check for errors without running
plaintext build examples/basics.pt   # bundle into a standalone app
plaintext edit_tilemap examples/tilemap.pt   # paint a level (rewrites the file)
plaintext repl                       # try expressions interactively
plaintext new mygame                 # scaffold a new project
```

**VS Code:** install the CLI **and** `plaintext-lang.vsix` from the release zip
(two steps — see [editors/vscode/README.md](editors/vscode/README.md)).
The extension needs `plaintext` on your PATH for red squiggles / hover / go-to-def.

## A taste

<table>
<tr><td>

**A desktop UI**

```plaintext
counter = 0

make function called add() {
    increase counter by 1
}

window "Counter" (width: 420, height: 220) {
    column (padding: 24, spacing: 16, align: center) {
        text "Clicked {counter} times" (size: 26)
        button "Click me" (on_click: add)
    }
}
```

</td><td>

**A 2D game**

```plaintext
ball = 300

game "Bounce" (width: 800, height: 600) {
    on update(delta) {
        ball = ball + 150 * delta
    }
    on draw() {
        clear_screen(skyblue)
        draw_circle(400, ball, 24, red)
    }
}
```

</td></tr>
</table>

## Examples

See **[`examples/README.md`](examples/README.md)** for the full index. Highlights:

| File | What it shows |
|------|---------------|
| [`examples/basics.pt`](examples/basics.pt) | Core language (functions, classes, loops, Dynamic) |
| [`examples/stdlib.pt`](examples/stdlib.pt) | `import math`, collection methods |
| [`examples/list_tools.pt`](examples/list_tools.pt) | Multi-file `import` + list tools |
| [`examples/catch.pt`](examples/catch.pt) | Complete arcade game |
| [`examples/platformer.pt`](examples/platformer.pt) | `import gamekit` platformer |
| [`examples/tilemap.pt`](examples/tilemap.pt) | Text-row tilemap levels + menu/play screen |
| [`examples/form.pt`](examples/form.pt) | Desktop form UI |
| [`examples/scroll_list.pt`](examples/scroll_list.pt) | Scroll, list, dropdown, multiline text |
| [`examples/audio.pt`](examples/audio.pt) | Sounds, looping SFX, streamed music, fade |
| [`examples/camera_sheets.pt`](examples/camera_sheets.pt) | Camera follow + sprite-sheet frames + HUD |
| [`examples/save.pt`](examples/save.pt) | `save` / `load` progress across runs |
| [`examples/fetch.pt`](examples/fetch.pt) | `import web` (offline JSON) |
| [`examples/learn.pt`](examples/learn.pt) | `import ai` — train, GPU, save/load |
| [`examples/dataset.pt`](examples/dataset.pt) | Train from CSV + accuracy |
| [`examples/evolve.pt`](examples/evolve.pt) | Neuroevolution in a game window |

## Learn the language

1. **[docs/README.md](docs/README.md)** — start here (lesson map)
2. **[docs/learn/](docs/learn/)** — teacher-style lessons (hello → games, UI, and neural nets)
3. **[docs/cheatsheet.md](docs/cheatsheet.md)** — one-page syntax reminder
4. **[docs/language-reference.md](docs/language-reference.md)** — full reference
5. **[docs/troubleshooting.md](docs/troubleshooting.md)** — common errors (old binary, `import math`, …)

Math helpers require `import math`. Prefer `increase` / `decrease` and word comparisons
(`is at least`, …) when they make the code clearer.

## Editor support

A VS Code extension lives in [editors/vscode/](editors/vscode/): syntax highlighting plus a
language server (`plaintext lsp`) for errors, hover, go to definition, and completions.
The `plaintext` binary must be on your PATH (see [editors/vscode/README.md](editors/vscode/README.md)).

For the `.pt` icon in Windows File Explorer / on the desktop:

```
powershell -ExecutionPolicy Bypass -File scripts\install-pt-icon.ps1
```

## Build from source

Needs the [Rust toolchain](https://rustup.rs/).

- **Windows:** LLVM/`libclang` + MSVC build tools. Copy `.cargo/config.toml.example` to
  `.cargo/config.toml` and set `LIBCLANG_PATH`.
- **macOS:** Xcode Command Line Tools (`xcode-select --install`).
- **Linux (Debian/Ubuntu):** install Raylib/bindgen deps — see the `apt-get` list in
  [GETTING-STARTED.md](GETTING-STARTED.md).

```
cargo build            # interpreter at target/debug/plaintext
cargo install --path . # install `plaintext` onto your PATH
```

Build a distributable Windows zip with `scripts\package-release.ps1`. Tagged releases
(`v2.10.1`, …) are built for **Windows + macOS (arm64 & Intel) + Linux** by GitHub Actions
([`.github/workflows/release.yml`](.github/workflows/release.yml)).

## What's new

**2.10.1** — tilemap editor pan (MMB / Space-drag / arrows) and wheel zoom.

**2.10** — scrolling worlds and animation:

- **Camera** — `set_camera(x, y)` / `center_camera(x, y)` scroll the world; every ordinary draw
  subtracts the offset. `camera_bounds(0, 0, w, h)` keeps the view inside the level, and
  `camera_x()` / `camera_y()` read it back.
- **Sprite sheets** — `load_sprite_sheet(path, cell_width:, cell_height:)`, then
  `draw_frame(sheet, frame, x, y)` (with `flip_x:` or `draw_frame_scaled`) and `frame_count`
  for walk cycles and animations.
- **HUD drawing** — `draw_text_screen` / `draw_rectangle_screen` stay fixed on screen,
  unaffected by the camera.
- **Particles** — `burst(x, y, color, count)` (optional `speed:` / `life:`) fires self-managing
  sparks — no bookkeeping.
- **Smarter `build`** — every literal `load_sprite` / `load_sprite_sheet` / `load_sound` /
  `load_music` / `load_font` path is packed next to the app (keeping its relative path), so a
  built game runs anywhere.

**2.9.1** — faster variable lookup/assignment in tight loops (`FxHashMap` scopes; assign without
re-allocating the key string).

**2.9** — a bigger toolbox for apps and games:

- **UI depth** — `scroll` areas, `list` and `dropdown` pickers, and multi-line `text_field`,
  with Tab / Shift+Tab focus movement.
- **Audio depth** — looping sound effects, streamed background **music**, and
  volume / pitch / pan / fade (`load_music`, `play_music`, `fade_music`, `set_sound_*`, …).
- **Tilemaps** — character-grid levels (`tilemap`, `tile_at`, `draw_tilemap`) that solid bodies
  collide with, plus **`plaintext edit_tilemap`** — a paint window that lays out a level and
  rewrites your `.pt` file.
- **Save system** — `save` / `load` / `has_save` persist any value (including your own class
  instances) to disk, written atomically.

**Earlier in the 2.x line:**

- **2.8** — Linux x64 and Intel-Mac release builds; `plaintext lsp` (diagnostics, hover,
  go-to-definition, completions) and the VS Code extension that spawns it.
- **2.4** — inline/anonymous functions (`make function (…) { … }`) and the first form UI
  (`text_field` / `checkbox` / `slider` / `image` with `bind:` / `on_change`).
- **2.x modules** — `import gamekit` (gravity, bodies, hitboxes), `import web`
  (`web.get` / `get_json` / `post_json`, `to_json` / `parse_json`), neuroevolution and CSV
  dataset loading for `import ai`.
- **2.0** — the neural-network / GPU / `plaintext build` baseline.
