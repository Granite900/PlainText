# PlainText next milestone — UI toolkit depth, audio depth, tilemaps + scenes

Copy everything below the line into a coding agent working in the PlainText repo.

---

You are working in the PlainText repo (Rust tree-walking interpreter + Raylib, optional wgpu
for `import ai` GPU training). Current version is 2.8.1. The language already has: type checker,
GC, `import math` / `import ai` / `import gamekit` / `import web`, `game { }` / `window { }`
blocks, inline/anonymous functions (`make function (params) { }`), a `save`/`load`/`has_save`
persistence system, a VS Code extension with an LSP (`plaintext lsp`), and CI/release builds for
Windows + macOS (arm64 + Intel) + Linux.

## Product rules (non-negotiable — copied from the standing project rules, still in force)

1. **Readable like English.** Prefer words over symbols. New APIs should sound teachable in a
   14-year-old's lesson. Match the existing naming voice (`text_field`, `physics_world`,
   `has_save`, `key_down("space")`, `on_click`, `bind:`) — not `scrollView` / `AudioSource`.
2. **Small vertical slices.** Every feature ships with a working example + docs. Don't half-parse
   syntax that errors at runtime. Don't ship a builtin with no example exercising it.
3. **Match existing style.** Follow the patterns in `src/parser.rs`, `checker.rs`,
   `interpreter.rs`, `value.rs`, `ui.rs`, `gamekit.rs`, `gfx.rs`. See "Architecture notes" below
   for exactly how a builtin/module is wired end-to-end in this codebase — do not invent a
   different plumbing pattern.
4. **Don't scope-creep.** No visual/drag-and-drop editor, no bytecode VM, no PyTorch-scale ML, no
   Marketplace publishing automation, no inheritance/enums, no 3D, no networking multiplayer.
   Stay inside the three features below.
5. **Update everything that needs updating when you ship a feature:** the VS Code TextMate
   grammar (`editors/vscode/syntaxes/plaintext.tmLanguage.json`), `docs/language-reference.md`,
   `docs/cheatsheet.md`, the relevant `docs/learn/*.md` lesson, and the README's example table if
   you add a new top-level example file.
6. **Keep Windows + macOS + Linux CI green.** Don't add a dependency that only builds on one
   platform without checking `.github/workflows/ci.yml` and `release.yml` still pass conceptually
   (no new external services, no non-portable APIs).
7. **You MUST STOP after every one of the three numbered features below is completed.** Do not
   continue to the next one until told to proceed. This is the most important rule in this
   prompt — a completed feature includes its example, docs, and grammar updates, verified
   end-to-end, before you stop.
8. **Do not commit or push.** Leave every feature's changes uncommitted in the working tree
   unless explicitly told otherwise — the user reviews and commits themselves.
9. **Verify, don't assume.** This project's interpreter/checker/game runner can all be run
   headlessly (`plaintext check <file>`, `plaintext run <file>` for console programs, `cargo
   test`). GUI/window features (anything needing an actual Raylib window) can only be verified by
   compiling cleanly, type-checking the example, and reasoning carefully through the code — say
   so explicitly rather than claiming you "tested" a click or a drag you couldn't actually drive.

## Architecture notes (read before writing code)

- **Three-way builtin wiring.** Every builtin function/module follows the same pattern across
  three files:
  - `src/value.rs` — add a `Builtin::Foo` variant, plus its `.name()` string and `from_name()`
    parse arm. If the builtin belongs to a gated module, add it to that module's `is_x()` check
    (see `is_ai`, `is_gamekit`, `is_web`).
  - `src/checker.rs` — add its return type in `builtin_return()`, and if it's module-gated, the
    same `imports.contains("...")` guard pattern used for `ai`/`gamekit`/`web`.
  - `src/interpreter.rs` — add the `Builtin::Foo => { ... }` arm in `call_builtin`, using
    `self.expect_arity(...)`, `self.as_number/as_text/as_color(...)` helpers already present.
  - A new module (like `gamekit`/`web`) gets a `pub fn install(globals: &Env)` that declares its
    namespace object, gated the same way `import ai`/`import gamekit`/`import web` are today
    (see `process_imports` in `interpreter.rs` and the `Stmt::Import` arm in `checker.rs`).
