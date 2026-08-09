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

Recover from failure: `count = try to_number(text) otherwise 0` — `try` turns a would-be crash
into `nothing`; `otherwise` supplies a default (also works on `list.first()`, `dict[key]`, …).

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

double = make function (n) { return n * 2 }   // inline/anonymous — a function value

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

ship = load_sprite("assets/ship.png")   // image file, relative to where you run plaintext
beep = load_sound("assets/beep.wav")

game "Title" (width: 800, height: 600) {
    on update(delta) {
        if key_down("left") { decrease x by 200 * delta }   // held
        if key_pressed("space") { play_sound(beep) }        // just pressed
    }
    on draw() {
        clear_screen(skyblue)
        draw_circle(x, y, 20, red)
        draw_sprite(ship, x, y)                             // also _scaled / _rotated
        draw_text("Score: {score}", 20, 20, 24, white)
    }
}
```

Input: `key_down` / `key_pressed` (`"up"`, `"space"`, `"w"`, …), `mouse_x()`, `mouse_pressed()`.

## UI (sketch)

```plaintext
name = ""
volume = 50

window "Title" (width: 420, height: 320) {
    column (padding: 24, spacing: 12, align: left) {
        text "Clicked {n} times" (size: 24)
        text_field (bind: name, width: 280)
        checkbox "Notify me" (bind: subscribed)
        slider (bind: volume, min: 0, max: 100, step: 1, width: 280)
        image (sprite: logo, width: 64, height: 64)
        button "More" (on_click: increment)
    }
}
```

## Neural networks (`import ai`)

```plaintext
import ai

brain = neural_network(inputs: 2, hidden: [8], outputs: 1)   // device: auto to use a GPU
brain.train(examples, answers, epochs: 3000, optimizer: adam, rate: 0.05)
brain.predict([1, 0])                     // → a list, one number per output
brain.save("brain.ai")                    // load_network("brain.ai") to reload

data = load_dataset("training.csv", outputs: 1)   // examples = data[0], answers = data[1]
```

Neuroevolution (learn from a score, no answers — great for game agents):

```plaintext
brains = population(count: 100, inputs: 4, hidden: [8], outputs: 2)
brains = evolve(brains, scores, mutation: 0.1, keep: 4)   // best_of(brains, scores) = the champion
```

## Where to go next

- Lessons: [docs/README.md](README.md)
- Full guide: [language-reference.md](language-reference.md)
- Stuck: [troubleshooting.md](troubleshooting.md)
