//! Raylib GUI for `plaintext edit_tilemap <file.pt>`.
//!
//! Paints text-row tilemaps, toggles solids, assigns one PNG per character via
//! drag-and-drop, and rewrites the `.pt` file in place.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use raylib::core::drawing::RaylibScissorModeExt;
use raylib::prelude::*;

use crate::tilemap_edit_source::{
    apply_to_source, insert_starter, load_from_source, relativize_path, TilemapDoc,
};

const PALETTE: &[char] = &['#', '.', 'P', 'A', 'B', 'C', 'D', 'E', 'F', 'G'];
const LEFT: i32 = 160;
const TOP: i32 = 56;
const CELL_VIEW: i32 = 28;

// Grid-size buttons in the left panel: (x, y, w, h).
const COL_MINUS: (i32, i32, i32, i32) = (16, 512, 30, 26);
const COL_PLUS: (i32, i32, i32, i32) = (52, 512, 30, 26);
const ROW_MINUS: (i32, i32, i32, i32) = (16, 568, 30, 26);
const ROW_PLUS: (i32, i32, i32, i32) = (52, 568, 30, 26);

fn in_rect(mx: i32, my: i32, r: (i32, i32, i32, i32)) -> bool {
    mx >= r.0 && mx < r.0 + r.2 && my >= r.1 && my < r.1 + r.3
}

struct EditorState {
    path: PathBuf,
    source: String,
    doc: TilemapDoc,
    selected: char,
    status: String,
    dirty: bool,
    backed_up: bool,
    /// Texture id per image path (loaded lazily).
    textures: HashMap<String, Texture2D>,
    /// Map view: pan in screen pixels (grid origin relative to LEFT/TOP).
    pan_x: f32,
    pan_y: f32,
    /// Multiplier on [`CELL_VIEW`] (clamped).
    zoom: f32,
    /// Last mouse pos while middle-drag / Space-drag panning.
    drag_mx: i32,
    drag_my: i32,
    panning: bool,
}

fn cell_px(zoom: f32) -> i32 {
    ((CELL_VIEW as f32) * zoom).round().clamp(6.0, 96.0) as i32
}

pub fn run(path: &Path) -> Result<(), String> {
    let path = path.to_path_buf();
    let mut source = std::fs::read_to_string(&path)
        .map_err(|e| format!("can't read {}: {}", path.display(), e))?;

    let doc = match load_from_source(&source) {
        Ok(d) => d,
        Err(_) => {
            source = insert_starter(&source);
            load_from_source(&source).map_err(|e| {
                format!("couldn't find or insert a tilemap in {}: {}", path.display(), e)
            })?
        }
    };

    let title = format!("PlainText tilemap — {}", path.display());
    let (mut rl, thread) = raylib::init().size(1100, 720).title(&title).build();
    rl.set_target_fps(60);
    rl.set_exit_key(None); // Esc is for UI; close via window X

    let mut state = EditorState {
        path,
        source: source.clone(),
        doc,
        selected: '#',
        status: "Paint · MMB/Space-drag pan · Wheel zoom · S solid · Ctrl+S save · drop PNG"
            .into(),
        dirty: false,
        backed_up: false,
        textures: HashMap::new(),
        pan_x: 0.0,
        pan_y: 0.0,
        zoom: 1.0,
        drag_mx: 0,
        drag_my: 0,
        panning: false,
    };

    // Preload any mapped PNGs.
    reload_textures(&mut rl, &thread, &mut state);

    while !rl.window_should_close() {
        handle_input(&mut rl, &thread, &mut state);
        draw_frame(&mut rl, &thread, &state);
    }
    Ok(())
}

fn reload_textures(rl: &mut RaylibHandle, thread: &RaylibThread, state: &mut EditorState) {
    let paths: Vec<String> = state.doc.tiles.values().map(|s| s.clone()).collect();
    for p in paths {
        if state.textures.contains_key(&p) {
            continue;
        }
        let try_path = resolve_asset(&state.path, &p);
        if let Ok(tex) = rl.load_texture(thread, &try_path) {
            state.textures.insert(p, tex);
        }
    }
}

fn resolve_asset(pt_file: &Path, rel: &str) -> String {
    let cand = PathBuf::from(rel);
    if cand.is_file() {
        return rel.to_string();
    }
    if let Some(parent) = pt_file.parent() {
        let joined = parent.join(rel);
        if joined.is_file() {
            return joined.to_string_lossy().into();
        }
    }
    rel.to_string()
}

