# Examples

Runnable PlainText programs. From the repo root:

```bash
plaintext run examples/basics.pt
plaintext check examples/basics.pt
```

Paths like `examples/assets/...` and `examples/data/...` are relative to **where you run
`plaintext`**, so stay at the repo root.

## Language

| File | What it shows |
|------|---------------|
| [`basics.pt`](basics.pt) | Variables, functions, classes, loops, optionals, Dynamic parameters |
| [`ask.pt`](ask.pt) | `input()` from the console |
| [`stdlib.pt`](stdlib.pt) | `import math`, list / text / dictionary methods, time |
| [`list_tools.pt`](list_tools.pt) | Multi-file `import`, `sorted` / `transformed_by` / `kept_if` / `combined`, `exit` |
| [`toolbox.pt`](toolbox.pt) | Helper module imported by `list_tools.pt` (not run alone) |
| [`timers.pt`](timers.pt) | `after` / `every` (runs until you stop it) |
| [`save.pt`](save.pt) | `save` / `load` / `has_save` — remembers a high score across runs |

## Games & UI

| File | What it shows |
|------|---------------|
| [`catch.pt`](catch.pt) | Complete arcade loop — score, lives, restart |
| [`platformer.pt`](platformer.pt) | `import gamekit` — gravity, platforms, attack hitboxes (hold **H**) |
| [`enemies.pt`](enemies.pt) | Spawn several enemy bodies in a list, `world.remove` each on death |
| [`tilemap.pt`](tilemap.pt) | Text-row tilemap, solid tiles, menu ↔ play via a `screen` variable (`plaintext edit_tilemap` to paint) |
| [`counter.pt`](counter.pt) | Desktop UI — buttons and a live label |
| [`form.pt`](form.pt) | Settings form — `text_field`, `checkbox`, `slider`, `image`, `bind:` |
| [`scroll_list.pt`](scroll_list.pt) | `scroll`, `list`, `dropdown`, multiline `text_field`, Tab focus |
| [`audio.pt`](audio.pt) | Sound effects, looping SFX, streamed music, volume / pitch / pan / fade |
| [`camera_sheets.pt`](camera_sheets.pt) | `set_camera` / `center_camera`, `load_sprite_sheet` + `draw_frame`, HUD |

Sprites and sounds are covered in [lesson 10](../docs/learn/10-games.md); `counter.pt` /
`form.pt` load button images from [`assets/`](assets/); `audio.pt` / `camera_sheets.pt` use
assets there too.

## Web & AI

| File | What it shows |
|------|---------------|
| [`fetch.pt`](fetch.pt) | `import web` — JSON offline (+ live URL in comments) |
| [`learn.pt`](learn.pt) | `import ai` — XOR, `device: auto`, save / `load_network` |
| [`dataset.pt`](dataset.pt) | Train from CSV + accuracy on new points |
| [`image_dataset.pt`](image_dataset.pt) | Train from a folder of labeled images |
| [`evolve.pt`](evolve.pt) | Neuroevolution — agents learn to play in a window |

Supporting data: [`fixtures/sample.json`](fixtures/sample.json), [`data/circle.csv`](data/circle.csv),
[`data/shapes/`](data/shapes/).
