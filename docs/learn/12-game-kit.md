# 12. Game kit

**Goal:** Build a platformer-style game with gravity, solid ground, and attack/hurt
hitboxes — without hand-rolling rectangle math every frame.

Turn it on with `import gamekit`. The usual `draw_*` / `key_*` tools still work.

## Bodies and gravity

```plaintext
import gamekit

world = physics_world(gravity: 1800)   // pixels per second², downward

hero = body(x: 100, y: 100, width: 28, height: 40, solid: true)
ground = body(x: 0, y: 560, width: 800, height: 40, solid: true, static: true)

world.add(hero)
world.add(ground)

// each frame:
world.step(delta)   // applies gravity, moves bodies, resolves solids
```

- **Dynamic** bodies (default) fall and slide.
- **`static: true`** bodies are platforms/walls — they block others but do not move.
- After `step`, `hero.on_ground` is true when you are standing on something solid.
- Jump with `hero.jump(700)` (only works on the ground unless you pass `true` to force it).

Units are **pixels** and **seconds** (use the `delta` from `on update`).

## Hitboxes

A hitbox is a rectangle glued to a body (or free-floating). Tags are ordinary text:

```plaintext
hurt = hitbox(owner: hero, offset_x: 2, offset_y: 4, width: 24, height: 34, kind: "hurt")
attack = hitbox(owner: hero, offset_x: 22, offset_y: 10, width: 26, height: 18, kind: "attack", active: false)

world.add(hurt)
world.add(attack)

attack.active = key_down("z")          // only “on” while swinging
if world.hits(attack, enemy_hurt) {    // true once per swing
    // deal damage
}
if overlaps(hurt, enemy_hurt) {
    // continuous overlap (touch damage, pickups, …)
}
```

`world.hits` fires **once** while the attack stays active and overlapping; it can fire again
after `attack.active` becomes false. Use `overlaps` when you want every frame.

## Drawing helpers

```plaintext
draw_body(hero, blue)
draw_hitbox(attack, orange)
draw_hitboxes(world)        // outlines every active hitbox (debug)
```

## Tilemaps

A level can be a grid of characters — easy to sketch in your `.pt` file:

```plaintext
level = tilemap(cell_size: 40, rows: [
    "##########",
    "#........#",
    "#..P.....#",
    "#....##..#",
    "##########",
])

world.add_tilemap(level, solid_tiles: ["#"])   // '#' blocks solid bodies
```

- `tile_at(level, col, row)` (or `level.tile_at(col, row)`) returns that character, or
  `nothing` if you ask outside the map. Columns and rows are integers from the top-left.
- Fields: `cell_size`, `width` (columns), `height` (rows).
- Draw with colors (no image needed):  
  `draw_tilemap(level, tile_colors: dictionary { "#": gray, ".": darkgray, "P": green })`  
  Characters missing from the dictionary are skipped.
- You can also put `solid_tiles: ["#"]` on `tilemap(...)` itself, then `world.add(level)`.

Solid tiles collide the same way static bodies do: land on floors, bump into walls.

### Scenes (menu ↔ play) without a framework

A “scene” is just which update/draw code runs. Keep a text variable and branch:

```plaintext
screen = "menu"

// in on update(delta):
if screen is "menu" {
    if pressed("jump") { screen = "play" /* reset hero, etc. */ }
} else if screen is "play" {
    world.step(delta)
    if key_pressed("escape") { screen = "menu" }
}
```

No scene graph or state machine is built in — this pattern is enough for a menu and a level.

### Painting a level (`plaintext edit_tilemap`)

```bash
plaintext edit_tilemap examples/tilemap.pt
```

Opens a window where you paint the map, toggle solids (**S**), and drop a **PNG** onto the
window to assign it to the selected character. Resize the grid with the **+ / −** buttons for
**Columns** and **Rows** in the left panel. **Pan** with middle-mouse drag (or Space + drag),
**zoom** with the mouse wheel, and nudge with the arrow keys. **Save** rewrites the `.pt` file
in place:

- updates that map’s `rows` (and `solid_tiles`, wherever they already live)
- creates/updates a nearby `level_tiles = dictionary { "#": "path.png", ... }`  
  (name is `<map>_tiles`)

It does **not** edit `body` / `hitbox` literals yet. First save also writes `file.pt.bak`.

## Input helper

`pressed("jump")` is like `key_pressed`, with friendly aliases (`jump` → space/up/w,
`attack` → z/j/space, …).

## Practice

| Example | Idea |
|---------|------|
| [`platformer.pt`](../../examples/platformer.pt) | Run, jump, stomp / attack — hold **H** to outline hitboxes |
| [`tilemap.pt`](../../examples/tilemap.pt) | Text-row level, solid tiles, menu/play `screen` switch |

```bash
plaintext run examples/platformer.pt
plaintext run examples/tilemap.pt
```

## Common mistakes

| Mistake | Fix |
|---------|-----|
| `physics_world` needs gamekit | Add `import gamekit` |
| Character falls through the floor | Make the floor `solid: true, static: true` and `world.add` both bodies — or use a tilemap with `solid_tiles` |
| Attack hits every frame | Use `world.hits`, not bare `overlaps`, for swings |
| Forgetting `world.step(delta)` | Nothing moves without it |
| Tilemap has no collision | Call `world.add_tilemap(map, solid_tiles: ["#"])` (or set `solid_tiles` on the map) |

## Out of scope (for now)

No slopes, rotation, joints, or **animated / tileset-drawn maps** — collision is still
axis-aligned boxes and solid character tiles. Character sprite sheets (`load_sprite_sheet` /
`draw_frame`) are already in [lesson 10](10-games.md); what’s missing here is drawing the
*tilemap itself* from a sheet instead of `tile_colors`. The `edit_tilemap` painter covers tile
layout and per-character PNG assignment, but not `body`/`hitbox` placement. Color
`draw_tilemap` is the debug/teachable path; image tilesets can come later.

**Previous:** [Desktop UI ←](11-ui.md) · **Next:** [Neural networks →](13-neural-networks.md)
