# The PlainText Language Reference

PlainText is a small language designed to read like plain English. This guide teaches it from
the ground up. It assumes you can run programs with `plaintext run yourfile.pt`.

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
print("You have " + count)      // error: can't add Text and Number
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

Comparisons: `==`, `!=`, `<`, `<=`, `>`, `>=`.

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
`index_of`, `remove_at`, `reversed`, `join`.

An empty list has no way to guess its element type, so annotate it:

```plaintext
names: Text list = []
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

### Input

- `key_down(name)` — held this frame. `key_pressed(name)` — pressed just now. Names: `"up"`,
  `"down"`, `"left"`, `"right"`, `"space"`, `"enter"`, `"escape"`, letters like `"w"`.
- `mouse_x()`, `mouse_y()`, `mouse_down()`, `mouse_pressed()`.
- `screen_width()`, `screen_height()`.

### Sound

```plaintext
beep = load_sound("beep.wav")
play_sound(beep)
```

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
- `text "..."` — a label (interpolation works).
- `button "..."` — clickable; give it `on_click: <a function>`.
- `spacer` — empty space (`width` / `height`).

### Properties

Written in parentheses as `name: value`:

- `padding` — space inside a column/row.
- `spacing` — gap between children.
- `align` — `center`, `left`, `right`, `top`, or `bottom` (cross-axis alignment).
- `size` — font size for `text` / button labels.
- `width`, `height` — a fixed size (handy for buttons).
- `color`, `bg` — text color / background (a named color or `rgb(...)`).
- `font` — a font from `load_font("path.ttf")`.
- `sprite` — a sprite from `load_sprite("path.png")` used as a button face.
- `on_click` — for buttons, a function to run.

The window itself also accepts `bg` / `background` for the clear color:

```plaintext
ui_font = load_font("C:/Windows/Fonts/segoeui.ttf")  // Windows; on Mac try /System/Library/Fonts/Supplemental/Arial Unicode.ttf
btn = load_sprite("button.png")

window "Demo" (width: 480, height: 300, bg: rgb(24, 28, 40)) {
    text "Hello" (size: 28, color: white, font: ui_font)
    button "Go" (on_click: go, sprite: btn, width: 140, height: 52, color: white)
}

```

---

## 12. The standard library at a glance

**Math**: `min`, `greatest`, `abs`, `sqrt`, `floor`, `ceil`, `round`, `pow`, `clamp`, `sin`, `cos`,
`tan`, `random_between(lo, hi)`, and the constants `pi` and `e`.

**Convert**: `to_text(x)`, `to_number(text)`, `length(x)`.

**Files**: `read_file(path)`, `write_file(path, text)`, `append_file(path, text)`,
`file_exists(path)`.

**Time**: `now()` (seconds since 1970), `clock()` (seconds since the program started).

**Output**: `print(...)`.

---

## Appendix: when do I have to write a type?

Almost never. You only need an explicit type when PlainText genuinely can't infer one:

1. An **empty list** — `scores: Number list = []` (nothing to infer the element type from).
2. A bare **`nothing`** — `nickname: Text? = nothing`.
3. **Function parameters** with no default (or leave it off to make the parameter flexible).
4. **Class fields** with no default value.

Everything else — variables, return types, non-empty collections, parameters with defaults —
is inferred.
