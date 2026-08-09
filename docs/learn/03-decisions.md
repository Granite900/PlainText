# 3. Decisions

**Goal:** Branch with `if` / `else`, and write comparisons that read like English.

## `if` / `else if` / `else`

```plaintext
score = 88

if score is at least 90 {
    print("A")
} else if score is at least 80 {
    print("B")
} else {
    print("C")
}
```

The condition must be a real `Boolean`. PlainText will not treat a number or string as
“truthy” — write a comparison.

## Word comparisons (and symbols)

Both styles mean the same thing. Prefer words when they make the line clearer.

| Words | Symbols |
|-------|---------|
| `is` / `is not` | `==` / `!=` |
| `is at least` / `is at most` | `>=` / `<=` |
| `is more than` / `is less than` | `>` / `<` |

Combine with `and`, `or`, and `not`:

```plaintext
if ready and lives is more than 0 {
    print("go")
}
```

## Values that might be missing

A type with `?` is optional — it can hold a value **or** `nothing`:

```plaintext
nickname: Text? = nothing

if nickname is not nothing {
    print("Also known as {nickname}")
}
```

Always check before you use an optional as if it were present.

## Practice

In [`examples/basics.pt`](../../examples/basics.pt), read `classify_score` and the `Contact`
example. Change a score and re-run to see the letter grade change.

## Common mistakes

| Mistake | Fix |
|---------|-----|
| `if score { … }` | Need `if score is more than 0 { … }` |
| Using an optional without checking | Test `is not nothing` first |

**Previous:** [Variables ←](02-variables-and-text.md) · **Next:** [Loops →](04-loops.md)