- **The UI system (`src/ui.rs` + widget building in `interpreter.rs` + rendering in
  `src/game.rs::run_window`)** is Raylib-free by design: `ui.rs` does layout/measurement and
  emits `DrawCmd`s (defined in `src/gfx.rs`) plus `Control`s (interactive regions); only
  `game.rs` touches actual Raylib types. **Preserve this boundary** — new widgets get a `UiKind`
  variant, sizing in `measure()`, positioning in `position()`, and drawing in `draw_node()`, all
  in `ui.rs`; only input *handling* (mouse/keyboard polling) belongs in `game.rs::run_window`.
- **Widgets are built fresh every frame** (immediate-mode redraw driven by ordinary PlainText
  variables via `bind:` / `on_change`, wired in `interpreter.rs::build_widget` /
  `apply_widget_prop`). Any new interactive widget needs: a `UiKind` variant, a `ControlKind`
  variant, prop handling in `apply_widget_prop`, and a case in the input-dispatch match inside
  `run_window` in `game.rs`. The checker validates `bind:`/`value:` types against widget kind in
  `check_widget_value_ty` in `checker.rs` — extend that table for any new bindable widget.
- **`gamekit` (`src/gamekit.rs`)** already has `physics_world`, `body`, `hitbox`, `overlaps`,
  `pressed`, `draw_body`, `draw_hitbox(es)`. It has unit tests (`cargo test`) exercising gravity
  and hit-once-per-swing logic in pure Rust, no Raylib needed — **follow this pattern**: put
  tilemap collision/query logic in plain Rust functions that are unit-testable without a window.
- **Audio today** is two builtins, `load_sound`/`play_sound`, wired through `GfxBridge`
  (`src/gfx.rs`: `sound_loads`/`sound_plays` queues) and fulfilled in `game.rs::load_pending` /
  the per-frame sound-play drain. Raylib's `raylib::core::audio` module (already a dependency)
  exposes `Music` streaming, and `Sound::set_volume/set_pitch/set_pan` — check what the installed
  `raylib` crate version (`Cargo.toml`, currently raylib 5.5) actually exposes before assuming an
  API shape; read `~/.cargo/registry/src/*/raylib-5.5.1/src/core/audio.rs` if unsure.
- **Grammar file** (`editors/vscode/syntaxes/plaintext.tmLanguage.json`) has one big
  `support.function.builtin.pt` regex alternation — append new builtin names to it, don't
  restructure it.
- **Examples convention:** each new top-level feature gets one `examples/<name>.pt` file with a
  comment header explaining what it demonstrates and how to run it
  (`plaintext run examples/<name>.pt`), matching the style of `examples/form.pt` and
  `examples/save.pt`.
- **Docs convention:** `docs/language-reference.md` has a numbered section per topic with a
  Contents list at the top — add a subsection in the relevant existing section (UI → §11, games
  → §10, gamekit → its own lesson) rather than a new top-level section unless the feature is
  genuinely a new subsystem (tilemaps probably deserves a subsection under the existing game-kit
  lesson `docs/learn/12-game-kit.md`, not a new lesson number — use judgment, but don't renumber
  existing lessons without checking every cross-reference).

## The three features (implement all three; you choose the order, but STOP after each)

### Feature 6 — UI toolkit depth

The window system today (`row`/`column`/`text`/`button`/`spacer`/`text_field`/`checkbox`/
`slider`/`image`) has no way to show more content than fits on screen, no way to pick from a
list of options, and no multi-line text entry. Add, at minimum:

