# PlainText for VS Code

Syntax highlighting for the [PlainText](../../README.md) programming language (`.pt` files):
keywords (including `class` and `dictionary`), strings with `{interpolation}`, numbers, comments,
function names, types/classes, built-in functions, and the `{pt` file icon in the explorer.

## Install (local)

PlainText isn't on the VS Code Marketplace yet. Install it from this folder:

**Option A — copy into your extensions folder**

Copy this `vscode` folder into your VS Code extensions directory, then reload VS Code:

- Windows: `%USERPROFILE%\.vscode\extensions\plaintext.plaintext-lang-0.1.4`
- macOS / Linux: `~/.vscode/extensions/plaintext.plaintext-lang-0.1.4`

**Option B — package a .vsix** (needs Node.js)

```
npm install -g @vscode/vsce
cd editors/vscode
vsce package --allow-missing-repository
code --install-extension plaintext-lang-0.1.4.vsix
```

Once installed, any `.pt` file is highlighted automatically and shows the PlainText icon.
The language id is `pt` (the id `plaintext` is taken by VS Code's built-in Plain Text).

## Windows desktop / Explorer icon

From the repo root:

```
powershell -ExecutionPolicy Bypass -File scripts\install-pt-icon.ps1
```

That registers `.pt` with `assets/pt.ico` for File Explorer and the desktop.
