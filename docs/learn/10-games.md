# 10. Games

A `game` block opens a window and runs a loop:

```plaintext
import math

game "My Game" (width: 800, height: 600) {
    on update(delta) {
        // delta = seconds since last frame
    }
    on draw() {
        clear_screen(skyblue)
        draw_circle(400, 300, 20, red)
    }
}
```

Hooks: `on start()`, `on update(delta)`, `on draw()`.

Drawing, sprites, keys, mouse, sound, and timers (`after` / `every`) are all available
inside a game. Multiply motion by `delta` so it stays smooth.

## Try it

| Example | Idea |
|---------|------|
| [`bounce.pt`](../../examples/bounce.pt) | Physics + arrow keys |
| [`sprites.pt`](../../examples/sprites.pt) | Images |
| [`spawner.pt`](../../examples/spawner.pt) | Timers spawning objects |
| [`catch.pt`](../../examples/catch.pt) | Full arcade loop (score, lives, restart) |

**Next:** [Desktop UI →](11-ui.md)