- **`scroll` widget** — a container (like `column`) that clips its children to its own
  height/width and lets the mouse wheel scroll them. Needs internal scroll-offset state that
  persists across frames without living in a PlainText variable (follow the existing pattern for
  `focused`/`dragging`/`caret` — runner-local state keyed by a stable per-frame `Control`/node
  index, not program state).
- **`list` widget** (a selectable list of text items, e.g. from a `Text list` PlainText value) —
  scrollable, with a `bind:`/`on_change` selected-index or selected-value contract like the other
  interactive widgets, and per-row hover/selection visuals.
- **`dropdown`** (closed by default, shows a `list`-like popup on click, collapses on selection or
  click-away) — reuse the `list` rendering internally rather than a third implementation.
- **Multi-line `text_field`** — either a `multiline: true` prop on the existing `text_field` or a
  new `text_area` widget; needs line-wrapping in layout and up/down arrow-key caret movement in
  addition to the left/right/Home/End/backspace/delete/paste editing `text_field` already has.
- **Tab / Shift+Tab focus movement** between interactive widgets (currently focus is
  click-only) — a natural fit for `text_field`s and the new `list`/`dropdown`.

Do NOT attempt a full menu-bar/tabs system if scroll + list + dropdown + multiline text_field
already make this a substantial, real feature — note menu bars/tabs as an explicit follow-up if
you cut them for scope, rather than doing all of it shallowly.

Ship: the new widgets in `ui.rs`/`interpreter.rs`/`checker.rs`/`game.rs`, an
`examples/scroll_list.pt` (or similarly named) example using scroll + list + dropdown +
multiline text_field together in one small tool (e.g. a scrollable to-do list with a category
dropdown), grammar updates, and a `docs/learn/11-ui.md` + `docs/language-reference.md` §11
update.

### Feature 9 — Audio depth

Today: `load_sound(path)` / `play_sound(id)`, nothing else. Add, at minimum:

- **`load_music(path)` / `play_music(id)` / `stop_music(id)`** — streamed background music
  (Raylib's `Music` type, not `Sound` — it's designed for long files and needs a per-frame
  `update_music_stream` call, which belongs in the `game.rs` frame loop next to the existing
  sound-play drain).
- **Looping** — `play_music` loops by default (typical for background music); give `play_sound`
  an optional `loop: true` keyword-arg for one-shot-vs-looping sound effects.
- **Volume / pan / pitch** — `set_volume(id, 0.0..1.0)`, `set_pitch(id, multiplier)` at minimum,
  working on both sound and music ids if Raylib's API allows a uniform treatment; check whether
  sound and music ids need separate namespaces or can share one (probably separate, since
  `Sound`/`Music` are different Raylib types — if so, make that explicit in the builtin names,
  e.g. `set_music_volume`/`set_sound_volume`, rather than a single ambiguous `set_volume`).
- **Fade** — at least a simple `fade_music(id, target_volume, seconds)` driven by the existing
  timer/frame-delta plumbing (look at how `after`/`every` timers work in `interpreter.rs` for the
  existing "do something over time" pattern, though this one is likely simplest as per-frame
  linear interpolation tracked in `GfxBridge` state, not a full timer).

Ship: the new builtins wired through all three files, an `examples/audio.pt` example (note in
its header that CI/headless runs can't hear it, but it should still type-check and run without a
window if it only calls the builtins outside a `game`/`window` block — check what currently
happens when an audio builtin is called with no window open, e.g. in `examples/save.pt`-style
pure console execution, and make sure it fails clearly rather than panicking if there's no audio
device, matching the existing "no-op if the device can't be opened" behavior in `game.rs`).
Update `docs/learn/10-games.md` "Sound" subsection and `docs/language-reference.md` §10.

### Feature 10 — Tilemaps + scenes

`gamekit` today does gravity + AABB bodies + hitboxes but no level geometry beyond individual
solid bodies, and no way to organize a game into multiple screens/levels. Add, at minimum:

