# Getting started with PlainText

This guide is for **writing programs** in PlainText — not for hacking on the
language itself.

## 1. Install

### Option A — download a release (easiest)

1. Open the [Releases](https://github.com/Granite900/PlainText/releases) page.
2. Download `plaintext-windows-x64.zip` from the latest release.
3. Unzip it somewhere permanent, e.g. `C:\Tools\PlainText`.
4. Add that folder to your PATH, **or** run `plaintext.exe` with a full path.

Check that it works:

```
plaintext version
```

### Option B — build from this repo

Needs the [Rust toolchain](https://rustup.rs/). On Windows you also need LLVM
(`libclang`) and the MSVC build tools (see `.cargo/config.toml`).

```
git clone https://github.com/Granite900/PlainText.git
cd PlainText
cargo install --path .
```

## 2. Your first program

```
plaintext new hello
plaintext run hello/main.pt
```

Or copy an example:

```
plaintext run examples/basics.pt
plaintext check examples/basics.pt
```

## 3. Editor support (VS Code)

The release zip includes `plaintext-lang.vsix`. In VS Code:

```
code --install-extension plaintext-lang.vsix
```

Then reload the window. `.pt` files get syntax highlighting and the PlainText icon.

(If you cloned the repo instead, package it from `editors/vscode/` — see that folder's README.)

## 4. Learn the language

- [Language reference](docs/language-reference.md) — full guide
- [examples/](examples/) — small programs for the core language, games, and UI

Useful commands:

| Command | What it does |
|---------|----------------|
| `plaintext run file.pt` | Type-check and run |
| `plaintext check file.pt` | Type-check only |
| `plaintext new name` | Scaffold a project folder |
| `plaintext version` | Print the version |

## 5. Windows file icon (optional)

From a release unzip (or a clone):

```
powershell -ExecutionPolicy Bypass -File scripts\install-pt-icon.ps1
```

That makes `.pt` files show the PlainText icon in File Explorer.
