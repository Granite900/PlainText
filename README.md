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
- **Games & UIs built in.** `game { on update … on draw … }` and `window { column { button … text_field … } }`, drawn natively. `import gamekit` adds gravity, solid bodies, and hitboxes.
- **Batteries included.** Math, lists (map/filter/fold-style tools), dictionaries, text, files, time, timers, and console input out of the box.
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
| [`examples/form.pt`](examples/form.pt) | Desktop form UI |
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
(`v2.8.0`, …) are built for **Windows + macOS (arm64 & Intel) + Linux** by GitHub Actions
([`.github/workflows/release.yml`](.github/workflows/release.yml)).

## How it works

PlainText is a tree-walking interpreter written in Rust: **lexer → parser → type checker →
interpreter**, with a standard library and Raylib-backed game/UI APIs. Memory is reference
counted with a mark-and-sweep collector on top to reclaim reference cycles, so you never manage
memory yourself. See [docs/language-reference.md](docs/language-reference.md) for the language.

Being a tree-walker, it favors clarity over raw speed — great for games, UIs, and learning, but
slower than an optimized bytecode VM on tight numeric loops (see [benchmarks/](benchmarks/)). The
one place that matters most, neural-network training, drops to compiled Rust and an optional GPU.

## Status

The full language, type checker, standard library, 2D game API (shapes, sprites, input, timers),
declarative UI system, and a cycle-collecting garbage collector all work today.
