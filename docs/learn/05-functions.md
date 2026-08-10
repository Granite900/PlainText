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

### Careful: capturing a loop variable

"Remembers the variables around it" is literal — an inline function holds on to the *variable*,
not a copy of its value. If you build functions inside a loop and save them for later, they all
end up sharing the loop variable's **final** value:

```plaintext
actions: Anything list = []
for every i in [1, 2, 3] {
    actions.append(make function () { return i })   // all three would return 3
}
```

Each saved function would report `3`, because by the time you call them the loop has finished
and `i` is `3`. PlainText **flags this** at check time. If you need each function to remember its
own value, pass it in as a parameter instead:

```plaintext
make function called remember(value) {
    return make function () { return value }        // `value` is this call's own
}

actions: Anything list = []
for every i in [1, 2, 3] {
    actions.append(remember(i))                     // 1, 2, 3
}
```

(Calling an inline function *right away* inside the loop — like `transformed_by(...)` does — is
always fine; this only bites when you store it and call it after the loop moves on.)

## Practice

Open [`examples/basics.pt`](../../examples/basics.pt). Near the end it defines an `Item`
class and a `total_cost` function with a flexible `items` parameter (no type → Dynamic).
Run it:

```bash
plaintext run examples/basics.pt
```

## Common mistakes

| Mistake | Fix |
|---------|-----|
| Writing `-> Number` or a return type | Omit it; use `return` |
| Forgetting `return` when you need a value | Last expression is **not** auto-returned |

**Previous:** [Loops ←](04-loops.md) · **Next:** [Classes →](06-classes.md)
