# Tilemap editor design (`plaintext edit_tilemap`)

Validated in chat, 2026-08-09. Side feature before milestone Features 6 (UI depth) and 9 (audio).

**Status:** implemented (`plaintext edit_tilemap`, `src/tilemap_editor.rs`, `src/tilemap_edit_source.rs`).

## Goal

A small Raylib GUI that opens a `.pt` file, lets you paint a text-row tilemap, mark solids, place spawn characters, and assign one PNG per tile character via drag-and-drop — then **rewrites that file in place**.

## CLI

```bash
plaintext edit_tilemap path/to/file.pt
```

Not `plaintext edit` — the subcommand is specifically `edit_tilemap`.

## Product choices (locked)

| Choice | Decision |
|--------|----------|
| Host | Separate tool (Raylib window), same stack as games |
| PNG drop | **One PNG = one tile character** (not a tileset sheet) |
| Scope v1 | Tilemap rows + `solid_tiles` + spawn chars + image dict — **not** body/hitbox gizmos |
| Image mapping in source | **Separate nearby dictionary**, e.g. `level_tiles = dictionary { "#": "examples/assets/wall.png" }` — not a `tiles:` arg on `tilemap` |

## What gets edited

1. One `tilemap(...)` in the file, preferably bound like `level = tilemap(...)`.
2. Its `rows` and `solid_tiles`.
3. Spawn / special markers as ordinary paint characters (document `P` as player spawn).
4. A paired dictionary `<name>_tiles` (e.g. `level` → `level_tiles`). Create it beside the tilemap on Save if missing.

**Not in v1:** `body(...)`, `hitbox(...)`, multi-map tabs, tileset sheet slicing, VS Code panel, rich undo (Revert to last loaded/saved is enough).

## UI sketch

- **Center:** zoomable tile grid. Show PNG thumbnail when mapped; else letter / solid color.
- **Palette:** characters (`#`, `.`, `P`, …), solid toggle for selected char, erase tool (one consistent empty char).
- **Chrome:** open path, Save, Revert, cols×rows, `cell_size`.
- **Drop:** PNG onto window → path for **currently selected** palette character → update `level_tiles` (paths relative like existing `load_sprite` examples).

## File rewrite rules

- Locate `name = tilemap(...)` and `name_tiles = dictionary { ... }` with a focused parse (not a full language rewrite).
- Replace **only** those literal interiors; leave the rest of the file alone.
- Pretty-print rows (one quoted row per line) to match examples.
- On locate/parse failure: show error in the window; **do not** write.
- On first Save in a session: write `file.pt.bak` once.
- No tilemap in file: offer “Insert starter level” (`level = tilemap(...)` + `level_tiles = dictionary { }` after imports).

## Runtime

- v1 does **not** require `draw_tilemap` to read `level_tiles` / draw PNGs. Color drawing stays valid.
- The dictionary is editor-owned source of truth for image paths; sprite drawing from it is a follow-up.

## Testing / quality

- Unit tests: find + rewrite tilemap rows / `solid_tiles` / tiles dict without corrupting surrounding source.
- `plaintext check` on rewritten examples.
- GUI interaction and OS drag-drop: manual / code review; CI has no display.

## Out of scope (say so in docs)

Body/hitbox editor, slopes, tile animation, level marketplace, `plaintext edit` as a generic umbrella.

## Implementation sketch (when building)

- `src/main.rs` — subcommand `edit_tilemap`.
- New module e.g. `src/tilemap_editor.rs` (window loop) + small `src/tilemap_edit_source.rs` (locate/rewrite, unit-tested, no Raylib).
- Docs: lesson 12 + cheatsheet/CLI help; example note on `examples/tilemap.pt`.
- Do not bump crate version or commit unless asked.
