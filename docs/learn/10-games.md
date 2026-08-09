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

## Practice — play these in order

| Example | Idea |
|---------|------|
| [`bounce.pt`](../../examples/bounce.pt) | Physics + arrow keys |
| [`sprites.pt`](../../examples/sprites.pt) | Images |
| [`timers.pt`](../../examples/timers.pt) | `after` / `every` |
| [`spawner.pt`](../../examples/spawner.pt) | Timers spawning objects |
| [`catch.pt`](../../examples/catch.pt) | Full arcade loop (score, lives, restart) |

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
