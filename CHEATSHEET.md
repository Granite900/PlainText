# PlainText Cheatsheet

A dense, `Ctrl+F`-friendly reference for the whole language. For prose explanations see
[`docs/language-reference.md`](docs/language-reference.md); for lessons see
[`docs/README.md`](docs/README.md).

Files end in `.pt`. Statements are one per line — no semicolons. Blocks use `{ }`.

---

## CLI commands

```bash
plaintext run <file.pt>              # type-check, then run
plaintext check <file.pt>            # type-check only (no run)
plaintext build <file.pt>            # bundle into a standalone executable
plaintext build <file.pt> -o Game    # choose output name
plaintext build <file.pt> --run      # build, then run it
plaintext build <file.pt> --runtime <bin>   # bundle onto another OS's runtime (cross-build)
plaintext repl                       # interactive session (type `exit` to leave)
plaintext new <name>                 # scaffold a project folder
plaintext edit_tilemap <file.pt>     # GUI tilemap painter (writes <file>.tiles.pt)
plaintext lsp                        # language server over stdio (for editors)
plaintext version                    # print version   (also --version, -v)
plaintext help                       # usage            (also --help, -h)
```

A top-level `game` or `window` block makes `run` open a window; otherwise it runs on the console.

---

## Comments

```plaintext
// line comment
/* block
   comment */
```

---

## Variables & types

```plaintext
age = 5              // Number   (one numeric type: 5, -2, 3.14)
name = "Ada"         // Text
ok = true            // Boolean  (true / false)
items = [1, 2, 3]    // list
ages = dictionary { "a": 1 }   // dictionary
missing = nothing    // nothing  (absence of a value)

pi: Number = 3.14159 // optional explicit type annotation
```

Built-in types: **Number**, **Text**, **Boolean**, **list**, **dictionary**, **nothing**,
optionals **`T?`**, your own **classes**, and (with modules) `body` / `hitbox` / physics world /
tilemap / network.

**Type is required only when it can't be inferred:**

```plaintext
scores: Number list = []      // empty list
nickname: Text? = nothing     // bare nothing
make function called f(x) {}  // untyped param → flexible (Dynamic)
class C { field: Text }       // class field with no default
```

---

## Operators

```plaintext
a + b   a - b   a * b   a / b   a % b     // arithmetic (Number only)
-a                                        // negation
a and b   a or b   not a                  // logical (short-circuit)
x = value                                 // assign / declare
increase score by 10                      // score = score + 10
decrease lives by 1                       // lives = lives - 1
```

> `+` never mixes Number and Text — use interpolation or `to_text(x)`.

### Comparisons (symbols and word forms are identical)

| Symbol | Words |
|--------|-------|
| `==`   | `is` |
| `!=`   | `is not` |
| `>=`   | `is at least` |
| `<=`   | `is at most` |
| `>`    | `is more than` / `is greater than` |
| `<`    | `is less than` / `is fewer than` |

```plaintext
if score is at least 90 { }
if name is "Ada" { }
if lives is at most 0 { }
value is nothing        // optional test
value is not nothing
```

---

## Text & interpolation

```plaintext
name = "Ada"
print("Hi {name}, {5 + 5} years")   // any expression inside { }
```

---

## Optionals & error recovery

```plaintext
if c.nickname is not nothing { print(c.nickname) }   // narrow before use

notes = try read_file("notes.txt") otherwise "none"  // try → nothing on failure
count = try to_number(answer) otherwise 0
best  = scores.first() otherwise 0                   // otherwise on any optional
rate  = settings["speed"] otherwise 1
```

`try expr` yields `nothing` instead of crashing (its type gains `?`). `value otherwise fallback`
substitutes when `value` is `nothing`. `try` never swallows `exit(...)`.

---

## Control flow

```plaintext
if x > 0 { } else if x < 0 { } else { }   // conditions must be Boolean (no truthy)

for every item in items { }               // list, dictionary, or text
repeat 3 times { }
while ready { }
loop { if done { break } }

break        // leave the loop
continue     // skip to next iteration
```