fn handle_input(rl: &mut RaylibHandle, thread: &RaylibThread, state: &mut EditorState) {
    // Palette hotkeys 1..9
    for (i, ch) in PALETTE.iter().enumerate().take(9) {
        let key = match i {
            0 => KeyboardKey::KEY_ONE,
            1 => KeyboardKey::KEY_TWO,
            2 => KeyboardKey::KEY_THREE,
            3 => KeyboardKey::KEY_FOUR,
            4 => KeyboardKey::KEY_FIVE,
            5 => KeyboardKey::KEY_SIX,
            6 => KeyboardKey::KEY_SEVEN,
            7 => KeyboardKey::KEY_EIGHT,
            8 => KeyboardKey::KEY_NINE,
            _ => continue,
        };
        if rl.is_key_pressed(key) {
            state.selected = *ch;
        }
    }

    if rl.is_key_pressed(KeyboardKey::KEY_S)
        && !rl.is_key_down(KeyboardKey::KEY_LEFT_CONTROL)
        && !rl.is_key_down(KeyboardKey::KEY_RIGHT_CONTROL)
    {
        let ch = state.selected;
        let on = !state.doc.is_solid(ch);
        state.doc.set_solid(ch, on);
        state.dirty = true;
        state.status = if on {
            format!("`{ch}` is solid")
        } else {
            format!("`{ch}` is not solid")
        };
    }

    if (rl.is_key_down(KeyboardKey::KEY_LEFT_CONTROL) || rl.is_key_down(KeyboardKey::KEY_RIGHT_CONTROL))
        && rl.is_key_pressed(KeyboardKey::KEY_S)
    {
        save(state);
    }

    if rl.is_key_pressed(KeyboardKey::KEY_R)
        && (rl.is_key_down(KeyboardKey::KEY_LEFT_CONTROL) || rl.is_key_down(KeyboardKey::KEY_RIGHT_CONTROL))
    {
        revert(state);
        reload_textures(rl, thread, state);
    }

    // Drag-drop PNGs → selected char
    if rl.is_file_dropped() {
        let list = rl.load_dropped_files();
        let paths = list.paths();
        if let Some(p) = paths.first() {
            let path = PathBuf::from(p);
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if ext == "png" {
                let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                let rel = relativize_path(&path, &cwd);
                state.doc.tiles.insert(state.selected, rel.clone());
                state.dirty = true;
                state.status = format!("`{}` → {}", state.selected, rel);
                reload_textures(rl, thread, state);
            } else {
                state.status = "drop a .png file".into();
            }
        }
    }

    let mx = rl.get_mouse_x();
    let my = rl.get_mouse_y();
    let over_map = mx >= LEFT && my >= TOP;

    // Pan: middle mouse, or Space + left drag (only over the map).
    let space = rl.is_key_down(KeyboardKey::KEY_SPACE);
    let mid = rl.is_mouse_button_down(MouseButton::MOUSE_BUTTON_MIDDLE);
    let space_drag = space && rl.is_mouse_button_down(MouseButton::MOUSE_BUTTON_LEFT);
    if over_map && (mid || space_drag) {
        if !state.panning {
            state.panning = true;
            state.drag_mx = mx;
            state.drag_my = my;
        } else {
            state.pan_x += (mx - state.drag_mx) as f32;
            state.pan_y += (my - state.drag_my) as f32;
            state.drag_mx = mx;
            state.drag_my = my;
        }
    } else {
        state.panning = false;
    }

    // Arrow keys nudge the view.
    let step = 24.0;
    if rl.is_key_down(KeyboardKey::KEY_LEFT) {
        state.pan_x += step;
    }
    if rl.is_key_down(KeyboardKey::KEY_RIGHT) {
        state.pan_x -= step;
    }
    if rl.is_key_down(KeyboardKey::KEY_UP) {
        state.pan_y += step;
    }
    if rl.is_key_down(KeyboardKey::KEY_DOWN) {
        state.pan_y -= step;
    }

    // Wheel zoom toward the cursor (over the map).
    let wheel = rl.get_mouse_wheel_move();
    if over_map && wheel != 0.0 {
        let old_cell = cell_px(state.zoom) as f32;
        let before_col = (mx as f32 - LEFT as f32 - state.pan_x) / old_cell;
        let before_row = (my as f32 - TOP as f32 - state.pan_y) / old_cell;
        let factor = if wheel > 0.0 { 1.12 } else { 1.0 / 1.12 };
        state.zoom = (state.zoom * factor).clamp(0.25, 4.0);
        let new_cell = cell_px(state.zoom) as f32;
        state.pan_x = mx as f32 - LEFT as f32 - before_col * new_cell;
        state.pan_y = my as f32 - TOP as f32 - before_row * new_cell;
    }

    // Palette clicks
    if rl.is_mouse_button_pressed(MouseButton::MOUSE_BUTTON_LEFT) && mx < LEFT - 8 {
        if let Some(ch) = palette_hit(mx, my) {
            state.selected = ch;
        }
        if save_button_hit(mx, my) {
            save(state);
        }
        if revert_button_hit(mx, my) {
            revert(state);
            reload_textures(rl, thread, state);
        }
        resize_click(mx, my, state);
    }

    // Grid paint (not while panning / Space held)
    if !state.panning && !space {
        if let Some((col, row)) = grid_cell(mx, my, state) {
            if rl.is_mouse_button_down(MouseButton::MOUSE_BUTTON_LEFT) {
                if state.doc.tile_at(col, row) != state.selected {
                    state.doc.set_tile(col, row, state.selected);
                    state.dirty = true;
                }
            }
            if rl.is_mouse_button_down(MouseButton::MOUSE_BUTTON_RIGHT) {
                if state.doc.tile_at(col, row) != '.' {
                    state.doc.set_tile(col, row, '.');
                    state.dirty = true;
                }
            }
        }
    }
}

