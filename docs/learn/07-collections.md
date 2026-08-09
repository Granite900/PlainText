# 7. Lists and dictionaries

## Lists

```plaintext
scores = [90, 82, 74]
scores.append(88)
print(scores.length())
print(scores[0])
```

Useful methods: `append`, `pop`, `contains`, `first`, `last`, `reversed`, `join`,
`sorted`, `transformed_by`, `kept_if`, `combined`.

Empty lists need a type: `names: Text list = []`.

## Dictionaries

```plaintext
ages = dictionary { "Ada": 36, "Alan": 41 }
print(ages["Ada"])
ages["Grace"] = 45
```

Methods: `has`, `get`, `keys`, `values`, `remove`, …

## Try it

[`examples/stdlib.pt`](../../examples/stdlib.pt) and
[`examples/list_tools.pt`](../../examples/list_tools.pt).

**Next:** [Math and the toolbox →](08-math-and-tools.md)
