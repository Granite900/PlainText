# 8. Math and the toolbox

**Goal:** Use the math module, ask the user questions, remember progress across runs, and stop a
program cleanly.

## Math needs an import

Helpers like `sqrt`, `pow`, `clamp`, `random_between`, and constants `pi` / `e` live in the
**math** module. Without the import they simply do not exist:

```plaintext
import math

print(sqrt(144))
print(greatest(3, 9, 5))
print(pi)
print(random_between(1, 6))    // useful in games
print(round(3.14159, 2))       // 3.14 — round(x, places) tidies output
```

Put `import math` near the top of any file that needs them (including games).

## Console input

```plaintext
name = input("What is your name? ")
print("Nice to meet you, {name}!")

age = to_number(input("How old are you? "))
print("Next year: {age + 1}")
```

`input` always returns `Text`. Convert with `to_number` when you need a `Number`.

## When something can fail

`to_number` stops the whole program if the text isn't a number. Wrap it in **`try`** to get
`nothing` back instead, then supply a default with **`otherwise`** — or check it yourself:

```plaintext
age = try to_number(input("How old are you? ")) otherwise 0

// or handle the bad case explicitly:
maybe = try to_number(answer)
if maybe is nothing {
    print("That wasn't a number.")
}
```

`try` rescues other failures too — a missing file (`read_file`), an out-of-range index — turning
them into `nothing` rather than a crash. It never swallows an `exit()`. See
[`examples/ask.pt`](../../examples/ask.pt).

## Saving progress

Games and tools often need to remember something after you quit — a high score, settings, how
many times you've played. Use **`save` / `load` / `has_save`** (no import):

```plaintext
class Progress {
    best = 0
    runs = 0
}

// First run: no file yet → load gives nothing → otherwise starts fresh.
progress = load("game.save") otherwise Progress { }

increase progress.runs by 1
if score is more than progress.best {
    progress.best = score
}

save(progress, "game.save")     // atomic write — a crash mid-save won't corrupt the file
```

- `load(path)` returns the value, or `nothing` if the file is missing.
- `has_save(path)` is `true` / `false` if you only need to know whether a file exists.
- You can save numbers, text, booleans, lists, dictionaries, and **your own class values**.
  A saved `Progress` comes back as a real `Progress` (fields and methods), not a plain
  dictionary. Functions and neural networks cannot be saved this way (networks use
  `brain.save` / `load_network` in [lesson 13](13-neural-networks.md)).

Paths are relative to the folder you run `plaintext` from. Run
[`examples/save.pt`](../../examples/save.pt) a few times — the best score sticks around.

## Stopping with `exit`

```plaintext
exit()       // status 0
exit(1)      // non-zero = “something went wrong” for shells
```

## Practice

| File | What it shows |
|------|----------------|
| [`examples/stdlib.pt`](../../examples/stdlib.pt) | Math + collection methods |
| [`examples/ask.pt`](../../examples/ask.pt) | `input` conversation |
| [`examples/save.pt`](../../examples/save.pt) | `save` / `load` a high score across runs |
| [`examples/list_tools.pt`](../../examples/list_tools.pt) | List tools + `exit()` |

## Common mistakes

| Mistake | Fix |
|---------|-----|
| `` `sqrt` needs the math module `` | Add `import math` |
| Treating `input(...)` as a number | Wrap with `to_number(...)` |
| Assuming `load` crashes when missing | It returns `nothing` — use `otherwise` for a default |
| Expecting a save next to the `.pt` file | Paths are from where you *run* `plaintext`, not the file's folder |

**Previous:** [Collections ←](07-collections.md) · **Next:** [Modules →](09-modules.md)
