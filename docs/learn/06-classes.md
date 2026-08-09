# 6. Classes

**Goal:** Group related data and the functions that belong with it.

## Defining and creating

```plaintext
class Player {
    name: Text          // required when you create a Player
    health = 100        // optional; defaults to 100

    make function called take_damage(amount: Number) {
        decrease self.health by amount
        if self.health is at most 0 {
            print("{self.name} has been defeated")
        }
    }
}

hero = Player { name: "Kara" }   // health uses the default
hero.take_damage(30)
print("{hero.name} has {hero.health} health")
```

Output:

```
Kara has 70 health
```

## Rules

- Every field needs a **type** or a **default** (or both).
- Inside a method, `self` is the instance you called.
- Optional fields use `?`: `nickname: Text?`.
- Create instances with `TypeName { field: value, … }`.

Classes are how games keep “a fruit”, “a player”, or “a button’s state” tidy — see
[`examples/catch.pt`](../../examples/catch.pt) for a fuller game-sized example later.

## Practice

Study the `Player` class in [`examples/basics.pt`](../../examples/basics.pt). Add another
method (for example `heal`) and call it.

## Common mistakes

| Mistake | Fix |
|---------|-----|
| Forgetting `self.` inside a method | `self.health`, not bare `health`, for the field |
| Missing type and default on a field | Write `name: Text` or `health = 100` |

**Previous:** [Functions ←](05-functions.md) · **Next:** [Lists and dictionaries →](07-collections.md)
