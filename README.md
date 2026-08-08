# PlainText

A readability-first programming language for desktop apps and 2D games. Code reads close to
plain English. Files end in `.pt`.

```plaintext
make function called greet(name: Text) {
    print("Hello, {name}!")
}

greet("world")
```

PlainText is statically typed with strong inference (you rarely write a type), garbage
collected, and ships with a standard library plus native APIs for 2D games and desktop UIs —
all backed by a Rust interpreter wrapping [Raylib](https://www.raylib.com/).

## Use PlainText (write programs)

**New here?** Start with [GETTING-STARTED.md](GETTING-STARTED.md).

1. Download the latest **[Release](https://github.com/Granite900/PlainText/releases)**
   (`plaintext-windows-x64.zip`).
2. Unzip and put `plaintext.exe` on your PATH.
3. Run an example:

```
plaintext run examples/basics.pt
plaintext check examples/basics.pt
plaintext new mygame
```

Install `plaintext-lang.vsix` from the zip for VS Code highlighting.

## Learn the language

See [docs/language-reference.md](docs/language-reference.md) for the full guide: syntax, the
type system, the standard library, and the game/UI APIs.

## Examples

| File | What it shows |
|------|---------------|
| `examples/basics.pt`   | variables, functions, classes, loops, interpolation |
| `examples/cart.pt`     | a small program with a `Dynamic` parameter |
| `examples/stdlib.pt`   | math, list/text/dictionary methods, time |
| `examples/timers.pt`   | `after` / `every` timers |
| `examples/bounce.pt`   | a 2D game — a bouncing ball, arrow-key input |
| `examples/sprites.pt`  | loading and drawing image sprites |
| `examples/spawner.pt`  | timers driving a game (spawning enemies) |
| `examples/counter.pt`  | a desktop UI — buttons and a live label |

## Editor support

A VS Code syntax-highlighting extension lives in [editors/vscode/](editors/vscode/).
It also shows the PlainText `{pt` icon on `.pt` files in the explorer.

For the same icon in Windows File Explorer / on the desktop:

```
powershell -ExecutionPolicy Bypass -File scripts\install-pt-icon.ps1
```

## Building from source (contributors)

Needs the [Rust toolchain](https://rustup.rs/). On Windows the graphics dependency also needs
LLVM/`libclang` (for `raylib-sys`'s bindgen) and the MSVC build tools; `.cargo/config.toml`
points at the default LLVM install path.

```
cargo build
cargo install --path .
```

The debug interpreter is at `target/debug/plaintext.exe`. To build a shareable zip:

```
powershell -ExecutionPolicy Bypass -File scripts\package-release.ps1
```

## Status

v1 in progress. Working: the full core language, static type checker, standard library, 2D
game API (shapes, sprites, input, timers), and a declarative UI system. Implemented as a
tree-walking interpreter; a tracing garbage collector and a few polish items are still planned.
