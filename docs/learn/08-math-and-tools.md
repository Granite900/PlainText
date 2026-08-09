# 8. Math and the toolbox

**Goal:** Use the math module, ask the user questions, and stop a program cleanly.

## Math needs an import

Helpers like `sqrt`, `pow`, `clamp`, `random_between`, and constants `pi` / `e` live in the
**math** module. Without the import they simply do not exist:

```plaintext
import math

print(sqrt(144))
print(greatest(3, 9, 5))
print(pi)
print(random_between(1, 6))    // useful in games
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
| [`examples/list_tools.pt`](../../examples/list_tools.pt) | List tools + `exit()` |

## Common mistakes

| Mistake | Fix |
|---------|-----|
| `Unknown name: sqrt` | Add `import math` |
| Treating `input(...)` as a number | Wrap with `to_number(...)` |

**Previous:** [Collections ←](07-collections.md) · **Next:** [Modules →](09-modules.md)
