# Prompt: PlainText next milestone — LSP, UI, lambdas, web, Linux, game kit

Copy everything below the line into a coding agent working in the PlainText repo.

---

You are working in the **PlainText** repo (Rust tree-walking interpreter + Raylib + optional wgpu). Current version is around **2.2.0**. The language already has: type checker, GC, `import math` / `import ai`, `game` / `window` blocks, `plaintext build` bundling, Win + macOS-arm64 CI releases, VS Code **syntax highlighting only**.

## Product rules (non-negotiable)

1. **Readable like English.** Prefer words over symbols. New APIs should sound teachable in a 14-year-old’s lesson.
2. **Small vertical slices.** Ship working examples + docs for each feature; do not half-parse syntax that errors at runtime.
3. **Match existing style.** Follow `src/parser.rs`, `checker.rs`, `interpreter.rs`, `value.rs`, module pattern like `import math` / `import ai`, and docs in `docs/learn/` + `docs/language-reference.md`.
4. **Do not** add PyTorch-scale ML, a bytecode VM, Marketplace publishing automation, or inheritance/enums unless required by a feature below.
5. Update **VS Code TextMate grammar**, **README examples table**, **GETTING-STARTED** where relevant, and bump the crate/extension version appropriately when shipping.
6. Keep Windows + macOS CI green; add Linux as specified below.
7. Prefer extending builtins/modules over inventing cryptic new statement forms — except where syntax is the point (lambdas).
8. You WILL STOP after EVERY feature is added, you will NOT continue until I say you may. Continue with the suggested order as specified bellow.

---



## Milestone goals (implement all six tracks)



### Track A — VS Code language server (LSP)

**Goal:** Editing `.pt` files shows real errors and basic IDE help, not just colors.

**Deliver:**

- A PlainText language server (Rust is fine: e.g. `tower-lsp`, or a small JSON-RPC server the extension spawns).
- VS Code extension (`editors/vscode/`) starts the server and wires:
  - **Diagnostics** from the existing type checker / parse errors (same messages as `plaintext check`)
  - **Hover** (type of name, short builtin docs if easy)
  - **Go to definition** for functions/classes/locals in the current file (multi-file nice-to-have)
  - **Completions** for keywords + builtins + in-scope names (good enough, not perfect)
- CLI optional: `plaintext lsp` (stdio) so the extension can launch `plaintext lsp`.
- Document install: still `.vsix` from release / local package; explain that LSP needs the `plaintext` binary on PATH.

**Acceptance:** Open a broken `.pt` file in VS Code → red squiggle with the same hint-quality as CLI. Fix it → diagnostics clear.

---



### Track B — Richer desktop UI

**Goal:** `window` apps can be real small tools, not only counters.

**Add widgets** (names can be adjusted for English feel, but keep them short):


| Widget        | Behavior                                          |
| ------------- | ------------------------------------------------- |
| `text_field`  | Editable text; bind via a variable or `on_change` |
| `checkbox`    | Boolean on/off                                    |
| `slider`      | Number in a range (`min`, `max`, `step`)          |
| `image`       | Show a loaded sprite                              |
| Keep existing | `column`, `row`, `text`, `button`, `spacer`       |


**Requirements:**

- State stays in PlainText variables; window still redraws each frame (current model).
- Type-check widget props; clear errors for bad `on_click` / missing handlers.
- Example: `examples/form.pt` (name field + checkbox + slider + submit button that `print`s values).
- Docs: update lesson 11 + language reference UI section.

**Acceptance:** A beginner can build a tiny settings/form UI without dropping to game-canvas hacks.

---



### Track C — Inline / anonymous functions

**Goal:** Pass short functions to `transformed_by`, `kept_if`, `combined`, `on_click`, timers, etc. without always writing `make function called …`.

**Suggested surface** (prefer the most English-readable option that still parses cleanly):

```plaintext
// Prefer something like this — adjust spelling to fit the lexer/parser cleanly:
nums.transformed_by(make function (n: Number) { return n * 2 })

// Or a tighter form if the long form is too noisy:
nums.kept_if(function (n: Number) { return n is more than 0 })
```

**Requirements:**

