# PlainText for VS Code

Syntax highlighting **and a language server** for the [PlainText](../../README.md)
programming language (`.pt` files).

You need **two pieces**. The extension alone gives colors; the `plaintext` program
powers red squiggles, hover, go-to-definition, and completions.

---

## Install in two steps

### Step 1 — Install the `plaintext` program

Pick one:

**From a release (easiest)**

1. Download the latest zip from
   [GitHub Releases](https://github.com/Granite900/PlainText/releases).
2. Unzip it somewhere permanent (for example `C:\PlainText` or `~/plaintext`).
3. Put that folder on your **PATH** so a new terminal can find `plaintext`
   (Windows: Environment Variables → Path → add the folder that contains
   `plaintext.exe`).
4. Open a **new** terminal and check:

```bash
plaintext version
```

You want **2.8.0** or newer.

**From this repo (developers)**

```bash
cargo install --path . --force
plaintext version
```

### Step 2 — Install the VS Code extension

**From a release zip (easiest)**

The Windows release zip includes `plaintext-lang.vsix`. In a terminal:

```bash
code --install-extension path/to/plaintext-lang.vsix
```

Then reload VS Code (**Developer: Reload Window**).

**From this repo**

```bash
cd editors/vscode
npm install
npx @vscode/vsce package --allow-missing-repository
code --install-extension plaintext-lang-2.8.0.vsix
```

Reload VS Code when it asks.

### Step 3 — Check that it works

1. Open any `.pt` file (try `examples/basics.pt` from this repo).
2. Break something on purpose, e.g. change `print` to `prnt`.
3. You should see a **red squiggle** with a hint — the same kind of message as
   `plaintext check`.
4. Fix it → the squiggle should clear.

If you only get colors and **no** squiggles, Step 1 failed (PATH). See
[Troubleshooting](#troubleshooting) below.

---

## What you get

| Feature | Needs `plaintext` on PATH? |
|---------|----------------------------|
| Syntax colors + `.pt` icon | No |
| Red squiggles (diagnostics) | **Yes** |
| Hover / go to definition / completions | **Yes** |

The language id is `pt` (VS Code’s built-in Plain Text already uses `plaintext`).

## Settings (optional)

Open Settings and search for **PlainText**:

| Setting | Default | When to change it |
|---------|---------|-------------------|
| `plaintext.path` | `plaintext` | If `plaintext` is not on PATH — set the **full path** to the executable (e.g. `C:\PlainText\plaintext.exe`) |
| `plaintext.trace.server` | `off` | Turn on only when debugging the language server |

You do **not** need to run `plaintext lsp` yourself. VS Code starts it when you
open a `.pt` file.

## Troubleshooting

**Colors work, but no red squiggles**

1. In a terminal: `plaintext version` — if that fails, fix PATH or set `plaintext.path`.
2. Reload the VS Code window.
3. Open **View → Output**, choose **PlainText Language Server**, and read any error.

**Extension won’t install**

Use the full path to the `.vsix`, and make sure the `code` command works
(`Shell Command: Install 'code' command in PATH` on Mac).

**Wrong / old PlainText**

Examples need **2.8.0+**. Update the binary (Step 1), then reload VS Code.

## Windows desktop / Explorer icon (optional)

From the repo root (or a release unzip):

```
powershell -ExecutionPolicy Bypass -File scripts\install-pt-icon.ps1
```

That registers `.pt` with `assets/pt.ico` for File Explorer and the desktop.
