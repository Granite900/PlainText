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

## Input helper

`pressed("jump")` is like `key_pressed`, with friendly aliases (`jump` → space/up/w,
`attack` → z/j/space, …).

## Practice

| Example | Idea |
|---------|------|
| [`platformer.pt`](../../examples/platformer.pt) | Run, jump, stomp / attack — hold **H** to outline hitboxes |

```bash
plaintext run examples/platformer.pt
```

## Common mistakes

| Mistake | Fix |
|---------|-----|
| `physics_world` needs gamekit | Add `import gamekit` |
| Character falls through the floor | Make the floor `solid: true, static: true` and `world.add` both bodies |
| Attack hits every frame | Use `world.hits`, not bare `overlaps`, for swings |
| Forgetting `world.step(delta)` | Nothing moves without it |

## Out of scope (for now)

No slopes, rotation, joints, or a full physics engine — axis-aligned boxes only.

**Previous:** [Desktop UI ←](11-ui.md) · **Next:** [Neural networks →](13-neural-networks.md)