fn palette_hit(mx: i32, my: i32) -> Option<char> {
    let x0 = 16;
    let y0 = 100;
    for (i, ch) in PALETTE.iter().enumerate() {
        let y = y0 + i as i32 * 36;
        if mx >= x0 && mx < x0 + 120 && my >= y && my < y + 32 {
            return Some(*ch);
        }
    }
    None
}

fn save_button_hit(mx: i32, my: i32) -> bool {
    mx >= 16 && mx < 140 && my >= 16 && my < 44
}

fn revert_button_hit(mx: i32, my: i32) -> bool {
    mx >= 16 && mx < 140 && my >= 48 && my < 76
}

/// Handle a click on the +/- column and row buttons.
fn resize_click(mx: i32, my: i32, state: &mut EditorState) {
    let changed = if in_rect(mx, my, COL_PLUS) {
        state.doc.add_column()
    } else if in_rect(mx, my, COL_MINUS) {
        state.doc.remove_column()
    } else if in_rect(mx, my, ROW_PLUS) {
        state.doc.add_row()
    } else if in_rect(mx, my, ROW_MINUS) {
        state.doc.remove_row()
    } else {
        return;
    };
    if changed {
        state.dirty = true;
        state.status = format!("grid is {}×{}", state.doc.width(), state.doc.height());
    } else {
        state.status = "grid must stay at least 1×1".into();
    }
}

fn grid_cell(mx: i32, my: i32, state: &EditorState) -> Option<(usize, usize)> {
    if mx < LEFT || my < TOP {
        return None;
    }
    let cell = cell_px(state.zoom) as f32;
    let col = ((mx as f32 - LEFT as f32 - state.pan_x) / cell).floor() as i32;
    let row = ((my as f32 - TOP as f32 - state.pan_y) / cell).floor() as i32;
    if col < 0 || row < 0 {
        return None;
    }
    let col = col as usize;
    let row = row as usize;
    if col < state.doc.width() && row < state.doc.height() {
        Some((col, row))
    } else {
        None
    }
}

fn save(state: &mut EditorState) {
    match apply_to_source(&state.source, &state.doc) {
        Ok(new_src) => {
            if !state.backed_up {
                let bak = state.path.with_extension("pt.bak");
                let _ = std::fs::write(&bak, &state.source);
                state.backed_up = true;
            }
            if let Err(e) = std::fs::write(&state.path, &new_src) {
                state.status = format!("save failed: {e}");
                return;
            }
            state.source = new_src;
            // Reload doc so solid_loc / has_tiles_dict stay accurate.
            match load_from_source(&state.source) {
                Ok(d) => state.doc = d,
                Err(e) => {
                    state.status = format!("saved, but reload failed: {e}");
                    return;
                }
            }
            state.dirty = false;
            state.status = format!("saved {}", state.path.display());
        }
        Err(e) => state.status = format!("can't write source: {e}"),
    }
}

fn revert(state: &mut EditorState) {
    match load_from_source(&state.source) {
        Ok(d) => {
            state.doc = d;
            state.dirty = false;
            state.status = "reverted to last saved text".into();
        }
        Err(e) => state.status = format!("revert failed: {e}"),
    }
}