- Closures capture enclosing locals (you already have function objects with env — extend that).
- Works in checker (infer param/return types where possible; require param types if inference is too hard for v1).
- Update examples that currently invent one-off named helpers only for map/filter.
- Grammar + learn docs (collections / functions lessons).
- **No** JS arrow-forest aesthetics (`=>` chains everywhere). Keep it PlainText.

**Acceptance:** `examples/list_tools.pt` (or a new example) uses inline functions and still reads aloud well.

---



### Track D — HTTP + JSON (`import web` or `import data`)

**Goal:** Tiny internet + data toolkit for demos and tools.

**Suggested API:**

```plaintext
import web

page = web.get("https://example.com")              // Text body (or a result object)
data = web.get_json("https://api.example.com/…")   // dictionary / list
web.post_json(url, dictionary { "a": 1 })

text = to_json(dictionary { "name": "Ada" })
value = parse_json("{\"x\": 1}")
```

**Requirements:**

- Use a small Rust HTTP client (e.g. `ureq` or `reqwest` blocking) — keep it simple; no async runtime required in PlainText.
- Timeouts + clear PlainText diagnostics on network failure (readable messages).
- JSON ↔ PlainText `dictionary` / `list` / numbers / text / booleans / `nothing`.
- Example: `examples/fetch.pt` (hit a public JSON API or a local fixture if CI needs offline — prefer offline fixture in CI + live URL in comments).
- Docs lesson or reference section; cheatsheet line.
- Security note in docs: this can talk to the network; don’t pretend otherwise.

**Acceptance:** `plaintext run examples/fetch.pt` works offline in CI (fixture) and documents how to call a live API.

---



### Track E — Linux releases

**Goal:** Official Linux x64 (and optionally aarch64 if easy) artifacts beside Windows + macOS.

**Deliver:**

- Add `ubuntu-latest` (or similar) to `.github/workflows/ci.yml` and `release.yml`.
- Install whatever Raylib/`libclang` deps Linux needs; document in GETTING-STARTED.
- Artifact name like `plaintext-linux-x64.zip` with same layout as other zips (binary, docs, examples, START-HERE).
- Update README platform badge + Getting Started download table.
- Confirm `plaintext build` on Linux produces a runnable Linux bundle.

**Acceptance:** Tag/release workflow uploads a Linux zip; CI builds + checks examples on Linux.

---



### Track F — Game maker kit (BIG — treat as a first-class subsystem)

**Goal:** Make 2D action/platform games dramatically easier. Today users hand-roll positions, `delta`, and circle overlaps. Add a **readable game kit** that covers gravity, bodies, hitboxes, and combat overlaps — without becoming Unity.

Prefer a module such as `import gamekit` (name bikeshed OK: `arcade`, `play`) so the raw `draw_*` / `key_*` APIs remain available.

#### Design principles for the kit

- Plain English method names and options.
- Frame-friendly: integrate with existing `game { on update(delta) / on draw() }`.
- Composition over deep class inheritance (PlainText has no inheritance).
- Sensible defaults (gravity strength, box sizes) with overrides.
- Debuggable: optional draw hitboxes (`draw_hitboxes` or `debug: true`).



#### Minimum viable kit API (implement at least this)

**World / physics step**

```plaintext
import gamekit

world = physics_world(gravity: 1800)    // downward accel in px/s²

hero = body(
    x: 100, y: 100,
    width: 32, height: 48,
    solid: true          // collides with solid bodies / tiles if you add them
)

world.add(hero)
world.step(delta)        // applies gravity + velocity + resolves simple collisions
```

**Bodies**

- Position, size, velocity (`vx`, `vy`), acceleration optional
- `on_ground` (boolean) after step when standing on something solid
- Helpers: `hero.move(dx, dy)`, `hero.set_velocity(vx, vy)`, `hero.bump(vx, vy)` / `hero.jump(speed)`
- `solid` vs trigger (triggers overlap but don’t block)

**Hitboxes**

```plaintext
hurt = hitbox(owner: hero, offset_x: 0, offset_y: 0, width: 32, height: 48, kind: "hurt")
attack = hitbox(owner: hero, offset_x: 24, offset_y: 8, width: 28, height: 20, kind: "attack", active: false)

// each frame:
attack.active = key_down("space")
world.sync_hitboxes()   // or automatic in world.step

if overlaps(attack, enemy_hurt) {
    // deal damage once per swing — provide a short "already hit" helper if needed
}
```

