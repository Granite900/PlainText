# Getting started with PlainText

This guide is for **writing programs** in PlainText — not for hacking on the
language itself.

## 1. Install

### Option A — download a release (easiest)

1. Open the [Releases](https://github.com/Granite900/PlainText/releases) page.
2. Download the zip for your computer:
   - **Windows:** `plaintext-windows-x64.zip`
   - **Mac (Apple Silicon — M1/M2/M3/M4):** `plaintext-macos-arm64.zip`
   - **Mac (Intel):** `plaintext-macos-x64.zip`
   - **Linux x64:** `plaintext-linux-x64.zip`
3. Unzip it somewhere permanent (your home folder is fine).
4. Install it so you can type `plaintext` from anywhere:

   **macOS — one command.** Open **Terminal**, `cd` into the unzipped folder, and run:

   ```bash
   bash scripts/install-macos.sh
   ```

   That does all the fiddly macOS steps for you: it clears Apple's "downloaded from the
   internet" block, makes the binary runnable, and copies `plaintext` into `/usr/local/bin`
   (a folder already on your PATH). It may ask for your login password to copy the file.

   **Linux — one command.** Open a terminal, `cd` into the unzipped folder, and run:

   ```bash
   bash scripts/install-linux.sh
   ```

   That makes the binary executable and copies it to `~/.local/bin`. If that folder is not
   on your PATH yet, the script prints the one line to add to `~/.bashrc`.

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

You want **2.8.0 or newer**. Older binaries will choke on current examples (`import ai`,
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

**Linux (Debian/Ubuntu):** install build tools and Raylib’s system libraries, then build:

```bash
sudo apt-get update
sudo apt-get install -y build-essential cmake pkg-config libclang-dev clang \
  libasound2-dev libx11-dev libxrandr-dev libxi-dev libxcursor-dev libxinerama-dev \
  libgl1-mesa-dev libglu1-mesa-dev libwayland-dev libxkbcommon-dev
cargo install --path .
```

Other distros need the same packages under their own names (clang/libclang, X11, OpenGL, ALSA).

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

You need **two things**: the `plaintext` program (Step 2 above / on your PATH) and
the VS Code extension.

**Install the extension from a release zip** (includes `plaintext-lang.vsix`):

```bash
code --install-extension path/to/plaintext-lang.vsix
```

Then reload VS Code. Open a `.pt` file — you should get colors **and** red squiggles
when something is wrong (same messages as `plaintext check`).

If you only see colors, `plaintext` is not on your PATH. Either fix PATH or set
the VS Code setting **`plaintext.path`** to the full path of `plaintext.exe`
(Windows) / `plaintext` (Mac).

Full walkthrough: [editors/vscode/README.md](editors/vscode/README.md).
You can also build the `.vsix` from that folder if you are developing from a clone.

## 4. Learn the language

- **[docs/README.md](docs/README.md)** — start here (lesson map, ~60–90 minutes)
- **[docs/learn/](docs/learn/)** — teacher-style lessons (01 → 14) with exercises
- **[docs/cheatsheet.md](docs/cheatsheet.md)** — one-page syntax reminder
- **[docs/language-reference.md](docs/language-reference.md)** — full lookup guide
- **[docs/troubleshooting.md](docs/troubleshooting.md)** — common errors and fixes
- **[examples/](examples/)** — runnable programs ([index](examples/README.md))

Useful commands:

| Command | What it does |
|---------|----------------|
| `plaintext run file.pt` | Type-check and run |
| `plaintext check file.pt` | Type-check only |
| `plaintext edit_tilemap file.pt` | Paint a tilemap (rewrites the file) |
| `plaintext lsp` | Language server for editors (stdio) |
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
