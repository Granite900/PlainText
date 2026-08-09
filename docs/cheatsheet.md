# PlainText cheatsheet

One page of the shapes you will type most often. Details live in the
[language reference](language-reference.md).

## Run

```bash
plaintext run file.pt
plaintext check file.pt
plaintext repl
```

## Values & names

```plaintext
age = 5
name = "Ada"
ok = true
increase score by 10
decrease lives by 1
print("Hello, {name}!")     // interpolation — not "Hello, " + name + age
```

## Decisions

```plaintext
if score is at least 90 {
    print("A")
} else if score is more than 70 {
    print("B")
} else {
    print("C")
}
```

| Words | Symbols |
|-------|---------|
| `is` / `is not` | `==` / `!=` |
| `is at least` / `is at most` | `>=` / `<=` |
| `is more than` / `is less than` | `>` / `<` |

Also: `and`, `or`, `not`. Optionals: `value is nothing` / `is not nothing`.

## Loops

```plaintext
for every item in items { … }
repeat 3 times { … }
while ready { … }
loop { if done { break } }
```

## Functions & classes

```plaintext
make function called add(a: Number, b: Number) {
    return a + b
}

class Player {
    name: Text
    health = 100
    make function called hurt(n: Number) {
        decrease self.health by n
    }
}

hero = Player { name: "Kara" }
```

## Collections

```plaintext
nums = [3, 1, 2]
nums.append(4)
print(nums.sorted())
print(nums.transformed_by(double))
print(nums.kept_if(is_even))
print(nums.combined(0, add))

ages = dictionary { "Ada": 36 }
ages["Grace"] = 45
```

Empty list needs a type: `names: Text list = []`.

## Imports

```plaintext
import math                 // sqrt, pi, random_between, …
import "./helpers.pt"       // another file (relative path)
```

## Console

```plaintext
print("hi")
name = input("Name? ")
exit()                      // optional code 0–255
```

## Game (sketch)

```plaintext
import math

game "Title" (width: 800, height: 600) {
    on update(delta) { … }
    on draw() {
        clear_screen(skyblue)
        draw_circle(x, y, 20, red)
    }
}
```

## UI (sketch)

```plaintext
window "Title" (width: 420, height: 260) {
    column (padding: 24, spacing: 12, align: center) {
        text "Clicked {n} times" (size: 24)
        button "More" (on_click: increment)
    }
}
```

## Where to go next

- Lessons: [docs/README.md](README.md)
- Full guide: [language-reference.md](language-reference.md)
- Stuck: [troubleshooting.md](troubleshooting.md)
