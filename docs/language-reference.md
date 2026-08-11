# The PlainText Language Reference

PlainText is a small language designed to read like plain English. This guide is the **full
lookup** for syntax, types, the standard library, games, and UI.

**New here?** Start with the [lesson path](README.md) instead — then come back when you need
detail. Keep the [cheatsheet](cheatsheet.md) open while you code.

It assumes you can run programs with `plaintext run yourfile.pt` (version **2.8.0+**).

## Contents

1. [The basics](#1-the-basics)
2. [Text and interpolation](#2-text-and-interpolation)
3. [Functions](#3-functions)
4. [Making decisions](#4-making-decisions)
5. [Loops](#5-loops)
6. [Classes](#6-classes-your-own-types)
7. [Optionals and nothing](#7-optionals-and-nothing)
8. [Collections](#8-collections)
9. [Timers](#9-timers)
10. [Making a game](#10-making-a-game)
11. [Making a desktop UI](#11-making-a-desktop-ui)
12. [The standard library at a glance](#12-the-standard-library-at-a-glance)
13. [Splitting a program across files](#13-splitting-a-program-across-files)
14. [Game kit (the gamekit module)](#14-game-kit-the-gamekit-module)
15. [Web and JSON (the web module)](#15-web-and-json-the-web-module)
16. [Neural networks (the ai module)](#16-neural-networks-the-ai-module)
17. [The REPL](#17-the-repl)
18. [Building a standalone app](#18-building-a-standalone-app)
19. [Appendix: when do I have to write a type?](#19-appendix-when-do-i-have-to-write-a-type)

---

## 1. The basics

A program is a list of statements, one per line. No semicolons.

```plaintext
print("Hello!")
```

Comments use `//` for a line and `/* ... */` for a block:

```plaintext
// this is a note
print("hi")   /* so is this */
```

### Variables

Write `name = value`. The first time you write a name it is created; after that, the same
syntax reassigns it.

```plaintext
age = 5
age = age + 1        // now 6
```

Adding to or subtracting from a variable is common enough to have a word form — `increase … by`
and `decrease … by` — which reads like an instruction:

```plaintext
score = 0
increase score by 10     // same as: score = score + 10
decrease score by 3      // same as: score = score - 3
```

You don't write the type — PlainText infers it. `age` is a `Number`, `"hi"` is `Text`,
`true` is a `Boolean`. You *may* write a type when you want to:

```plaintext
pi: Number = 3.14159
```

### The built-in types

- **Number** — one numeric type for everything (`5`, `-2`, `3.14`).
- **Text** — a string of characters (`"hello"`).
- **Boolean** — `true` or `false`.
- **list** — an ordered collection: `[1, 2, 3]`.
- **dictionary** — key/value pairs: `dictionary { "a": 1, "b": 2 }`.
- **nothing** — the absence of a value (see [Optionals](#7-optionals-and-nothing)).

---

## 2. Text and interpolation

Put an expression in `{curly braces}` inside a string and it's inserted:

```plaintext
name = "Ada"
print("Hello {name}, you are {5 + 5} years old.")
// Hello Ada, you are 10 years old.
```

Numbers and text don't mix with `+` — that's a mistake PlainText catches for you. Use
interpolation, or `to_text(x)`:

```plaintext
count = 3
print("You have " + count)      // error: can't add a Text and a Number
print("You have {count}")       // good
```

---

## 3. Functions

Declare a function with `make function called <name>(<parameters>)`:

```plaintext
make function called add(a: Number, b: Number) {
    return a + b
}

print(add(2, 3))    // 5
```

- Parameter types are written after `:`.
- You **never** write the return type — it's inferred from what you `return`.
- Parameters can have defaults, and then their type is inferred too:

```plaintext
make function called greet(name: Text = "world") {
    print("Hello, {name}!")
}

greet()          // Hello, world!
greet("Ada")     // Hello, Ada!
```

### Flexible (untyped) parameters

If you leave a parameter's type off entirely, it becomes **flexible** (`Dynamic`): PlainText
stops type-checking how you use it and checks at run time instead. This is an escape hatch —
handy, but you lose compile-time safety for that value and anything computed from it.

```plaintext
make function called total_cost(items) {         // `items` is flexible
    total = 0
    for every item in items {
        total = total + item.price * item.quantity
    }
    return total
}
```

### Anonymous (inline) functions

A function you don't need to name is written `make function (<parameters>) { <body> }` — the
same shape as a named one, minus `called <name>`. It's a **value**: pass it straight to
something, or store it in a variable.

```plaintext
nums = [5, 3, 8, 1]
doubled = nums.transformed_by(make function (n) { return n * 2 })

triple = make function (n) { return n * 3 }
print(triple(7))    // 21
```

An inline function **closes over** its surroundings — it can read variables from the scope
where it was written:

```plaintext
bonus = 100
boosted = nums.transformed_by(make function (n) { return n + bonus })
```

Typical uses: the list helpers `transformed_by` / `kept_if` / `combined` (§8), a button's
`on_click` (§11), and `after` / `every` timers (§9). Parameter types are optional here too —
leave them off for a flexible parameter.

> **Capturing a loop variable.** Because a closure holds the *variable*, not a snapshot, saving
> a loop-made function for later (`list.append(make function () { return i })`) means every copy
> reads the loop's final value. PlainText rejects that pattern at check time — pass the value
> into a helper that returns the function so each keeps its own. Calling a lambda immediately
> (as `transformed_by` does) is always fine. See the [functions lesson](learn/05-functions.md).

---

## 4. Making decisions

```plaintext
if score >= 90 {
    print("A")
} else if score >= 80 {
    print("B")
} else {
    print("C")
}
```

Conditions must be a `Boolean` — there's no "truthy" shortcut, so write a real comparison
(`x > 0`, `name == "Ada"`). Combine conditions with the words `and`, `or`, `not`:

```plaintext
if age >= 18 and has_ticket {
    print("Welcome")
}
```

Comparisons come in two spellings — symbols, or words that read like a sentence. They mean
exactly the same thing; use whichever is clearer:

| symbol | in words |
|--------|----------|
| `==`   | `is` |
| `!=`   | `is not` |
| `>=`   | `is at least` |
| `<=`   | `is at most` |
| `>`    | `is more than` (or `is greater than`) |
| `<`    | `is less than` (or `is fewer than`) |

```plaintext
if score is at least 90 {
    print("A")
}
if name is "Ada" {
    print("hi Ada")
}
if lives is at most 0 {
    print("game over")
}
```

(The `is nothing` / `is not nothing` check for optionals is the same `is`, see [Optionals](#7-optionals-and-nothing).)

---

## 5. Loops

```plaintext
// Over each item in a list, dictionary, or text:
for every score in scores {
    print(score)
}

// A fixed number of times:
repeat 3 times {
    print("go")
}

// While a condition holds:
while count > 0 {
    count = count - 1
}

// Forever, until you break out:
loop {
    if done {
        break
    }
}
```

Use `break` to leave a loop and `continue` to skip to the next round.

> Note: loops (and `if`) don't create a new scope — a variable you make inside a loop is still
> there afterward. Only functions start a fresh scope.

---

## 6. Classes (your own types)

A "class" groups related data. Declare one with `class`:

```plaintext
class Player {
    name: Text
    x: Number
    y: Number
    health = 100          // a default value; the type (Number) is inferred
}
```

Every field needs either a type or a default value. Create one by filling in the fields:

```plaintext
hero = Player { name: "Kara", x: 0, y: 0 }
print(hero.name)          // read a field
hero.x = 10               // change a field
```

Classes can have their own functions (methods). Inside a method, `self` is the instance:

```plaintext
class Player {
    name: Text
    health = 100

    make function called take_damage(amount: Number) {
        self.health = self.health - amount
        if self.health <= 0 {
            print("{self.name} is defeated")
        }
    }
}

hero = Player { name: "Kara" }
hero.take_damage(30)
```

---

## 7. Optionals and nothing

A value that might be missing has an optional type, written with a trailing `?`. The missing
value is `nothing`.

```plaintext
class Contact {
    name: Text
    nickname: Text?       // may be text, or nothing
}

c = Contact { name: "Robert" }     // nickname defaults to nothing
```

PlainText won't let you use a possibly-missing value directly — you have to check first. This
prevents a whole class of crashes:

```plaintext
if c.nickname is not nothing {
    print("Goes by {c.nickname}")   // safe here
}
```

Use `is nothing` and `is not nothing` to test.

### Handling things that can fail

Some operations can fail: a file might be missing, text might not be a number, an index might be
out of range. By default those stop the program. Two small words let you recover, both built on
`nothing`:

- **`try expr`** runs `expr` and gives back `nothing` if it would have failed (instead of stopping).
  Its type gains a `?`, so you handle it like any optional.
- **`value otherwise fallback`** uses `fallback` whenever `value` is `nothing`.

```plaintext
// Give a default when something fails:
notes = try read_file("notes.txt") otherwise "no notes yet"
count = try to_number(answer) otherwise 0

// Or branch on the failure:
saved = try load_network("brain.ai")
if saved is nothing {
    print("No saved brain — starting fresh.")
    saved = neural_network(inputs: 2, hidden: [8], outputs: 1)
}
```

`otherwise` also works on anything that already returns an optional, no `try` needed:

```plaintext
best = scores.first() otherwise 0
rate = settings["speed"] otherwise 1
```

(`try` only catches genuine failures — it never swallows an `exit(...)`.)

---

## 8. Collections

### Lists

```plaintext
scores = [90, 82, 74]
scores.append(88)            // add to the end
print(scores.length())       // 4
print(scores[0])             // 90 (index from 0)
print(scores.contains(82))   // true
```

List methods: `length`, `is_empty`, `append`/`add`, `pop`, `get`, `contains`, `first`, `last`,
`index_of`, `remove_at`, `reversed`, `join`, `sorted`, `transformed_by`, `kept_if`, `combined`.

An empty list has no way to guess its element type, so annotate it:

```plaintext
names: Text list = []
```

**Working over a whole list.** Four methods take a *function* and give back a new list (the
original is untouched):

```plaintext
make function called double(n) { return n * 2 }
make function called is_even(n) { return n % 2 == 0 }
make function called add(a, b) { return a + b }

nums = [5, 3, 8, 1]
print(nums.sorted())                // [1, 3, 5, 8]
print(nums.transformed_by(double))  // [10, 6, 16, 2]  — run a function over each item
print(nums.kept_if(is_even))        // [8]             — keep the items it says true for
print(nums.combined(0, add))        // 17              — fold everything into one value
```

`sorted` works on a list of all numbers or all text. `combined(start, f)` calls `f(running, item)`
for each item, beginning from `start`.

The function can be passed by name (as above) or written inline (§3) — often shorter:

```plaintext
print(nums.transformed_by(make function (n) { return n * 2 }))
print(nums.kept_if(make function (n) { return n % 2 is 0 }))
```

### Dictionaries

```plaintext
ages = dictionary { "Ada": 36, "Alan": 41 }
print(ages["Ada"])           // 36
ages["Grace"] = 45           // add or update
print(ages.has("Alan"))      // true
```

Dictionary methods: `length`, `is_empty`, `has`, `get`, `keys`, `values`, `remove`. Keys may be text, numbers, or booleans.

### Text

Text is also a collection of characters:

```plaintext
name = "Ada Lovelace"
print(name.length())              // 12
print(name.upper())               // ADA LOVELACE
print(name.split(" "))            // ["Ada", "Lovelace"]
print(name.starts_with("Ada"))    // true
```

Text methods: `length`, `upper`, `lower`, `trim`, `contains`, `starts_with`, `ends_with`,
`replace`, `repeat`, `split`, `substring`.

---

## 9. Timers

For things that happen later or repeatedly, without freezing the program:

```plaintext
make function called spawn() { /* ... */ }

after(2, spawn)      // run spawn once, 2 seconds from now
every(1, spawn)      // run spawn every 1 second
```

Both take a delay in seconds and a function to run. In a game these are driven by the frame
loop; in a console program they run in real time.

---

## 10. Making a game

A `game` block opens a window and runs a loop. Put your game state at the top (or in the
block's init area), and respond to events with `on` hooks:

```plaintext
class Ball {
    x: Number
    y: Number
    dx: Number
    dy: Number
}

ball = Ball { x: 400, y: 300, dx: 200, dy: 150 }

game "My Game" (width: 800, height: 600) {

    on update(delta) {                 // called every frame; delta = seconds since last frame
        ball.x = ball.x + ball.dx * delta
        ball.y = ball.y + ball.dy * delta
        if key_down("left") {
            ball.dx = ball.dx - 300 * delta
        }
    }

    on draw() {                        // called every frame to paint the screen
        clear_screen(skyblue)
        draw_circle(ball.x, ball.y, 20, red)
        draw_text("Score: 0", 20, 20, 20, black)
    }
}
```

Hooks: `on start()` (once at launch), `on update(delta)` (each frame), `on draw()` (each
frame, for drawing).

### Drawing

- `clear_screen(color)`
- `draw_circle(x, y, radius, color)`
- `draw_rectangle(x, y, width, height, color)`
- `draw_line(x1, y1, x2, y2, color)`
- `draw_text(text, x, y, size, color)`

These use **world** coordinates (see Camera below). For HUD overlays that stay put on the
screen, use `draw_text_screen` / `draw_rectangle_screen` instead.

Colors are named (`red`, `blue`, `green`, `yellow`, `white`, `black`, `skyblue`, `gray`, …) or
built with `rgb(r, g, b)` / `rgba(r, g, b, a)` where each value is 0–255.

### Sprites (images)

```plaintext
game "Sprites" (width: 800, height: 600) {
    ship = load_sprite("ship.png")     // load once, in the init area

    on draw() {
        clear_screen(black)
        draw_sprite(ship, 100, 100)                  // native size
        draw_sprite_scaled(ship, 200, 100, 2)        // 2x
        draw_sprite_rotated(ship, 400, 300, 45)      // rotated 45°, about its center
    }
}
```

Also: `sprite_width(sprite)`, `sprite_height(sprite)`.

### Sprite sheets

A sheet is one PNG with equal-sized cells, left-to-right then top-to-bottom:

```plaintext
sheet = load_sprite_sheet("hero.png", cell_width: 32, cell_height: 32)
frames = frame_count(sheet)

draw_frame(sheet, frame, x, y)
draw_frame_scaled(sheet, frame, x, y, 2)
draw_frame(sheet, frame, x, y, flip_x: true)
```

Advance `frame` yourself in `on update` (timers, walk cycles, …). Out-of-range frames draw
nothing.

### Camera

`set_camera(x, y)` sets the top-left of the view in **world** space. All ordinary draws
(`draw_sprite`, `draw_tilemap`, shapes, world `draw_text`, …) subtract that offset. Follow a
body yourself each frame:

```plaintext
center_camera(hero.center_x, hero.center_y)   // or set_camera(hero.x - 400, hero.y - 300)
camera_bounds(0, 0, level_width, level_height) // keep the view inside the world
```

`camera_bounds` clamps every later `set_camera` / `center_camera` so you don’t scroll past the
level edge. Read the offset with `camera_x()` / `camera_y()`. Mouse stays in **screen** space —
world picking is `mouse_x() + camera_x()`, `mouse_y() + camera_y()`.

HUD (fixed on screen):

```plaintext
draw_text_screen("Score: {score}", 20, 20, 24, white)
draw_rectangle_screen(10, 10, 120, 16, red)
```

### Particles

```plaintext
burst(x, y, orange, 16)                      // world space; short-lived sparks
burst(x, y, gold, 24, speed: 220, life: 0.5)
```

Particles update and draw themselves each frame (with a little gravity). No need to store them.

See [`examples/camera_sheets.pt`](../examples/camera_sheets.pt).

**Where image files go.** `load_sprite("ship.png")` looks for the file **relative to the folder
you run `plaintext` from**, not the folder the `.pt` file is in. A common tidy layout is an
`assets/` folder next to your program, loaded as `load_sprite("assets/ship.png")` and run from
that program's folder. PNG works everywhere; if a sprite doesn't appear, a wrong path is the
usual cause. The same rule applies to `load_sound`, `load_music`, and `load_font`. Supported:
`.png` images, `.wav`/`.ogg`/`.mp3` audio, `.ttf` fonts.

### Input

- `key_down(name)` — held this frame. `key_pressed(name)` — pressed just now. Names: `"up"`,
  `"down"`, `"left"`, `"right"`, `"space"`, `"enter"`, `"escape"`, letters like `"w"`.
- `mouse_x()`, `mouse_y()`, `mouse_down()`, `mouse_pressed()` (screen space).
- `screen_width()`, `screen_height()`.

### Sound & music

Sound ids and music ids are **separate** (a `0` from `load_sound` is not the same as a `0` from
`load_music`). Use the matching `set_sound_*` / `set_music_*` helpers.

```plaintext
beep = load_sound("beep.wav")
tune = load_music("theme.ogg")

play_sound(beep)                 // one-shot
play_sound(beep, loop: true)     // keep restarting until stop_sound
stop_sound(beep)
set_sound_volume(beep, 0.8)      // 0..1
set_sound_pitch(beep, 1.2)       // 1 = normal
set_sound_pan(beep, 0.0)         // 0 = left, 0.5 = center, 1 = right

play_music(tune)                 // streamed; loops by default
set_music_volume(tune, 0.5)
set_music_pitch(tune, 1.0)
set_music_pan(tune, 0.5)
fade_music(tune, 0, 2)           // fade to volume 0 over 2 seconds
stop_music(tune)
```

These only work inside a `game` block (same as drawing). If the audio device can't open, loads
and plays become harmless no-ops rather than crashing. See [`examples/audio.pt`](../examples/audio.pt).

---

## 11. Making a desktop UI

A `window` block describes an interface. It's redrawn continuously, so whatever it shows always
reflects your current state. Buttons run a function when clicked.

```plaintext
counter = 0

make function called increment() {
    counter = counter + 1
}

window "Counter" (width: 420, height: 260, bg: rgb(24, 28, 40)) {
    column (padding: 24, spacing: 16, align: center) {
        text "You clicked {counter} times" (size: 26, color: white)
        row (spacing: 12) {
            button "More" (on_click: increment)
            button "Reset" (on_click: reset, bg: gray)
        }
    }
}
```

### Widgets

- `column { ... }` — stacks children top to bottom.
- `row { ... }` — places children left to right.
- `scroll { ... }` — like a column, but clipped to `height` / `width` and mouse-wheel scrollable.
- `text "..."` — a label (interpolation works).
- `button "..."` — clickable; give it `on_click: <a function>`.
- `spacer` — empty space (`width` / `height`).
- `text_field` — a typeable box; bind a `Text` variable or use `on_change`.
  Add `multiline: true` for wrapped lines, Enter for newlines, and Up/Down caret movement.
- `checkbox "..."` — on/off; bind a `Boolean` or use `on_change`.
- `slider` — a number in a range (`min`, `max`, `step`); bind a `Number` or use `on_change`.
- `list` — scrollable text rows from `items:` (a Text list); bind a selected **index** (Number).
- `dropdown` — closed by default; opens a list popup on click (same `items:` / index bind).
- `image` — shows a sprite from `load_sprite` (`sprite:`, optional `width` / `height`).

**Focus.** Click an input, or use **Tab** / **Shift+Tab** to move between text fields, lists,
dropdowns, checkboxes, and sliders.

### Binding and `on_change`

`bind: myVar` ties a widget to an ordinary variable: each frame the widget reads it, and
user edits write back. That is enough for most forms.

```plaintext
name = ""
volume = 50
text_field (bind: name, width: 320)
slider (bind: volume, min: 0, max: 100, step: 1, width: 320)
```

`on_change: <function>` runs whenever the value changes and receives the new value. Handy
with an inline function:

```plaintext
text_field (
    value: name,
    on_change: make function (new) { name = new }
)
```

You can use `bind` and `on_change` together (write-back, then the handler).

### Properties

Written in parentheses as `name: value`:

- `padding` — space inside a column/row.
- `spacing` — gap between children.
- `align` — `center`, `left`, `right`, `top`, or `bottom` (cross-axis alignment).
- `size` — font size for `text` / button labels.
- `width`, `height` — a fixed size (handy for buttons and fields).
- `color`, `bg` — text color / background (a named color or `rgb(...)`).
- `font` — a font from `load_font("path.ttf")`.
- `sprite` — a sprite from `load_sprite("path.png")` (button face or `image`).
- `on_click` — for buttons, a function to run.
- `bind` — variable name to read/write (`text_field`, `checkbox`, `slider`, `list`, `dropdown`).
- `on_change` — function `(new_value)` when an input changes.
- `value` / `checked` — set the current value without binding (pair with `on_change`).
- `min`, `max`, `step` — slider range and snap.
- `items` — Text list of choices for `list` / `dropdown`.
- `multiline` — `true` for a multi-line `text_field`.

The window itself also accepts `bg` / `background` for the clear color:

```plaintext
ui_font = load_font("C:/Windows/Fonts/segoeui.ttf")  // Windows; on Mac try /System/Library/Fonts/Supplemental/Arial Unicode.ttf
btn = load_sprite("button.png")

window "Demo" (width: 480, height: 300, bg: rgb(24, 28, 40)) {
    text "Hello" (size: 28, color: white, font: ui_font)
    button "Go" (on_click: go, sprite: btn, width: 140, height: 52, color: white)
}

```

See [`examples/form.pt`](../examples/form.pt) for a settings form, and
[`examples/scroll_list.pt`](../examples/scroll_list.pt) for scroll + list + dropdown +
multiline notes.

Menu bars and tab strips are not built in yet — scroll/list/dropdown/multiline cover the
common “more than fits on screen” cases.

---

## 12. The standard library at a glance

**Math** — put `import math` at the top of your file to use these: `min`, `greatest`, `abs`,
`sqrt`, `floor`, `ceil`, `round`, `pow`, `clamp`, `sin`, `cos`, `tan`, `random_between(lo, hi)`,
and the constants `pi` and `e`. `round(x)` rounds to a whole number; `round(x, places)` rounds to
that many decimals — handy for tidy output, e.g. `round(0.98765, 2)` is `0.99`.

```plaintext
import math

print(sqrt(144))          // 12
area = pi * radius * radius
```

Everything else below is always available — no import needed.

**Convert**: `to_text(x)`, `to_number(text)`, `length(x)`.

**Files**: `read_file(path)`, `write_file(path, text)`, `append_file(path, text)`,
`file_exists(path)`.

**Saving progress**: `save(value, path)` writes any value — a number, text, boolean, list,
dictionary, or one of your own **class** values — to a file, and `load(path)` reads it back.
A missing file loads as `nothing`, so pair it with `otherwise` to start fresh on the first run.
`has_save(path)` tells you whether a save exists yet.

```plaintext
class Progress {
    best = 0
    runs = 0
}

progress = load("game.save") otherwise Progress { }   // fresh start the first time
increase progress.runs by 1
save(progress, "game.save")                            // written atomically
```

A class value comes back as a **real instance** — fields, methods, and all — because `save`
tags it with its type. Writes are atomic (a crash mid-save can't corrupt the file), and only the
data types above can be saved (a function or neural network can't). See
[`examples/save.pt`](../examples/save.pt).

**Data files**: `read_csv(path)` reads a file of comma- or space-separated numbers into a list
of rows (a one-line header of names is skipped automatically, and `#` lines are ignored).
`load_dataset(path, outputs: n)` goes one step further for machine learning — it splits each row
into inputs and answers and hands back `[examples, answers]`:

```plaintext
rows = read_csv("scores.csv")            // [[1, 2, 3], [4, 5, 6], ...]

data = load_dataset("training.csv", outputs: 1)
examples = data[0]
answers  = data[1]
```

**Time**: `now()` (seconds since 1970), `clock()` (seconds since the program started).

**Input / output**: `print(...)`, `input(prompt)`.

`input` shows an optional prompt, waits for the person to type a line, and gives it back as
**Text** (use `to_number(...)` if you need a number):

```plaintext
name = input("What's your name? ")
age = to_number(input("Your age? "))
print("Hi {name}, next year you'll be {age + 1}.")
```

**Stopping**: `exit()` ends the program right away; `exit(code)` ends it with a status code
(`0` means success). Inside a `game` or `window`, `exit()` closes the window.

```plaintext
if lives <= 0 {
    print("Game over.")
    exit()
}
```

---

## 13. Splitting a program across files

Keep helpers in their own file and pull them in with `import` and a quoted path:

```plaintext
// helpers.pt
make function called double(n) { return n * 2 }
tax_rate = 0.2
```

```plaintext
// main.pt
import "./helpers.pt"

print(double(21))          // 42
print(100 * tax_rate)      // 20
```

The path is relative to the file doing the importing. Everything defined at the top of the
imported file — functions, classes, and variables — becomes available. Importing the same file
from several places only loads it once, and import cycles are handled safely. (An imported file
can't itself contain a `game` or `window` block.)

---

## 14. Game kit (the `gamekit` module)

Put `import gamekit` at the top for gravity, solid bodies, tagged hitboxes, and tilemaps.
Units are pixels and seconds. Collision is axis-aligned boxes only (no slopes or rotation).

```plaintext
import gamekit

world = physics_world(gravity: 1800)
hero = body(x: 100, y: 100, width: 28, height: 40, solid: true)
ground = body(x: 0, y: 560, width: 800, height: 40, solid: true, static: true)
world.add(hero)
world.add(ground)

// in on update(delta):
world.step(delta)
if pressed("jump") { hero.jump(700) }
```

**Bodies.** Fields: `x`, `y`, `width`, `height`, `vx`, `vy`, `solid`, `static`, `on_ground`,
`center_x`, `center_y`. Methods: `move(dx, dy)`, `set_velocity(vx, vy)`, `bump(vx, vy)`,
`jump(speed)` (optional second arg / `force: true` to jump in mid-air).

**Hitboxes.** `hitbox(owner:, offset_x:, offset_y:, width:, height:, kind:, active:)`.
`kind` is text (`"hurt"`, `"attack"`, …). Methods: `overlaps(other)`. World helpers:
`world.hits(attack, hurt)` (once per swing), free function `overlaps(a, b)`.

**Tilemaps.** Hand-author levels as text rows:

```plaintext
level = tilemap(cell_size: 32, rows: [
    "######",
    "#..P.#",
    "######",
])
world.add_tilemap(level, solid_tiles: ["#"])
ch = tile_at(level, 2, 1)          // "P", or nothing if out of range
draw_tilemap(level, tile_colors: dictionary { "#": gray, ".": darkgray })
```

Fields: `cell_size`, `width`, `height`. Method: `tile_at(col, row)`. Optional
`solid_tiles:` on `tilemap(...)` lets you `world.add(level)` instead.
`draw_tilemap` fills each known character with a solid color (sprite-sheet tiles are out of
scope for now).

**Scenes.** There is no scene-graph API. Switch screens with a variable
(`screen = "menu"` / `"play"`) and `if` branches inside `on update` / `on draw` — see
[`examples/tilemap.pt`](../examples/tilemap.pt).

**Editor.** `plaintext edit_tilemap <file.pt>` opens a paint window that rewrites that file’s
`tilemap` rows / solids and a nearby `<name>_tiles` dictionary (one PNG path per character).
Resize the grid with the **+ / −** column and row buttons. It does not edit bodies or hitboxes.

**Drawing.** `draw_body(body, color)`, `draw_hitbox(hitbox, color)`, `draw_hitboxes(world)`,
`draw_tilemap(map, tile_colors: …)`.

**Input.** `pressed("jump")` — edge-trigger with aliases (`jump` → space/up/w).

**Out of scope.** Slopes, tile animation, image tilesets.

See [`examples/platformer.pt`](../examples/platformer.pt) (hold **H** to outline hitboxes),
[`examples/tilemap.pt`](../examples/tilemap.pt), and [lesson 12](learn/12-game-kit.md).

---

## 15. Web and JSON (the `web` module)

Put `import web` at the top for HTTP and JSON helpers.

**Security note:** these calls can reach the real network. Only request URLs you trust.
Local file paths (anything that is not `http://` or `https://`) are read from disk so
examples and CI can run offline.

```plaintext
import web

page = web.get("https://example.com")                 // response body as Text
data = web.get_json("examples/fixtures/sample.json")  // dictionary / list / …
web.post_json(url, dictionary { "a": 1 })             // POST JSON; returns Text body

text = to_json(dictionary { "name": "Ada" })
value = parse_json("{\"x\": 1}")
```

`to_json` / `parse_json` convert between PlainText values and JSON text. Supported values:
numbers, text, booleans, `nothing`, lists, and dictionaries. Timeouts and network failures
produce readable diagnostics.

See [`examples/fetch.pt`](../examples/fetch.pt) and [lesson 14](learn/14-web.md).

---

## 16. Neural networks (the `ai` module)

Put `import ai` at the top to train a small neural network: it learns a pattern from
example → answer pairs, then predicts answers for new inputs.

```plaintext
import ai

// 2 inputs → one hidden layer of 8 → 1 output
brain = neural_network(inputs: 2, hidden: [8], outputs: 1)

examples = [[0, 0], [0, 1], [1, 0], [1, 1]]
answers  = [[0],    [1],    [1],    [0]]        // XOR

brain.train(examples, answers, epochs: 3000, optimizer: adam, rate: 0.05)

print(brain.predict([1, 0]))   // close to [1]
print(brain.predict([1, 1]))   // close to [0]
```

**Building one.** `neural_network(inputs:, hidden:, outputs:)`. `hidden` is either one number (a
single hidden layer) or a list of numbers — one per layer — so you choose both how many hidden
layers there are and how big each one is:

```plaintext
big = neural_network(inputs: 4, hidden: [16, 12, 8], outputs: 3)
```

**Training.** `brain.train(examples, answers, ...settings)` runs many rounds. Every setting is
optional:

| setting | what it does | default |
|---|---|---|
| `epochs` | how many passes over the data | 1000 |
| `optimizer` | the update rule: `sgd`, `momentum`, `rmsprop`, or `adam` | `sgd` |
| `rate` | learning rate — how big each step is | per optimizer |
| `decay` | slowly shrinks the rate as training goes on | 0 (off) |

`examples` and `answers` are lists of lists of numbers; they must line up (same count, and each row
the right width for the network's inputs and outputs). You don't have to type them out by hand —
[`examples/dataset.pt`](../examples/dataset.pt) loads a CSV and then scores accuracy on fresh
random points.

**Training on a GPU.** Add `device:` when you build the network to train on a graphics card:

```plaintext
brain = neural_network(inputs: 2, hidden: [16, 12], outputs: 1, device: auto)
```

| `device:` | where it trains |
|---|---|
| `auto` (or `gpu`) | any GPU the machine has |
| `cuda` | an NVIDIA GPU |
| `rocm` | an AMD GPU |
| `mps` | an Apple GPU (Metal) |
| `vulkan` / `dx12` | a specific backend |
| `cpu` | force the CPU |

The same code runs on every vendor — one GPU backend covers NVIDIA, AMD, and Apple. If the chosen
GPU can't be opened (say you ask for `cuda` on a machine with no NVIDIA card), training **falls back
to the CPU and prints a note** rather than failing. On a GPU, training runs in batches (the whole
dataset each epoch) instead of one example at a time, and in single precision (`f32`); the CPU keeps
double precision. GPUs pay off for large networks and datasets — for a tiny network like XOR the CPU
is actually faster, since there's almost no work to hide the setup cost. `train_once` always runs on
the CPU.

**Watching it learn.** `brain.train_once(examples, answers, ...settings)` runs a *single* round and
returns the current error, so training can happen inside a game loop and draw its own progress:

```plaintext
on update(delta) {
    error = brain.train_once(examples, answers, optimizer: adam, rate: 0.05)
}
```

For a live population of agents instead, see [`examples/evolve.pt`](../examples/evolve.pt).

**Using and saving.**

```plaintext
answer = brain.predict([1, 0])            // a list of numbers, one per output
score  = brain.loss(examples, answers)    // current average error, without training
brain.save("brain.ai")                    // later:  brain = load_network("brain.ai")
```

[`examples/learn.pt`](../examples/learn.pt) trains a network, saves it, and loads it straight
back to show the trained brain survives a round-trip.

**Loading a dataset from a file.** Instead of typing examples and answers out by hand, keep them
in a `.csv` file — one row per example, the input columns first and the answer column(s) last —
and load it with `load_dataset(path, outputs: n)`:

```plaintext
data = load_dataset("training.csv", outputs: 1)
brain.train(data[0], data[1], epochs: 500, optimizer: adam)   // data[0] = examples, data[1] = answers
```

A header row of column names is skipped automatically. `read_csv(path)` is the lower-level tool
if you want the raw rows of numbers to arrange yourself.

**Neuroevolution — learning without answers.** Sometimes there is no "right answer" to train on,
only a way to score how well something did — how far a character got, how long it survived. For
that, breed a **population** of networks: score each one, keep the best, and let the winners
produce a slightly-mutated next generation. Over many generations they get better on their own.

```plaintext
brains = population(count: 100, inputs: 4, hidden: [8], outputs: 2)

// ...let every brain control an agent and measure how it did...
scores = [ ... ]                              // one fitness number per brain

champion = best_of(brains, scores)            // the current best (to draw or save)
brains = evolve(brains, scores, mutation: 0.1, keep: 4)   // the next generation
```

| Function | What it does |
|---|---|
| `population(count:, inputs:, hidden:, outputs:)` | a list of `count` fresh random networks (same shape) |
| `evolve(brains, scores, mutation:, keep:)` | next generation: keep the top `keep`, breed + mutate the rest |
| `best_of(brains, scores)` | the single highest-scoring network |

`brains` is an ordinary list — index it with `brains[i]` and call `brains[i].predict(...)` like any
network. Higher scores are better; they can be any numbers you like. `mutation` (default `0.1`) is
how much weights are jiggled each generation, and `keep` (default `1`) is how many top performers
survive unchanged. See [`examples/evolve.pt`](../examples/evolve.pt) — a population of dots that
teach themselves to reach a goal, no training data in sight.

This is a small feed-forward network — made for learning and for little numeric problems, not for
large images or text. Targets train best scaled to the 0–1 range.

> Those `name: value` settings work in any call, not just training — a call can end with labelled
> arguments like `epochs: 3000` for clarity.

---

## 17. The REPL

Run `plaintext repl` for an interactive session. Type an expression to see its value; anything
you define sticks around for later lines. Multi-line blocks (functions, `if`, …) keep reading
until their braces close. Type `exit` to leave.

```text
> 2 + 2
4
> greeting = "hi"
> greeting.upper()
HI
```

---

## 18. Building a standalone app

`plaintext build game.pt` turns your program into a single executable your friends can run
**without installing PlainText** — it bundles your code (and every file it imports) into a copy
of the runtime.

```text
plaintext build game.pt            # → game.exe (Windows) or game (macOS)
plaintext build game.pt -o Game    # choose the output name
plaintext build game.pt --run      # build, then run it to check
```

Assets are packed next to the app so load paths still work when friends run it elsewhere:

- If there's an `assets/` folder next to your `.pt` file, that folder is copied wholesale.
- Every literal path in `load_sprite` / `load_sprite_sheet` / `load_sound` / `load_music` /
  `load_font` is copied too, keeping the same relative path (e.g. `examples/assets/walk.png`).

**Making a Mac app (from any computer).** Building doesn't recompile — it just appends your
program to a runtime binary — so you can build a Mac app from Windows by pointing at a macOS
`plaintext` binary (the one from the macOS release zip):

```text
plaintext build game.pt --runtime ./plaintext-macos-arm64 -o Game
```

On the Mac the first run may need `chmod +x Game`, and if Gatekeeper complains,
`xattr -dr com.apple.quarantine Game`. On Windows, SmartScreen may ask once — choose
**More info → Run anyway**. (These prompts appear because the app isn't code-signed; signing is a
separate, paid step.)

---

## 19. Appendix: when do I have to write a type?

Almost never. You only need an explicit type when PlainText genuinely can't infer one:

1. An **empty list** — `scores: Number list = []` (nothing to infer the element type from).
2. A bare **`nothing`** — `nickname: Text? = nothing`.
3. **Function parameters** with no default (or leave it off to make the parameter flexible).
4. **Class fields** with no default value.

Everything else — variables, return types, non-empty collections, parameters with defaults —
is inferred.
