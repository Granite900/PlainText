# Troubleshooting

Quick fixes for the problems people hit first.

## `expected a value, found Import` (or similar nonsense errors)

Your `plaintext` binary is **older than the examples**.

```bash
plaintext version
```

Examples in this repo need **2.0.0 or newer**. Fix by either:

- Downloading the [latest release](https://github.com/Granite900/PlainText/releases), **or**
- From a clone: `cargo install --path . --force`

Then open a **new** terminal and check `plaintext version` again.

## `expected end of line, found counter` near `increase counter by`

Same root cause: an old binary that does not know `increase … by` / `decrease … by`.
Update PlainText (see above).

## `Unknown name: sqrt` / `pi` / `random_between`

Math helpers live in a module. Put this at the **top** of the file:

```plaintext
import math
```

## `Cannot add Text and Number`

Do not glue strings with `+` and a number. Use interpolation:

```plaintext
print("score: {score}")     // good
print("score: " + score)    // error
```

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
2. Compare with a nearby file in [`examples/`](../examples/).
3. Skim the [cheatsheet](cheatsheet.md) and the matching [lesson](README.md).
