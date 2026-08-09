# 9. Modules

Pull another `.pt` file into this one:

```plaintext
import "./toolbox.pt"

print(greeting)
print(double(21))
```

- Paths are relative to the file doing the import.
- Top-level functions and variables from the imported file become available.
- Imported files should not contain a `game` or `window` block.
- The only built-in module today is `math` (`import math`).

## Try it

[`examples/list_tools.pt`](../../examples/list_tools.pt) imports
[`examples/toolbox.pt`](../../examples/toolbox.pt).

**Next:** [Games →](10-games.md)
