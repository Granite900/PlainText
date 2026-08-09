# 3. Decisions

## `if` / `else if` / `else`

```plaintext
if score is at least 90 {
    print("A")
} else if score is at least 80 {
    print("B")
} else {
    print("C")
}
```

Conditions must be a real `Boolean` — write a comparison, not a “truthy” value.

## Word comparisons (and symbols)

| Words | Symbols |
|-------|---------|
| `is` / `is not` | `==` / `!=` |
| `is at least` / `is at most` | `>=` / `<=` |
| `is more than` / `is less than` | `>` / `<` |

Combine with `and`, `or`, `not`.

## Optionals

A value that might be missing is `Text?` (or any type with `?`). Test with
`is nothing` / `is not nothing` before using it.

## Try it

See `classify_score` and the `Contact` example in [`examples/basics.pt`](../../examples/basics.pt).

**Next:** [Loops →](04-loops.md)
