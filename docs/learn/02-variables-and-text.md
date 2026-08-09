# 2. Variables and text

## Creating names

```plaintext
age = 5
age = age + 1        // now 6

increase score by 10 // same as score = score + 10
decrease lives by 1
```

The first time you write a name, it is created. Types are usually inferred: `5` is a
`Number`, `"hi"` is `Text`, `true` is a `Boolean`.

## Interpolation

Put `{…}` inside a string to insert a value:

```plaintext
name = "Ada"
print("Hello, {name}!")
print("Next year: {age + 1}")
```

You **cannot** add text and a number with `+`. Use interpolation (or `to_text`).

## Try it

Run the first half of [`examples/basics.pt`](../../examples/basics.pt) and notice how
`"{hero.name} has {hero.health} health"` builds a sentence.

**Next:** [Decisions →](03-decisions.md)