Provide:

- `overlaps(a, b)` / `a.overlaps(b)`
- Hitboxes follow owner body automatically
- Optional lifetime / `active` flag for attack windows
- Kinds as text tags (`"hurt"`, `"attack"`, `"pickup"`) so beginners can filter

**Camera / drawing helpers (lightweight)**

- `draw_body(body, color)` or draw sprite centered on body
- `draw_hitbox(hitbox, color)` for debug
- Screen scroll helper optional: `camera_follow(hero, …)`

**Input helpers (optional but valuable)**

```plaintext
if pressed("jump") { hero.jump(700) }   // edge-trigger wrapper over key_pressed
```

**Platform / solids (pick one approach for v1)**

Either:

1. Axis-aligned solid bodies as platforms, or
2. A simple tilemap (`tiles` list + `cell_size`) that bodies collide with

Do **one** well. Document it. Platforms + gravity + jump is the killer demo.

#### Examples to ship

1. `examples/platformer.pt` — run, jump, gravity, solid ground, one moving enemy, stomp or touch damage, simple attack hitbox, lives/score text. Should be shorter and clearer than doing it with raw `draw_rectangle` math.
2. `examples/hitbox_lab.pt` — minimal scene that draws hitboxes so users *see* attack vs hurt overlaps.
3. Update `examples/catch.pt` only if it cleanly benefits; don’t force a rewrite.



#### Docs

- New learn lesson **12 — Game kit** (after games lesson): gravity, bodies, hitboxes, one combat pattern.
- Language reference section for `import gamekit`.
- Cheatsheet entries.
- The NN Lesson should be after the Game kit one, being 13



#### Implementation notes

- Implement kit logic in Rust (`src/gamekit.rs` or similar) exposed as builtins/module values — don’t ask users to copy a huge `.pt` library for core physics.
- Use Raylib only for input/draw as today; physics can be simple AABB (no full Box2D required for v1).
- Stay numerically stable with `delta`; document units (pixels, seconds).
- If something is out of scope for v1 (slope physics, rotation, joints), say so in docs.

**Acceptance:** A new user can clone `platformer.pt`, change gravity/jump, add a second enemy with a hurtbox, and understand overlaps by reading the code aloud.

---



## Suggested implementation order

1. **Track C (lambdas)** — unlocks cleaner UI handlers + list tools + game callbacks
2. **Track B (UI widgets)** — depends happily on C for `on_change`
3. **Track F (game kit)** — biggest user-facing win for the “games language” brand
4. **Track D (web/json)** — self-contained module
5. **Track A (LSP)** — tooling; can parallelize with others if two agents
6. **Track E (Linux)** — packaging/CI last so it includes the new modules

If you must ship one PR series: **C → B → F**, then D, A, E.

---



## Testing / quality bar

- `cargo build` / `cargo test` if you add tests; otherwise all `examples/*.pt` must `plaintext check`.
- CI: run a non-GUI smoke subset + `learn.pt`-style checks; for game/UI examples at least `check`.
- Add/adjust VS Code grammar for new keywords/builtins (`text_field`, `physics_world`, `hitbox`, `parse_json`, …).
- No secrets in repo. No network required for CI green.
- Update version to a sensible bump (e.g. **2.3.0** or **3.0.0** if gamekit is marketed as major).

---



## Out of scope for this milestone

- Full Unity/Godot editor
- 3D
- Multiplayer networking
- Softbody / realistic physics engine
- Publishing to VS Code Marketplace (local/`vsix` is enough)
- Removing or rewriting `import ai`

---



## When finished

1. Summarize what shipped per track with example paths.
2. Note any API deviations from this prompt and why.
3. List follow-ups explicitly deferred (e.g. tilemap editor, LSP rename).
4. Ensure README “Why you might like it” mentions the game kit + LSP if they landed.
5. Update version to 2.9 and commit but ONLY when you are done with all six letters.

Work in the existing architecture. Prefer boring, readable APIs over clever ones.