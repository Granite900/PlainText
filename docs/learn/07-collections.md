# 7. Lists and dictionaries

**Goal:** Store ordered collections and key/value maps, and use their methods.

## Lists

```plaintext
scores = [90, 82, 74]
scores.append(88)           // also: .add(…)
print(scores.length())
print(scores[0])            // first item
print(scores.contains(82))
```

Useful methods:

| Method | Idea |
|--------|------|
| `append` / `add`, `pop` | Grow / shrink |
| `first`, `last`, `get`, `index_of` | Look up |
| `reversed`, `join`, `sorted` | New list / text |
| `transformed_by(fn)` | Map each item through a function |
| `kept_if(fn)` | Keep items where `fn` returns true |
| `combined(start, fn)` | Fold / reduce into one value |

Empty lists need a type, because there is nothing to infer from:

```plaintext
names: Text list = []
```

## Dictionaries

```plaintext
ages = dictionary { "Ada": 36, "Alan": 41 }
print(ages["Ada"])
ages["Grace"] = 45
print(ages.has("Ada"))
print(ages.keys())
```

## Practice

1. [`examples/stdlib.pt`](../../examples/stdlib.pt) — list, text, and dictionary methods.
2. [`examples/list_tools.pt`](../../examples/list_tools.pt) — `sorted` / `transformed_by` /
   `kept_if` / `combined` (after you learn imports in the next lessons).

## Common mistakes

| Mistake | Fix |
|---------|-----|
| `[]` with no annotation | `items: Number list = []` |
| Expecting `sorted()` to change the original | It returns a **new** list |

**Previous:** [Classes ←](06-classes.md) · **Next:** [Math and the toolbox →](08-math-and-tools.md)