- **A tilemap type**: something like `tilemap(cell_size: 32, rows: [...])` where `rows` is a list
  of text rows (each character a tile code) or a list of number rows (tile ids) — pick whichever
  reads more like English and is easiest for a beginner to hand-author in a `.pt` file; a
  `tile_at(map, x, y)` / `map.tile_at(x, y)` query, and **collision**: bodies should be able to
  collide against solid tiles in a tilemap the same way they collide against solid bodies today
  (`world.add(tilemap)` or a dedicated `world.add_tilemap(map, solid_tiles: [...])` — follow
  whichever is more consistent with the existing `physics_world`/`body`/`world.add` shape in
  `gamekit.rs`). Put the actual AABB-vs-tilemap sweep logic in a plain, unit-testable Rust
  function in `gamekit.rs`, following the existing `gravity_lands_on_platform` /
  `hits_once_per_swing` test pattern — write at least two new `cargo test` cases (falling onto a
  tile, being blocked by a solid tile horizontally) before touching any Raylib drawing code.
- **`draw_tilemap(map, tile_colors: dictionary {...})` or sprite-based tile drawing** — at
  minimum a debug-colored draw so `examples/` can show it working without needing a tileset
  image asset; sprite-sheet tile drawing is a reasonable stretch goal but explicitly optional —
  note it as a follow-up if cut.
- **A minimal scene/level concept** — the simplest honest version is "a scene is just a PlainText
  function that sets up game state, and switching scenes means clearing/re-initializing that
  state and swapping which `on update`/`on draw` logic runs." Do NOT build a heavyweight
  scene-graph or a state machine framework; a couple of scene-related example patterns in the
  docs (e.g. "how to structure a game with a menu screen and a play screen using a `screen`
  variable and `if screen is "menu" { } else if screen is "play" { }` inside `on
  update`/`on draw`") may be enough, PLUS a small helper or two only if you find the raw pattern
  is genuinely awkward without one — don't invent API surface the language doesn't need.

Ship: the tilemap builtins/collision wired through `gamekit.rs` + the three-file builtin
pattern, the new `cargo test` cases, an `examples/tilemap.pt` (or extend the existing
`examples/platformer.pt` if that's cleaner — use judgment, but don't create two overlapping
platformer examples), and a tilemap subsection in `docs/learn/12-game-kit.md` +
`docs/language-reference.md`'s gamekit section, including an explicit note on what's out of
scope (slopes, tile animation, a level editor).

## Suggested order

Tilemaps (10) touches the most Rust logic and is the best return-on-investment for the "games
language" identity; UI depth (6) is the most user-visible; audio (9) is the most contained and
lowest-risk. A reasonable order is **10 → 6 → 9**, but you may pick any order — the only hard
rule is stopping after each one is fully shipped and verified.

## Testing / quality bar (same as prior milestones)

- `cargo build` and `cargo build --release` with **zero warnings**, every time you stop.
- `cargo test` — add real unit tests for any non-trivial pure-Rust logic (tilemap collision,
  audio fade math, scroll/list layout math), and don't regress the existing suite.
- Every `.pt` file in `examples/` must `plaintext check` clean after each feature — run the full
  sweep, not just your new file.
- For anything requiring an actual window (UI widgets, audio playback, tilemap rendering), be
  explicit in your final report about what you verified by running headlessly/type-checking vs.
  what you could only verify by code review — do not claim to have "tested" GUI interaction you
  could not actually drive.
- No secrets, no new external network dependencies, nothing that breaks a CI runner with no
  display/audio device.

## When you finish EACH feature (not just at the very end)

Stop and report, per feature:
- What shipped, with exact file paths (new and modified).
- The example file(s) and what running/checking them showed.
- Any API deviations from this prompt, and why.
- Anything explicitly deferred/cut for scope, named plainly (e.g. "menu bars/tabs not
  implemented — scroll+list+dropdown+multiline text_field already substantial").
- What you verified by execution vs. by code review only.

Then wait. Do not start the next feature, do not bump the crate version, and do not commit until
told to proceed.
