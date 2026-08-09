# 5. Functions

```plaintext
make function called add(a: Number, b: Number) {
    return a + b
}

print(add(2, 3))
```

- Write parameter types after `:`.
- Never write a return type — PlainText infers it from `return`.
- Defaults are allowed: `name: Text = "world"`.
- Leave the type off a parameter to make it **flexible** (`Dynamic`) — checked at run time.

## Try it

[`examples/cart.pt`](../../examples/cart.pt) uses a flexible `items` parameter and walks a list
of `Item`s.

**Next:** [Classes →](06-classes.md)