> **Scope:** only functions create a new scope. Variables made inside `if`/`for`/`while`/`loop`
> stay visible afterward.

---

## Functions

```plaintext
make function called add(a: Number, b: Number) {   // return type is always inferred
    return a + b
}

make function called greet(name: Text = "world") { print("Hi {name}") }   // default
make function called total(items) { }              // untyped param = flexible (Dynamic)

double = make function (n) { return n * 2 }        // anonymous = a value
print(double(7))                                   // 14
```

- Never write the return type.
- Anonymous functions **close over** their surroundings.
- Pass functions to list helpers, `on_click`, and `after`/`every`.

---

## Classes

```plaintext
class Player {
    name: Text                // field: type
    x: Number
    health = 100              // field with default (type inferred)

    make function called take_damage(amount: Number) {   // method; `self` = instance
        self.health = self.health - amount
    }
}

hero = Player { name: "Kara", x: 0 }   // construct (unset optional fields → nothing)
hero.x = 10                            // set field
print(hero.name)                       // read field
hero.take_damage(30)                   // call method
```

Every field needs a type **or** a default.

---

## Collections

```plaintext
nums = [3, 1, 2]
nums.append(4)          // or nums.add(4)
nums[0]                 // index from 0
nums[0] = 9             // index-assign

ages = dictionary { "Ada": 36 }
ages["Ada"]             // 36
ages["Grace"] = 45      // add / update
```

**List methods:** `length`, `is_empty`, `append`/`add`, `pop`, `get`, `contains`, `first`,
`last`, `index_of`, `remove_at`, `reversed`, `join`, `sorted`, `transformed_by`, `kept_if`,
`combined`.

```plaintext
nums.sorted()                 // new sorted list (all-number or all-text)
nums.transformed_by(double)   // map: run f over each item
nums.kept_if(is_even)         // filter: keep where f is true
nums.combined(0, add)         // fold: add(running, item) from start 0
```

**Dictionary methods:** `length`, `is_empty`, `has`, `get`, `keys`, `values`, `remove`.
Keys may be text, numbers, or booleans.

**Text methods:** `length`, `upper`, `lower`, `trim`, `contains`, `starts_with`, `ends_with`,
`replace`, `repeat`, `split`, `substring`.

---

## Imports

```plaintext
import math                 // math functions + pi, e   (see below)
import gamekit              // physics, bodies, hitboxes, tilemaps
import web                  // HTTP + JSON
import ai                   // neural networks
import "./helpers.pt"       // another file (relative path); an imported file has no game/window
```

---

## Standard library (always available)

```plaintext
// Console / IO
print(x)                    // print any value
input("prompt ")            // read a line → Text
exit()   exit(code)         // stop (code 0–255); closes a game/window

// Convert
to_text(x)   to_number(text)   length(x)

// Files
read_file(path)   write_file(path, text)   append_file(path, text)   file_exists(path)

// Save / load (any value or class instance; atomic write)
save(value, path)   load(path)   has_save(path)   // missing file loads as nothing

// Data files
read_csv(path)                       // rows of numbers (skips 1 header line + `#` lines)
load_dataset(path, outputs: n)       // → [examples, answers]

// Image files (PNG/JPG/BMP/GIF/TGA/PSD/HDR/PIC/PNM)
read_image(path, width: 28, height: 28, rgb: false)   // → flat 0..1 pixel numbers
load_image_dataset(folder, width: 28, height: 28)      // one subfolder per label → [examples, answers]

// Time
now()      // seconds since 1970
clock()    // seconds since program start

