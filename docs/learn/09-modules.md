# 9. Modules

**Goal:** Split a program across files without copy-paste.

## Import another file

```plaintext
import "./toolbox.pt"

print(greeting)
print(double(21))
```

What happens:

- The path is **relative to the file doing the import**.
- Top-level functions and variables from the imported file become available here.
- Imported files must **not** contain a `game` or `window` block (those belong in the entry file).

## Built-in modules

Today there is one: **math**.

```plaintext
import math
```

File imports use a quoted path; built-in modules use a bare name.

## Practice

Read [`examples/toolbox.pt`](../../examples/toolbox.pt), then run
[`examples/list_tools.pt`](../../examples/list_tools.pt):

```bash
plaintext run examples/list_tools.pt
```

Try adding another helper function to `toolbox.pt` and calling it from `list_tools.pt`.

## Common mistakes

| Mistake | Fix |
|---------|-----|
| Wrong relative path | Path is from the **importer’s** folder |
| `game` / `window` inside an imported file | Keep those in the main entry file only |

**Previous:** [Math ←](08-math-and-tools.md) · **Next:** [Games →](10-games.md)
