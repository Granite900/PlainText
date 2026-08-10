//! Raylib GUI for `plaintext edit_tilemap <file.pt>`.
//!
//! Paints text-row tilemaps, toggles solids, assigns one PNG per character via
//! drag-and-drop, and rewrites the `.pt` file in place.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use raylib::prelude::*;

use crate::tilemap_edit_source::{
    apply_to_source, insert_starter, load_from_source, relativize_path, TilemapDoc,
};

const PALETTE: &[char] = &['#', '.', 'P', 'A', 'B', 'C', 'D', 'E', 'F', 'G'];
const LEFT: i32 = 160;
const TOP: i32 = 56;
const CELL_VIEW: i32 = 28;

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
        status: "Left-click paint · Right-click erase (.) · S solid · Ctrl+S save · drop PNG onto window".into(),
        dirty: false,
        backed_up: false,
        textures: HashMap::new(),
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
    }

    // Grid paint
    if let Some((col, row)) = grid_cell(mx, my, state.doc.width(), state.doc.height()) {
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

fn grid_cell(mx: i32, my: i32, cols: usize, rows: usize) -> Option<(usize, usize)> {
    if mx < LEFT || my < TOP {
        return None;
    }
    let col = ((mx - LEFT) / CELL_VIEW) as usize;
    let row = ((my - TOP) / CELL_VIEW) as usize;
    if col < cols && row < rows {
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

    // Grid
    let cols = state.doc.width();
    let rows = state.doc.height();
    for row in 0..rows {
        for col in 0..cols {
            let x = LEFT + col as i32 * CELL_VIEW;
            let y = TOP + row as i32 * CELL_VIEW;
            let ch = state.doc.tile_at(col, row);
            let mut filled = false;
            if let Some(path) = state.doc.tiles.get(&ch) {
                if let Some(tex) = state.textures.get(path) {
                    let scale = CELL_VIEW as f32 / tex.width().max(1) as f32;
                    d.draw_texture_ex(tex, Vector2::new(x as f32, y as f32), 0.0, scale, Color::WHITE);
                    filled = true;
                }
            }
            if !filled {
                let color = char_color(ch, state.doc.is_solid(ch));
                d.draw_rectangle(x, y, CELL_VIEW - 1, CELL_VIEW - 1, color);
                if ch != '.' {
                    let label = ch.to_string();
                    d.draw_text(&label, x + 8, y + 4, 16, Color::WHITE);
                }
            }
            d.draw_rectangle_lines(x, y, CELL_VIEW, CELL_VIEW, Color::new(20, 20, 25, 255));
        }
    }

    let dirty = if state.dirty { "  • unsaved" } else { "" };
    let header = format!(
        "{}  {}×{}{}",
        state.path.display(),
        cols,
        rows,
        dirty
    );
    d.draw_text(&header, LEFT, 16, 18, Color::RAYWHITE);
    d.draw_text(&state.status, LEFT, 36, 16, Color::new(180, 200, 160, 255));
    d.draw_text(
        "Drop a PNG to assign it to the selected character. P = spawn marker.",
        LEFT,
        TOP + rows as i32 * CELL_VIEW + 12,
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
