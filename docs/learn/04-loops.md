# 4. Loops

**Goal:** Repeat work with `for every`, `repeat`, `while`, and `loop`.

## Four shapes

```plaintext
scores = [95, 82, 74]

for every score in scores {
    print(score)
}

repeat 3 times {
    print("go")
}

count = 3
while count is more than 0 {
    print(count)
    decrease count by 1
}

loop {
    // runs forever until you break
    break
}
```

- `for every` walks a list (also works on text and dictionaries).
- `repeat N times` is for a fixed count.
- `while` keeps going while a condition stays true.
- `loop` is an infinite loop — use `break` to leave.

Use `continue` to skip the rest of the current round and start the next one.

## Scope surprise

Loops do **not** create a new scope. Only **functions** do. A name you create inside a loop
is still visible afterward:

```plaintext
for every n in [1, 2, 3] {
    last = n
}
print(last)    // 3 — still in scope
```

## Practice

Run the loop section of [`examples/basics.pt`](../../examples/basics.pt) (`for every`,
`repeat`, countdown `while`).

## Common mistakes

| Mistake | Fix |
|---------|-----|
| Infinite `while` / `loop` with no `break` | Add a condition or `break` |
| Expecting loop-local variables to disappear | They don’t — use a function if you need a private scope |

**Previous:** [Decisions ←](03-decisions.md) · **Next:** [Functions →](05-functions.md)
