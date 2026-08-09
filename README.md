<div align="center">

# PlainText

**The programming language that reads like plain English.**

Build desktop apps and 2D games without the cryptic syntax. Statically typed, garbage
collected, batteries included. Files end in `.pt`.

[![Download](https://img.shields.io/github/v/release/Granite900/PlainText?color=2ea44f&label=download)](https://github.com/Granite900/PlainText/releases)
&nbsp;![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20macOS-blue)
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
- **Games & UIs built in.** `game { on update … on draw … }` and `window { column { button … } }`, drawn natively.
- **Batteries included.** Math, lists (map/filter/fold-style tools), dictionaries, text, files, time, timers, and console input out of the box.
- **No memory management.** Garbage collected — you never think about it.

## Get started

**New here?** Start with **[GETTING-STARTED.md](GETTING-STARTED.md)**.

1. Download the latest **[release](https://github.com/Granite900/PlainText/releases)** for your OS
   (`plaintext-windows-x64.zip` or `plaintext-macos-arm64.zip`).
2. Unzip it and put `plaintext` / `plaintext.exe` on your PATH.
3. Run something:

```
plaintext run examples/basics.pt     # run a program
plaintext check examples/basics.pt   # check for errors without running
plaintext repl                       # try expressions interactively
plaintext new mygame                 # scaffold a new project
```

Install `plaintext-lang.vsix` from the zip for VS Code syntax highlighting (same file on Windows
and Mac).

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

| File | What it shows |
|------|---------------|
| [`examples/basics.pt`](examples/basics.pt)   | variables, functions, classes, loops, wordy compares |
| [`examples/ask.pt`](examples/ask.pt)         | `input()` from the console |
| [`examples/cart.pt`](examples/cart.pt)       | a small program with a flexible (`Dynamic`) parameter |
| [`examples/stdlib.pt`](examples/stdlib.pt)   | `import math`, list / text / dictionary methods, time |
| [`examples/list_tools.pt`](examples/list_tools.pt) | multi-file `import`, `sorted` / `transformed_by` / `kept_if` / `combined`, `exit` |
| [`examples/toolbox.pt`](examples/toolbox.pt) | a module imported by `list_tools.pt` |
| [`examples/timers.pt`](examples/timers.pt)   | `after` / `every` timers |
| [`examples/bounce.pt`](examples/bounce.pt)   | a 2D game — a bouncing ball with arrow-key input |
| [`examples/sprites.pt`](examples/sprites.pt) | loading and drawing image sprites |
| [`examples/spawner.pt`](examples/spawner.pt) | timers driving a game (spawning objects) |
| [`examples/catch.pt`](examples/catch.pt)     | a **complete** arcade game — score, lives, game over, restart |
| [`examples/counter.pt`](examples/counter.pt) | a **complete** desktop UI — buttons and a live label |

## Learn the language

1. **[docs/README.md](docs/README.md)** — start here (lesson map)
2. **[docs/learn/](docs/learn/)** — teacher-style lessons (hello → games & UI)
3. **[docs/cheatsheet.md](docs/cheatsheet.md)** — one-page syntax reminder
4. **[docs/language-reference.md](docs/language-reference.md)** — full reference
5. **[docs/troubleshooting.md](docs/troubleshooting.md)** — common errors (old binary, `import math`, …)

Math helpers require `import math`. Prefer `increase` / `decrease` and word comparisons
(`is at least`, …) when they make the code clearer.

## Editor support

A VS Code syntax-highlighting extension lives in [editors/vscode/](editors/vscode/), and shows the
PlainText icon on `.pt` files. For that icon in Windows File Explorer / on the desktop:

```
powershell -ExecutionPolicy Bypass -File scripts\install-pt-icon.ps1
```

## Build from source

Needs the [Rust toolchain](https://rustup.rs/).

- **Windows:** LLVM/`libclang` + MSVC build tools. Copy `.cargo/config.toml.example` to
  `.cargo/config.toml` and set `LIBCLANG_PATH`.
- **macOS:** Xcode Command Line Tools (`xcode-select --install`).

```
cargo build            # interpreter at target/debug/plaintext
cargo install --path . # install `plaintext` onto your PATH
```

Build a distributable Windows zip with `scripts\package-release.ps1`. Tagged releases
(`v0.1.2`, …) are built for **Windows + macOS** by GitHub Actions
([`.github/workflows/release.yml`](.github/workflows/release.yml)).

## How it works

PlainText is a tree-walking interpreter written in Rust: **lexer → parser → type checker →
interpreter**, with a standard library and Raylib-backed game/UI APIs. Memory is reference
counted with a mark-and-sweep collector on top to reclaim reference cycles, so you never manage
memory yourself. See [docs/language-reference.md](docs/language-reference.md) for the language.

## Status

The full language, type checker, standard library, 2D game API (shapes, sprites, input, timers),
declarative UI system, and a cycle-collecting garbage collector all work today.
