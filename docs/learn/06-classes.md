# 6. Classes

A class groups related data and its methods.

```plaintext
class Player {
    name: Text
    health = 100

    make function called take_damage(amount: Number) {
        decrease self.health by amount
    }
}

hero = Player { name: "Kara" }
hero.take_damage(30)
```

- Every field needs a type **or** a default.
- Inside a method, `self` is the instance.
- Optional fields use `?` (for example `nickname: Text?`).

## Try it

The `Player` class in [`examples/basics.pt`](../../examples/basics.pt).

**Next:** [Lists and dictionaries →](07-collections.md)
