# 5. Functions

**Goal:** Package a chunk of work under a name you can call.

## Declaring and calling

```plaintext
make function called add(a: Number, b: Number) {
    return a + b
}

print(add(2, 3))    // 5
```

Rules worth memorizing:

- Write parameter types after `:` (`a: Number`).
- **Never** write a return type — PlainText infers it from what you `return`.
- Defaults are allowed: `name: Text = "world"`.
- A function creates its **own** scope for locals.

## Flexible parameters

Leave the type off a parameter to make it **Dynamic** (checked at run time). Useful when
you accept “whatever shape this list of objects has”:

```plaintext
make function called total_cost(items) {
    total = 0
    for every item in items {
        increase total by item.price * item.quantity
    }
    return total
}
```

Prefer real types when you can — the checker catches more mistakes.

## Functions without a name

Sometimes you need a *tiny* function just to hand to something else — no reason to name it.
Write it inline with `make function (...) { ... }` (the same shape as a named function, minus
`called <name>`):

```plaintext
nums = [5, 3, 8, 1]
doubled = nums.transformed_by(make function (n) { return n * 2 })
evens = nums.kept_if(make function (n) { return n % 2 is 0 })
```

An inline function **remembers the variables around it** — it can use names from the scope
where you wrote it:

```plaintext
bonus = 100
boosted = nums.transformed_by(make function (n) { return n + bonus })
```

You can also store one in a variable and call it later, just like a named function:

```plaintext
triple = make function (n) { return n * 3 }
print(triple(7))    // 21
```

These are handy for [collections](07-collections.md) (`transformed_by`, `kept_if`,
`combined`), button `on_click` handlers, and `after`/`every` timers.

## Practice

Open [`examples/cart.pt`](../../examples/cart.pt). It defines an `Item` class and a
`total_cost` function with a flexible `items` parameter. Run it:

```bash
plaintext run examples/cart.pt
```

## Common mistakes

| Mistake | Fix |
|---------|-----|
| Writing `-> Number` or a return type | Omit it; use `return` |
| Forgetting `return` when you need a value | Last expression is **not** auto-returned |

**Previous:** [Loops ←](04-loops.md) · **Next:** [Classes →](06-classes.md)
