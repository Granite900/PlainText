# Troubleshooting

Quick fixes for the problems people hit first.

## `expected a value, found Import` (or similar nonsense errors)

Your `plaintext` binary is **older than the examples**.

```bash
plaintext version
```

Examples in this repo need **2.8.0 or newer**. Fix by either:

- Downloading the [latest release](https://github.com/Granite900/PlainText/releases), **or**
- From a clone: `cargo install --path . --force`

Then open a **new** terminal and check `plaintext version` again.

## `expected end of line, found counter` near `increase counter by`

Same root cause: an old binary that does not know `increase … by` / `decrease … by`.
Update PlainText (see above).

## `` `sqrt` needs the math module `` (or `pi`, `random_between`, …)

Math helpers live in a module. Put this at the **top** of the file:

```plaintext
import math
```

Likewise, `neural_network`, `population`, and friends need `import ai`.

## VS Code shows colors but no red squiggles

You installed the extension, but VS Code can’t start `plaintext lsp`.

1. In a **new** terminal run: `plaintext version` (need **2.8.0+**).
2. If that fails, put the folder that contains `plaintext` on your PATH, **or** in
   VS Code settings set **`plaintext.path`** to the full path
   (example: `C:\PlainText\plaintext.exe`).
3. Reload the window (**Developer: Reload Window**).
4. Still stuck? **View → Output → PlainText Language Server** for the error.

Full install steps: [editors/vscode/README.md](../editors/vscode/README.md).

## `can't add a Text and a Number`

Do not glue strings with `+` and a number. Use interpolation:

```plaintext
print("score: {score}")     // good
print("score: " + score)    // error
```

## A loop never ends / a variable changes for no reason

A same-named variable inside a function reassigns the outer one instead of getting its own copy
if that name already exists where the function was *defined* (assignment reuses the nearest
existing binding up the scope chain; see [§5, Loops](language-reference.md#5-loops)). Rename
one of them — most often a loop counter like `i` reused inside a helper function.

## Empty list errors

An empty list has no element type to infer. Write one:

```plaintext
names: Text list = []
```

## Game or window never opens / flashes and exits

- Run from the project folder so relative paths like `examples/assets/...` resolve.
- Use `plaintext check file.pt` first — a type error aborts before the window appears.
- On macOS, if the OS blocks the binary: right-click → **Open**, or
  `xattr -dr com.apple.quarantine plaintext`.

## Windows: “Application Control / Device Guard blocked this file”

Some machines block freshly built `--release` binaries. Try:

- The zip from a [GitHub release](https://github.com/Granite900/PlainText/releases), or
- A debug build: `cargo build` then run `.\target\debug\plaintext.exe`

## Still stuck?

1. `plaintext check yourfile.pt` and read the hint under the error.
2. Compare with a nearby file in [`examples/`](../examples/)
   ([index](../examples/README.md)).
3. Skim the [cheatsheet](../CHEATSHEET.md) and the matching [lesson](README.md).
