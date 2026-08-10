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

## Saving progress

```plaintext
data = load("game.save") otherwise dictionary { "best": 0 }   // nothing on first run
save(data, "game.save")     // any value or class; written atomically
has_save("game.save")       // true/false
```

## Game (sketch)

```plaintext
import math

ship = load_sprite("assets/ship.png")   // image file, relative to where you run plaintext
beep = load_sound("assets/beep.wav")
tune = load_music("assets/theme.ogg")   // streamed; separate id space from sounds

game "Title" (width: 800, height: 600) {
    on update(delta) {
        if key_down("left") { decrease x by 200 * delta }   // held
        if key_pressed("space") { play_sound(beep) }        // just pressed
        center_camera(x, y)                                 // world scroll
        // camera_bounds(0, 0, world_w, world_h)
        // burst(x, y, orange, 16)  // also speed: / life:
        // play_sound(beep, loop: true) / stop_sound(beep)
        // play_music(tune) / fade_music(tune, 0, 2) / stop_music(tune)
        // set_sound_volume / set_music_volume (0..1), also _pitch / _pan
    }
    on draw() {
        clear_screen(skyblue)
        draw_circle(x, y, 20, red)                          // world space
        draw_sprite(ship, x, y)                             // also _scaled / _rotated
        // sheet = load_sprite_sheet(..., cell_width: 32, cell_height: 32)
        // draw_frame(sheet, frame, x, y) / flip_x: true
        draw_text_screen("Score: {score}", 20, 20, 24, white)  // HUD (screen space)
    }
}
```

Input: `key_down` / `key_pressed` (`"up"`, `"space"`, `"w"`, …), `mouse_x()`, `mouse_pressed()`.

## UI (sketch)

```plaintext
name = ""
volume = 50
choices = ["A", "B", "C"]
picked = 0

window "Title" (width: 420, height: 320) {
    scroll (height: 280, padding: 24, spacing: 12) {
        text "Clicked {n} times" (size: 24)
        text_field (bind: name, width: 280)
        text_field (bind: notes, multiline: true, width: 280, height: 80)
        checkbox "Notify me" (bind: subscribed)
        slider (bind: volume, min: 0, max: 100, step: 1, width: 280)
        list (items: choices, bind: picked, height: 100)
        dropdown (items: choices, bind: picked)
        image (sprite: logo, width: 64, height: 64)
        button "More" (on_click: increment)
    }
}
```

## Game kit (`import gamekit`)

```plaintext
import gamekit

world = physics_world(gravity: 1800)
hero = body(x: 100, y: 100, width: 28, height: 40, solid: true)
ground = body(x: 0, y: 560, width: 800, height: 40, solid: true, static: true)
hurt = hitbox(owner: hero, width: 28, height: 40, kind: "hurt")
attack = hitbox(owner: hero, offset_x: 24, width: 28, height: 20, kind: "attack", active: false)

world.add(hero)
world.add(ground)
world.add(hurt)
world.add(attack)

level = tilemap(cell_size: 32, rows: ["####", "#..#", "####"])
world.add_tilemap(level, solid_tiles: ["#"])
// paint in a GUI: plaintext edit_tilemap my_level.pt  → also writes level_tiles = dictionary {…}

world.step(delta)
if pressed("jump") { hero.jump(700) }
attack.active = key_down("z")
if world.hits(attack, enemy_hurt) { /* once per swing */ }

draw_tilemap(level, tile_colors: dictionary { "#": gray })
draw_body(hero, blue)
draw_hitboxes(world)
```

## Web (`import web`)

```plaintext
import web

data = web.get_json("examples/fixtures/sample.json")  // or an https:// URL
page = web.get("https://example.com")
web.post_json(url, dictionary { "a": 1 })

text = to_json(dictionary { "name": "Ada" })
value = parse_json("{\"x\": 1}")
```

Local paths work offline; `http://` / `https://` talk to the network.

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
