# Getting started with PlainText

This guide is for **writing programs** in PlainText — not for hacking on the
language itself.

## 1. Install

### Option A — download a release (easiest)

1. Open the [Releases](https://github.com/Granite900/PlainText/releases) page.
2. Download the zip for your computer:
   - **Windows:** `plaintext-windows-x64.zip`
   - **Mac (Apple Silicon — M1/M2/M3/M4):** `plaintext-macos-arm64.zip`
   - **Mac (Intel):** build from source for now (see Option B), or use Rosetta with the
     Apple Silicon build if needed.
3. Unzip it somewhere permanent (your home folder is fine).
4. Install it so you can type `plaintext` from anywhere:

   **macOS — one command.** Open **Terminal**, `cd` into the unzipped folder, and run:

   ```bash
   bash scripts/install-macos.sh
   ```

   That does all the fiddly macOS steps for you: it clears Apple's "downloaded from the
   internet" block, makes the binary runnable, and copies `plaintext` into `/usr/local/bin`
   (a folder already on your PATH). It may ask for your login password to copy the file.

   **Windows — one command.** Open **PowerShell**, `cd` into the unzipped folder, and run:

   ```powershell
   powershell -ExecutionPolicy Bypass -File scripts\install-windows.ps1
   ```

   That unblocks the binary (so SmartScreen stops nagging), copies `plaintext.exe` into a
   per-user programs folder, and adds it to your PATH — no admin rights needed. Open a **new**
   terminal afterwards so the PATH change takes effect.

   *(Prefer to do it by hand? Add the folder to your PATH via Start → "edit environment variables
   for your account" → Path → Edit → New, or just run it with a full path like
   `C:\PlainText\plaintext.exe run file.pt`.)*

Check that it works:

```bash
plaintext version
```

You want **2.4.0 or newer**. Older binaries will choke on current examples (`import ai`,
`increase … by`, `plaintext build`, and so on). If something weird happens, see
[docs/troubleshooting.md](docs/troubleshooting.md).

> **macOS, doing it by hand instead?** If you skip the script and macOS blocks the app,
> right-click `plaintext` → **Open** once, or run `xattr -dr com.apple.quarantine plaintext`.

### Option B — build from this repo

Needs the [Rust toolchain](https://rustup.rs/).

```bash
git clone https://github.com/Granite900/PlainText.git
cd PlainText
cargo install --path .
```

**Windows only:** also install LLVM (`libclang`) and the MSVC build tools, then copy
`.cargo/config.toml.example` → `.cargo/config.toml` and set `LIBCLANG_PATH`.

**macOS:** install Xcode Command Line Tools (`xcode-select --install`). That is usually enough.

## 2. Your first program

```bash
plaintext new hello
plaintext run hello/main.pt
```

Or copy an example:

```bash
plaintext run examples/basics.pt
plaintext check examples/basics.pt
plaintext repl
```

## 3. Editor support (VS Code)

The Windows release zip includes `plaintext-lang.vsix`. On any OS you can also build it from
`editors/vscode/` (see that folder's README), or copy the `.vsix` from a Windows zip — it works
on Mac VS Code too.

```bash
code --install-extension plaintext-lang.vsix
```

Then reload the window. `.pt` files get syntax highlighting and the PlainText icon.

## 4. Learn the language

- **[docs/README.md](docs/README.md)** — start here (lesson map, ~60–90 minutes)
- **[docs/learn/](docs/learn/)** — teacher-style lessons (01 → 12) with exercises
- **[docs/cheatsheet.md](docs/cheatsheet.md)** — one-page syntax reminder
- **[docs/language-reference.md](docs/language-reference.md)** — full lookup guide
- **[docs/troubleshooting.md](docs/troubleshooting.md)** — common errors and fixes
- **[examples/](examples/)** — small runnable programs

Useful commands:

| Command | What it does |
|---------|----------------|
| `plaintext run file.pt` | Type-check and run |
| `plaintext check file.pt` | Type-check only |
| `plaintext repl` | Interactive prompt |
| `plaintext new name` | Scaffold a project folder |
| `plaintext version` | Print the version |

### Quick language tips

- Math helpers need `import math` at the top of the file (`sqrt`, `pi`, `random_between`, …).
- Prefer wordy comparisons when it helps reading: `score is at least 90`.
- Use `increase x by n` / `decrease x by n` instead of `x = x + n`.
- Split files with `import "./other.pt"`.

## 5. Windows file icon (optional)

From a release unzip (or a clone), on Windows:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\install-pt-icon.ps1
```

That makes `.pt` files show the PlainText icon in File Explorer. (Mac Finder icons are not
automated yet — use the VS Code extension for the icon in the editor.)
