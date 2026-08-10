# 10. Games

**Goal:** Open a window, update state each frame, and draw.

## The `game` block

```plaintext
import math

x = 400
y = 300

game "My Game" (width: 800, height: 600) {
    on update(delta) {
        // delta = seconds since the last frame (~0.016 at 60 FPS)
        if key_down("right") {
            increase x by 200 * delta
        }
    }
    on draw() {
        clear_screen(skyblue)
        draw_circle(x, y, 20, red)
    }
}
```

Hooks you will use:

| Hook | When it runs |
|------|----------------|
| `on start()` | Once, when the game begins |
| `on update(delta)` | Every frame — change state here |
| `on draw()` | Every frame — draw only; don’t invent new state here |

**Always multiply motion by `delta`** so the game feels the same on fast and slow machines.

## Drawing, input, sprites, timers

Inside a game you can:

- Draw shapes and text (`draw_circle`, `draw_rectangle`, `draw_text`, …)
- Read keys and mouse (`key_down`, `key_pressed`, `mouse_x`, …)
- Load and draw images (`load_sprite`, `draw_sprite`, …)
- Schedule work with `after(seconds, fn)` and `every(seconds, fn)`

Need random numbers? `import math` and use `random_between`.

## Images & sound

Load an image once (in the init area, not every frame), then draw it:

```plaintext
game "Sprites" (width: 800, height: 600) {
    ship = load_sprite("assets/ship.png")     // load once
    beep = load_sound("assets/beep.wav")

    on draw() {
        clear_screen(black)
        draw_sprite(ship, 100, 100)               // native size
        draw_sprite_scaled(ship, 300, 100, 2)     // 2× bigger
        draw_sprite_rotated(ship, 500, 300, 45)   // rotated 45°, about its center
    }
}
```

Play a one-shot with `play_sound(beep)`, or loop a sound effect with
`play_sound(hum, loop: true)` / `stop_sound(hum)`.

For longer background tracks use **music** (streamed, separate ids from sounds):

```plaintext
tune = load_music("assets/theme.ogg")
play_music(tune)              // loops by default
set_music_volume(tune, 0.6)   // 0..1
fade_music(tune, 0, 1.5)      // fade out over 1.5 seconds
stop_music(tune)
```

Also: `set_sound_volume` / `set_sound_pitch` / `set_sound_pan`, and the matching
`set_music_volume` / `set_music_pitch` / `set_music_pan`. Sound ids and music ids do not share a
namespace — keep them in different variables.

Sizes come from `sprite_width(ship)` / `sprite_height(ship)`.

**Where do the files go?** Paths are relative to the folder you run `plaintext` **from**, not the
`.pt` file's folder. Keep images in an `assets/` folder next to your program and run from there.
PNG images, WAV/OGG audio, and TTF fonts are supported.

## Practice — play these

| Example | Idea |
|---------|------|
| [`timers.pt`](../../examples/timers.pt) | `after` / `every` |
| [`audio.pt`](../../examples/audio.pt) | Sounds, looping SFX, streamed music, volume / fade |
| [`catch.pt`](../../examples/catch.pt) | Full arcade loop (score, lives, restart) |
| [`platformer.pt`](../../examples/platformer.pt) | Gravity + hitboxes via `import gamekit` (lesson 12) |
| [`tilemap.pt`](../../examples/tilemap.pt) | Text-row levels + menu/play screens (lesson 12) |

Sprites (`load_sprite`, `draw_sprite_rotated`, …) are in the section above; UI examples
[`counter.pt`](../../examples/counter.pt) and [`form.pt`](../../examples/form.pt) also load images.

```bash
plaintext run examples/catch.pt
```

## Common mistakes

| Mistake | Fix |
|---------|-----|
| Moving with `x = x + 5` each frame | Use `5 * delta` (or a speed constant × `delta`) |
| Forgetting `import math` in a game that uses random | Add it at the top |
| Asset path not found | Run from the repo root so `examples/assets/...` resolves |

**Previous:** [Modules ←](09-modules.md) · **Next:** [Desktop UI →](11-ui.md)