// Timers (delay in seconds, function to run)
after(2, fn)     // once, later
every(1, fn)     // repeatedly (cannot be cancelled)
```

## Math (`import math`)

```plaintext
min(a, b)   greatest(a, b)   abs(x)   sqrt(x)   floor(x)   ceil(x)
round(x)   round(x, places)  pow(base, exp)   clamp(x, lo, hi)
sin(x)   cos(x)   tan(x)   random_between(lo, hi)
pi   e                 // constants (defined as globals — don't reuse these names)
```

---

## Games — `game "Title" (width:, height:) { }`

```plaintext
game "Title" (width: 800, height: 600) {
    ship = load_sprite("ship.png")     // init area: runs once
    on start()  { }                    // once at launch
    on update(delta) { }               // each frame; delta = seconds since last
    on draw()   { }                    // each frame; paint here
}
```

**Drawing (world space):**
`clear_screen(color)`, `draw_circle(x, y, r, color)`, `draw_rectangle(x, y, w, h, color)`,
`draw_line(x1, y1, x2, y2, color)`, `draw_text(text, x, y, size, color)`.

**HUD (screen space, ignores camera):**
`draw_text_screen(text, x, y, size, color)`, `draw_rectangle_screen(x, y, w, h, color)`.

**Colors:** named (`red blue green yellow white black skyblue gray darkgray orange gold …`) or
`rgb(r, g, b)` / `rgba(r, g, b, a)` (0–255).

**Sprites:**
`load_sprite(path)`, `draw_sprite(id, x, y)`, `draw_sprite_scaled(id, x, y, scale)`,
`draw_sprite_rotated(id, x, y, deg)`, `sprite_width(id)`, `sprite_height(id)`.

**Sprite sheets:**
`load_sprite_sheet(path, cell_width:, cell_height:)`, `frame_count(sheet)`,
`draw_frame(sheet, frame, x, y)`, `draw_frame(sheet, frame, x, y, flip_x: true)`,
`draw_frame_scaled(sheet, frame, x, y, scale)`.

**Camera:**
`set_camera(x, y)`, `center_camera(x, y)`, `camera_bounds(min_x, min_y, max_x, max_y)`,
`camera_x()`, `camera_y()`.

**Particles:** `burst(x, y, color, count)` (optional `speed:`, `life:`).

**Input:**
`key_down(name)`, `key_pressed(name)` (`"up" "down" "left" "right" "space" "enter" "escape" "w"`…),
`mouse_x()`, `mouse_y()`, `mouse_down()`, `mouse_pressed()`, `screen_width()`, `screen_height()`.

**Audio (sound ids ≠ music ids):**
`load_sound(path)`, `play_sound(id)`, `play_sound(id, loop: true)`, `stop_sound(id)`,
`set_sound_volume(id, 0..1)`, `set_sound_pitch(id, n)`, `set_sound_pan(id, 0..1)`,
`load_music(path)`, `play_music(id)`, `stop_music(id)`, `set_music_volume/pitch/pan(id, n)`,
`fade_music(id, target, seconds)`.

**Fonts:** `load_font("path.ttf")` → pass as `font:`.

---

## Desktop UI — `window "Title" (width:, height:) { }`

```plaintext
window "Counter" (width: 420, height: 260, bg: rgb(24, 28, 40)) {
    column (padding: 24, spacing: 16, align: center) {
        text "Clicked {n} times" (size: 26, color: white)
        row (spacing: 12) {
            button "More" (on_click: increment)
            button "Reset" (on_click: reset, bg: gray)
        }
    }
}
```

**Widgets:** `column`, `row`, `scroll`, `text`, `button`, `spacer`, `text_field`
(`multiline: true`), `checkbox`, `slider`, `list`, `dropdown`, `image`.

**Props:** `padding`, `spacing`, `align` (`center left right top bottom`), `size`, `width`,
`height`, `color`, `bg`/`background`, `font`, `sprite`, `on_click`, `bind`, `on_change`,
`value`/`checked`, `min`, `max`, `step`, `items`, `multiline`.

```plaintext
text_field (bind: name, width: 320)                 // read/write a variable
slider (bind: volume, min: 0, max: 100, step: 1)
text_field (value: name, on_change: make function (new) { name = new })
```

Focus moves with **Tab** / **Shift+Tab**.

---

## Game kit (`import gamekit`)

```plaintext
world = physics_world(gravity: 1800)
hero  = body(x: 100, y: 100, width: 28, height: 40, solid: true)
floor = body(x: 0, y: 560, width: 800, height: 40, solid: true, static: true)
hurt  = hitbox(owner: hero, offset_x: 0, offset_y: 0, width: 28, height: 40, kind: "hurt")
atk   = hitbox(owner: hero, offset_x: 24, width: 28, height: 20, kind: "attack", active: false)