fn draw_frame(rl: &mut RaylibHandle, thread: &RaylibThread, state: &EditorState) {
    let win_w = rl.get_screen_width();
    let win_h = rl.get_screen_height();
    let mut d = rl.begin_drawing(thread);
    d.clear_background(Color::new(36, 38, 45, 255));

    // Buttons
    draw_button(&mut d, 16, 16, 124, 28, "Save (Ctrl+S)", Color::new(60, 140, 90, 255));
    draw_button(&mut d, 16, 48, 124, 28, "Revert", Color::new(90, 90, 110, 255));

    d.draw_text("Palette", 16, 84, 18, Color::LIGHTGRAY);
    for (i, ch) in PALETTE.iter().enumerate() {
        let y = 100 + i as i32 * 36;
        let selected = *ch == state.selected;
        let bg = if selected {
            Color::new(80, 120, 200, 255)
        } else {
            Color::new(55, 58, 68, 255)
        };
        d.draw_rectangle(16, y, 120, 32, bg);
        let solid = if state.doc.is_solid(*ch) { " solid" } else { "" };
        let label = format!("{ch}{solid}");
        d.draw_text(&label, 28, y + 6, 18, Color::WHITE);
        if let Some(path) = state.doc.tiles.get(ch) {
            if let Some(tex) = state.textures.get(path) {
                d.draw_texture_ex(
                    tex,
                    Vector2::new(100.0, (y + 4) as f32),
                    0.0,
                    24.0 / tex.width().max(1) as f32,
                    Color::WHITE,
                );
            }
        }
    }

    // Grid (clipped to the map viewport; pan + zoom applied).
    let cols = state.doc.width();
    let rows = state.doc.height();

    // Grid-size controls (left panel, below the palette).
    d.draw_text("Grid size", 16, 476, 18, Color::LIGHTGRAY);
    d.draw_text(&format!("Columns: {cols}"), 16, 500, 16, Color::WHITE);
    draw_button(&mut d, COL_MINUS.0, COL_MINUS.1, COL_MINUS.2, COL_MINUS.3, "-", Color::new(90, 90, 110, 255));
    draw_button(&mut d, COL_PLUS.0, COL_PLUS.1, COL_PLUS.2, COL_PLUS.3, "+", Color::new(60, 120, 150, 255));
    d.draw_text(&format!("Rows: {rows}"), 16, 546, 16, Color::WHITE);
    draw_button(&mut d, ROW_MINUS.0, ROW_MINUS.1, ROW_MINUS.2, ROW_MINUS.3, "-", Color::new(90, 90, 110, 255));
    draw_button(&mut d, ROW_PLUS.0, ROW_PLUS.1, ROW_PLUS.2, ROW_PLUS.3, "+", Color::new(60, 120, 150, 255));
    let cell = cell_px(state.zoom);
    let view_w = (win_w - LEFT).max(1);
    let view_h = (win_h - TOP).max(1);

    d.draw_scissor_mode(LEFT, TOP, view_w, view_h, |mut sd| {
        for row in 0..rows {
            for col in 0..cols {
                let x = LEFT + state.pan_x as i32 + col as i32 * cell;
                let y = TOP + state.pan_y as i32 + row as i32 * cell;
                // Skip cells fully outside the viewport.
                if x + cell < LEFT || y + cell < TOP || x > LEFT + view_w || y > TOP + view_h {
                    continue;
                }
                let ch = state.doc.tile_at(col, row);
                let mut filled = false;
                if let Some(path) = state.doc.tiles.get(&ch) {
                    if let Some(tex) = state.textures.get(path) {
                        let scale = cell as f32 / tex.width().max(1) as f32;
                        sd.draw_texture_ex(
                            tex,
                            Vector2::new(x as f32, y as f32),
                            0.0,
                            scale,
                            Color::WHITE,
                        );
                        filled = true;
                    }
                }
                if !filled {
                    let color = char_color(ch, state.doc.is_solid(ch));
                    sd.draw_rectangle(x, y, cell - 1, cell - 1, color);
                    if ch != '.' && cell >= 14 {
                        let label = ch.to_string();
                        sd.draw_text(
                            &label,
                            x + cell / 4,
                            y + cell / 5,
                            (cell / 2).max(10),
                            Color::WHITE,
                        );
                    }
                }
                sd.draw_rectangle_lines(x, y, cell, cell, Color::new(20, 20, 25, 255));
            }
        }
    });

    let dirty = if state.dirty { "  • unsaved" } else { "" };
    let header = format!(
        "{}  {}×{}  zoom {:.0}%{}",
        state.path.display(),
        cols,
        rows,
        state.zoom * 100.0,
        dirty
    );
    d.draw_text(&header, LEFT, 16, 18, Color::RAYWHITE);
    d.draw_text(&state.status, LEFT, 36, 16, Color::new(180, 200, 160, 255));
    d.draw_text(
        "MMB or Space-drag to pan · Wheel to zoom · Drop PNG onto selected character · P = spawn",
        LEFT,
        win_h - 28,
        16,
        Color::LIGHTGRAY,
    );
}

fn draw_button(d: &mut RaylibDrawHandle, x: i32, y: i32, w: i32, h: i32, label: &str, color: Color) {
    d.draw_rectangle(x, y, w, h, color);
    d.draw_text(label, x + 10, y + 6, 16, Color::WHITE);
}

fn char_color(ch: char, solid: bool) -> Color {
    match ch {
        '#' => Color::new(110, 110, 120, 255),
        '.' => Color::new(50, 70, 90, 255),
        'P' => Color::new(60, 160, 80, 255),
        _ if solid => Color::new(140, 90, 70, 255),
        _ => Color::new(70, 90, 130, 255),
    }
}
