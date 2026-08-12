# PlainText documentation

Welcome. This folder has two jobs:

1. **Teach you** the language, step by step.
2. **Help you look things up** when you already know what you want.

You only need the `plaintext` command installed. Check with:

```bash
plaintext version
```

If that prints an old number (or fails), update from the
[latest release](https://github.com/Granite900/PlainText/releases) or rebuild from this repo —
then come back here. See [Troubleshooting](troubleshooting.md) if something feels off.

---

## Path A — Learn by doing (recommended)

Fourteen short lessons. Read one, run the linked example, then move on.
About **70–100 minutes** if you try every exercise.

| # | Lesson | What you practice |
|---|--------|-------------------|
| 1 | [Hello, PlainText](learn/01-hello.md) | Running `.pt` files, `print`, comments |
| 2 | [Variables and text](learn/02-variables-and-text.md) | Names, `increase` / `decrease`, `{interpolation}` |
| 3 | [Decisions](learn/03-decisions.md) | `if` / `else`, wordy comparisons, optionals |
| 4 | [Loops](learn/04-loops.md) | `for every`, `repeat`, `while`, `loop` |
| 5 | [Functions](learn/05-functions.md) | `make function called`, parameters, `return` |
| 6 | [Classes](learn/06-classes.md) | `class`, fields, methods, `self` |
| 7 | [Lists and dictionaries](learn/07-collections.md) | Collections and their methods |
| 8 | [Math and the toolbox](learn/08-math-and-tools.md) | `import math`, `input`, `save` / `load`, `exit` |
| 9 | [Modules](learn/09-modules.md) | Splitting a program across files |
| 10 | [Games](learn/10-games.md) | `game`, update/draw, sprites, input, sound & music |
| 11 | [Desktop UI](learn/11-ui.md) | `window`, buttons, forms, scroll / list / dropdown |
| 12 | [Game kit](learn/12-game-kit.md) | `import gamekit` — gravity, bodies, hitboxes, tilemaps |
| 13 | [Neural networks](learn/13-neural-networks.md) | `import ai`, train / predict, CSV & image datasets, GPU, neuroevolution |
| 14 | [Talking to the web](learn/14-web.md) | `import web` — HTTP, JSON, offline fixtures |

Every example under [`examples/`](../examples/) is listed in
[`examples/README.md`](../examples/README.md) — steal from them freely.

---

## Path B — Look something up

| Doc | Use it when… |
|-----|----------------|
| [**Examples index**](../examples/README.md) | Every runnable sample, grouped by topic |
| [**Cheatsheet**](../CHEATSHEET.md) | You need a dense, searchable syntax reference |
| [**Language reference**](language-reference.md) | You want the full story (types, stdlib, game/UI APIs) |
| [**Troubleshooting**](troubleshooting.md) | Errors, old binaries, or “why doesn’t this run?” |
| [Getting started](../GETTING-STARTED.md) | Install / PATH / VS Code setup |
| [Project README](../README.md) | Downloads and project overview |

---

## Commands you will use constantly

```bash
plaintext run examples/basics.pt      # type-check + run
plaintext check examples/basics.pt    # type-check only
plaintext repl                        # try ideas interactively
plaintext new myproject               # scaffold a folder
plaintext version                     # confirm you have a current build
```

**Tip:** Prefer `plaintext check` while you edit — it catches type mistakes without opening a game or UI window.