world.add(hero)   world.add(floor)   world.add(hurt)   world.add(atk)
world.remove(hero)                    // take a body or hitbox back out
world.step(delta)                     // in on update
if world.hits(atk, enemy_hurt) { }    // once per swing
if overlaps(hurt, enemy_hurt) { }     // every frame
if pressed("jump") { hero.jump(700) } // edge trigger; "jump" → space/up/w
```

**Body fields:** `x y width height vx vy solid static on_ground center_x center_y`.
**Body methods:** `move(dx, dy)`, `set_velocity(vx, vy)`, `bump(vx, vy)`, `jump(speed)`
(`jump(speed, force: true)` in mid-air).

**Tilemaps:**

```plaintext
level = tilemap(cell_size: 32, rows: ["######", "#..P.#", "######"], solid_tiles: ["#"])
world.add_tilemap(level, solid_tiles: ["#"])
tile_at(level, col, row)                                       // "P" or nothing
draw_tilemap(level, tile_colors: dictionary { "#": gray })     // color per char
draw_tilemap(level, tile_images: dictionary { "#": "wall.png" })   // PNG per char
```

Tilemap fields: `cell_size`, `width`, `height`. **Draw:** `draw_body`, `draw_hitbox`,
`draw_hitboxes(world)`, `draw_tilemap`.

---

## Web & JSON (`import web`)

```plaintext
web.get(url)                          // response body as Text
web.get_json(url_or_local_path)       // → dictionary / list / …
web.post_json(url, dictionary { })    // POST JSON, returns Text
to_json(value)                        // value → JSON Text
parse_json(text)                      // JSON Text → value
```

Local paths read from disk (offline); `http://` / `https://` hit the network.

---

## Neural networks (`import ai`)

```plaintext
brain = neural_network(inputs: 2, hidden: [8], outputs: 1)   // hidden: number or list
brain = neural_network(inputs: 2, hidden: [16, 12], outputs: 1, device: auto)

brain.train(examples, answers, epochs: 3000, optimizer: adam, rate: 0.05, decay: 0)
err = brain.train_once(examples, answers, optimizer: adam)   // one round; returns error
brain.predict([1, 0])                 // → list, one number per output
brain.loss(examples, answers)         // current error, no training
brain.save("brain.ai")                // load_network("brain.ai") to reload

data = load_dataset("training.csv", outputs: 1)   // examples=data[0], answers=data[1]
rows = read_csv("scores.csv")

data = load_image_dataset("digits", width: 28, height: 28)   // one subfolder per label
pixels = read_image("digit.png", width: 28, height: 28)      // one image → flat 0..1 numbers

// Neuroevolution
brains = population(count: 100, inputs: 4, hidden: [8], outputs: 2)
brains = evolve(brains, scores, mutation: 0.1, keep: 4)
champ  = best_of(brains, scores)
```

**Optimizers:** `sgd`, `momentum`, `rmsprop`, `adam`.
**Devices:** `auto`, `gpu`, `cuda`, `rocm`, `mps`, `vulkan`, `dx12`, `cpu`.

---

## Reserved words

```
make function called class if else while for in repeat times loop return
break continue and or not is nothing true false import game window on self
try otherwise list dictionary of to second seconds  (async wait start — reserved, unused)
```

Contextual words (usable as names elsewhere): `every`, `increase`, `decrease`, `by`,
`at`, `least`, `most`, `than`, `greater`, `fewer`.
