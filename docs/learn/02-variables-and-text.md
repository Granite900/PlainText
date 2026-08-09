# 2. Variables and text

**Goal:** Store values in names, update them in plain English, and build sentences with
interpolation.

## Creating names

```plaintext
age = 5
age = age + 1        // now 6
```

The first time you write a name, PlainText creates it. Later, the same `=` reassigns it.
You usually do **not** write a type — PlainText infers it:

- `5` → `Number`
- `"hi"` → `Text`
- `true` / `false` → `Boolean`

You *may* annotate when you want clarity: `age: Number = 5`.

## `increase` and `decrease`

Adding or subtracting from a name is so common it has a word form:

```plaintext
score = 0
increase score by 10     // same as score = score + 10
decrease lives by 1      // same as lives = lives - 1
```

Prefer these when the line should read like an instruction.

## Building text with `{…}`

Put an expression in curly braces inside a string and PlainText inserts it:

```plaintext
name = "Ada"
age = 36
print("Hello, {name}! Next year you will be {age + 1}.")
```

Output:

```
Hello, Ada! Next year you will be 37.
```

### Important: do not mix `+` across types

```plaintext
print("score: " + score)    // error — Text + Number
print("score: {score}")     // good
```

Use `to_text(x)` only when you truly need a `Text` value, not just a printout.

## Practice

Run the start of [`examples/basics.pt`](../../examples/basics.pt) and notice how
`"{hero.name} has {hero.health} health"` builds a sentence from fields.

Try in the REPL:

```plaintext
n = 3
print("You have {n} apples")
```

## Common mistakes

| Mistake | Fix |
|---------|-----|
| `"hi" + 5` | Use `"hi {5}"` or `to_text` |
| Forgetting quotes around text | `"Ada"`, not `Ada`, unless `Ada` is a name |

**Previous:** [Hello ←](01-hello.md) · **Next:** [Decisions →](03-decisions.md)
