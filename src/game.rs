//! Runs a PlainText `game` block on a real Raylib window.
//!
//! This is the only module that touches Raylib. Each frame it (1) copies input
//! state into the shared [`GfxBridge`], (2) runs the `update`/`draw` hooks in
//! the interpreter, then (3) replays the draw commands the `draw` hook produced
//! onto the window.

use std::cell::RefCell;
use std::rc::Rc;

use raylib::core::audio::{RaylibAudio, Sound};
use raylib::prelude::*;

use crate::ast::{Expr, GameDecl, Program, WindowDecl};
use crate::diagnostics::Diagnostic;
use crate::gfx::{Color as PtColor, DrawCmd, GfxBridge};
use crate::interpreter::Interpreter;
use crate::ui;
use crate::value::Value;

pub fn run(program: &Program, game: &GameDecl) -> Result<(), Diagnostic> {
    let width = prop_number(&game.props, "width").unwrap_or(800.0) as i32;
    let height = prop_number(&game.props, "height").unwrap_or(600.0) as i32;

    let bridge = Rc::new(RefCell::new(GfxBridge::new(width, height)));
    let mut interp = Interpreter::new();
    interp.set_gfx(bridge.clone());

    // Run init statements (set up game state) before opening the window.
    let scope = interp.prepare_game(program, game)?;

    let start = game.hooks.iter().find(|h| h.name == "start");
    let update = game.hooks.iter().find(|h| h.name == "update");
    let draw = game.hooks.iter().find(|h| h.name == "draw");

    let (mut rl, thread) = raylib::init().size(width, height).title(&game.title).build();
    rl.set_target_fps(60);

    // Audio device. Leaked to `'static` so the sounds that borrow it can live
    // for the whole run (it's freed at process exit anyway). If the device
    // can't be opened, sound calls become harmless no-ops.
    let audio: Option<&'static RaylibAudio> = RaylibAudio::init_audio_device()
        .ok()
        .map(|a| &*Box::leak(Box::new(a)));
    let mut textures: Vec<Option<Texture2D>> = Vec::new();
    let mut sounds: Vec<Option<Sound<'static>>> = Vec::new();
    let mut fonts: Vec<Option<Font>> = Vec::new();

    // Load anything queued during init before the first hook runs.
    load_pending(&mut rl, &thread, audio, &bridge, &mut textures, &mut sounds, &mut fonts);

    if let Some(h) = start {
        interp.run_hook(&scope, h, vec![])?;
        load_pending(&mut rl, &thread, audio, &bridge, &mut textures, &mut sounds, &mut fonts);
    }

    while !rl.window_should_close() {
        sync_input(&mut bridge.borrow_mut(), &rl);

        let dt = rl.get_frame_time() as f64;
        if let Some(h) = update {
            interp.run_hook(&scope, h, vec![Value::Number(dt)])?;
        }
        // Fire any due `after`/`every` timers.
        interp.tick_timers(dt)?;

        // Fulfill any newly requested assets, then play queued sounds.
        load_pending(&mut rl, &thread, audio, &bridge, &mut textures, &mut sounds, &mut fonts);
        for id in bridge.borrow_mut().sound_plays.drain(..) {
            if let Some(Some(s)) = sounds.get(id) {
                s.play();
            }
        }

        bridge.borrow_mut().draw.clear();
        if let Some(h) = draw {
            interp.run_hook(&scope, h, vec![])?;
        }

        let mut d = rl.begin_drawing(&thread);
        // Default background if the program didn't clear the screen itself.
        d.clear_background(Color::new(245, 245, 245, 255));
        for cmd in bridge.borrow().draw.iter() {
            render(&mut d, cmd, &textures, &fonts);
        }
    }
    Ok(())
}

/// Fulfill queued sprite/sound/font load requests, recording sprite sizes back
/// into the bridge so `sprite_width`/`sprite_height` work.
fn load_pending(
    rl: &mut RaylibHandle,
    thread: &RaylibThread,
    audio: Option<&'static RaylibAudio>,
    bridge: &Rc<RefCell<GfxBridge>>,
    textures: &mut Vec<Option<Texture2D>>,
    sounds: &mut Vec<Option<Sound<'static>>>,
    fonts: &mut Vec<Option<Font>>,
) {
    let sprite_reqs: Vec<(usize, String)> = bridge.borrow_mut().sprite_loads.drain(..).collect();
    for (id, path) in sprite_reqs {
        let tex = rl.load_texture(thread, &path).ok();
        if let Some(t) = &tex {
            bridge.borrow_mut().sprite_sizes.insert(id, (t.width(), t.height()));
        }
        grow_into(textures, id, tex);
    }

    let sound_reqs: Vec<(usize, String)> = bridge.borrow_mut().sound_loads.drain(..).collect();
    for (id, path) in sound_reqs {
        let snd = audio.and_then(|a| a.new_sound(&path).ok());
        grow_into(sounds, id, snd);
    }

    let font_reqs: Vec<(usize, String)> = bridge.borrow_mut().font_loads.drain(..).collect();
    for (id, path) in font_reqs {
        let font = rl.load_font(thread, &path).ok();
        grow_into(fonts, id, font);
    }
}

/// Place `value` at index `id`, growing the vec with `None` as needed.
fn grow_into<T>(vec: &mut Vec<Option<T>>, id: usize, value: Option<T>) {
    while vec.len() <= id {
        vec.push(None);
    }
    vec[id] = value;
}

/// Run a `window` block: an immediate-mode UI redrawn every frame.
pub fn run_window(program: &Program, window: &WindowDecl) -> Result<(), Diagnostic> {
    let width = prop_number(&window.props, "width").unwrap_or(400.0) as i32;
    let height = prop_number(&window.props, "height").unwrap_or(300.0) as i32;

    let bridge = Rc::new(RefCell::new(GfxBridge::new(width, height)));
    let mut interp = Interpreter::new();
    interp.set_gfx(bridge.clone());
    let scope = interp.prepare(program)?;

    // Optional window background color (`bg:` / `background:`).
    let mut bg = Color::new(245, 245, 247, 255);
    for (name, expr) in &window.props {
        if name == "bg" || name == "background" {
            let v = interp.eval_in(expr, &scope)?;
            let c = interp.value_as_color(&v, expr.span())?;
            bg = to_rl(c);
        }
    }

    let (mut rl, thread) = raylib::init().size(width, height).title(&window.title).build();
    rl.set_target_fps(60);

    let mut textures: Vec<Option<Texture2D>> = Vec::new();
    let mut fonts: Vec<Option<Font>> = Vec::new();
    // Assets queued during top-level setup (load_sprite / load_font).
    load_pending(&mut rl, &thread, None, &bridge, &mut textures, &mut Vec::new(), &mut fonts);

    // UI state that lives across frames but not in the program: which text field
    // has keyboard focus, and which slider (if any) is being dragged. These are
    // indices into the per-frame `controls` list, which is stable as long as the
    // widget tree keeps the same shape.
    let mut focused: Option<usize> = None;
    let mut dragging: Option<usize> = None;
    // Caret position (a character index) within the focused text field.
    let mut caret: usize = 0;

    while !rl.window_should_close() {
        sync_input(&mut bridge.borrow_mut(), &rl);
        load_pending(&mut rl, &thread, None, &bridge, &mut textures, &mut Vec::new(), &mut fonts);

        // Text typed this frame (must be drained even when nothing is focused,
        // so it doesn't pile up in Raylib's queue).
        let mut typed = String::new();
        while let Some(ch) = rl.get_char_pressed() {
            typed.push(ch);
        }
        // Editing keys for the focused field. Movement/delete keys auto-repeat
        // when held; Ctrl+V pastes.
        let ctrl = rl.is_key_down(KeyboardKey::KEY_LEFT_CONTROL)
            || rl.is_key_down(KeyboardKey::KEY_RIGHT_CONTROL);
        let edit = TextEdit {
            backspace: key_repeat(&rl, KeyboardKey::KEY_BACKSPACE),
            delete: key_repeat(&rl, KeyboardKey::KEY_DELETE),
            left: key_repeat(&rl, KeyboardKey::KEY_LEFT),
            right: key_repeat(&rl, KeyboardKey::KEY_RIGHT),
            home: rl.is_key_pressed(KeyboardKey::KEY_HOME),
            end: rl.is_key_pressed(KeyboardKey::KEY_END),
            paste: ctrl && rl.is_key_pressed(KeyboardKey::KEY_V),
            clipboard: rl.get_clipboard_text().unwrap_or_default(),
        };
        // A Ctrl chord isn't text — don't also insert the raw letter.
        if ctrl {
            typed.clear();
        }

        let (mouse, pressed, down) = {
            let b = bridge.borrow();
            ((b.mouse_x, b.mouse_y), b.mouse_pressed, b.mouse_down)
        };

        // Rebuild the widget tree from current state, lay it out, collect draw
        // commands and interactive controls.
        let mut nodes = interp.build_widgets(&window.root, &scope)?;
        ui::layout_root(&mut nodes, width, height);
        let mut cmds = Vec::new();
        let mut controls = Vec::new();
        ui::collect(&nodes, mouse, focused, caret, &mut cmds, &mut controls);

        if !down {
            dragging = None;
        }

        // A press dispatches to the topmost control under the cursor and (re)sets
        // keyboard focus. Any change is written back to the program, so it shows
        // on the next frame's rebuild.
        if pressed {
            focused = None;
            let hit = controls
                .iter()
                .enumerate()
                .rev()
                .find(|(_, c)| point_in(mouse, c))
                .map(|(i, _)| i);
            if let Some(i) = hit {
                match controls[i].kind {
                    ui::ControlKind::Button => {
                        if let Some(cb) = controls[i].callback.clone() {
                            interp.call_callback(&cb)?;
                        }
                    }
                    ui::ControlKind::TextField => {
                        focused = Some(i);
                        caret = caret_from_x(&controls[i], mouse.0);
                    }
                    ui::ControlKind::Checkbox => {
                        let new_val = !controls[i].checked;
                        write_back(&mut interp, &scope, &controls[i], Value::Bool(new_val))?;
                    }
                    ui::ControlKind::Slider => {
                        dragging = Some(i);
                        if let Some(v) = slider_value(&controls[i], mouse.0) {
                            write_back(&mut interp, &scope, &controls[i], Value::Number(v))?;
                        }
                    }
                }
            }
        }

        // Continue a slider drag while the mouse is held.
        if down {
            if let Some(i) = dragging {
                if let Some(c) = controls.get(i) {
                    if c.kind == ui::ControlKind::Slider {
                        if let Some(v) = slider_value(c, mouse.0) {
                            if (v as f32 - c.number).abs() > f32::EPSILON {
                                write_back(&mut interp, &scope, c, Value::Number(v))?;
                            }
                        }
                    }
                }
            }
        }

        // Edit the focused field: caret movement, insert/delete at the caret,
        // and paste. Only a real content change is written back.
        if let Some(i) = focused {
            if let Some(c) = controls.get(i) {
                if c.kind == ui::ControlKind::TextField {
                    let mut chars: Vec<char> = c.text.chars().collect();
                    let mut pos = caret.min(chars.len());
                    let mut changed = false;

                    if edit.left {
                        pos = pos.saturating_sub(1);
                    }
                    if edit.right {
                        pos = (pos + 1).min(chars.len());
                    }
                    if edit.home {
                        pos = 0;
                    }
                    if edit.end {
                        pos = chars.len();
                    }
                    if edit.backspace && pos > 0 {
                        chars.remove(pos - 1);
                        pos -= 1;
                        changed = true;
                    }
                    if edit.delete && pos < chars.len() {
                        chars.remove(pos);
                        changed = true;
                    }
                    if edit.paste {
                        for ch in edit.clipboard.chars().filter(|c| !c.is_control()) {
                            chars.insert(pos, ch);
                            pos += 1;
                            changed = true;
                        }
                    }
                    for ch in typed.chars() {
                        chars.insert(pos, ch);
                        pos += 1;
                        changed = true;
                    }

                    caret = pos;
                    if changed {
                        let s: String = chars.iter().collect();
                        write_back(&mut interp, &scope, c, Value::text(s))?;
                    }
                }
            } else {
                focused = None;
            }
        }

        let mut d = rl.begin_drawing(&thread);
        d.clear_background(bg);
        for cmd in &cmds {
            render(&mut d, cmd, &textures, &fonts);
        }
    }
    Ok(())
}

/// Write a control's new value back to the program: to its bound variable (if
/// any) and through its `on_change` handler (if any).
fn write_back(
    interp: &mut Interpreter,
    scope: &crate::value::Env,
    control: &ui::Control,
    value: Value,
) -> Result<(), Diagnostic> {
    if let Some(name) = &control.bind {
        interp.assign_var(scope, name, value.clone());
    }
    if let Some(cb) = &control.callback {
        interp.call_on_change(cb, value)?;
    }
    Ok(())
}

/// Keyboard editing state for the focused text field this frame.
struct TextEdit {
    backspace: bool,
    delete: bool,
    left: bool,
    right: bool,
    home: bool,
    end: bool,
    paste: bool,
    clipboard: String,
}

/// A key that counts as "pressed" on the initial press and while held (so
/// backspace/arrows repeat like a normal text box).
fn key_repeat(rl: &RaylibHandle, key: KeyboardKey) -> bool {
    rl.is_key_pressed(key) || rl.is_key_pressed_repeat(key)
}

/// Map a click x-position to a caret index within a text field, using the same
/// ~0.5em-per-character estimate the layout uses.
fn caret_from_x(c: &ui::Control, click_x: f32) -> usize {
    let em = (c.font_size as f32 * 0.5).max(1.0);
    let rel = (click_x - (c.x + 8.0)).max(0.0);
    ((rel / em).round() as usize).min(c.text.chars().count())
}

/// Map a mouse x-position to a slider's value, snapped to its step and clamped
/// to its range. Returns `None` if the slider has no width.
fn slider_value(c: &ui::Control, mouse_x: f32) -> Option<f64> {
    if c.w <= 0.0 {
        return None;
    }
    let frac = ((mouse_x - c.x) / c.w).clamp(0.0, 1.0);
    let mut v = c.min + frac * (c.max - c.min);
    if c.step > 0.0 {
        v = (v / c.step).round() * c.step;
    }
    // Order-safe clamp: `f32::clamp` panics if min > max, so never assume the
    // author wrote them in order.
    let (lo, hi) = (c.min.min(c.max), c.min.max(c.max));
    v = v.clamp(lo, hi);
    // Shed floating-point noise from the snap so a 0.1 step doesn't surface as
    // 0.30000000000000004 when displayed.
    Some((v as f64 * 1e6).round() / 1e6)
}

fn point_in(p: (f32, f32), c: &ui::Control) -> bool {
    p.0 >= c.x && p.0 <= c.x + c.w && p.1 >= c.y && p.1 <= c.y + c.h
}

fn render(
    d: &mut RaylibDrawHandle,
    cmd: &DrawCmd,
    textures: &[Option<Texture2D>],
    fonts: &[Option<Font>],
) {
    match cmd {
        DrawCmd::Clear(c) => d.clear_background(to_rl(*c)),
        DrawCmd::Circle { x, y, r, color } => {
            d.draw_circle_v(Vector2::new(*x, *y), *r, to_rl(*color));
        }
        DrawCmd::Rect { x, y, w, h, color } => {
            d.draw_rectangle_rec(Rectangle::new(*x, *y, *w, *h), to_rl(*color));
        }
        DrawCmd::Line { x1, y1, x2, y2, thick, color } => {
            d.draw_line_ex(Vector2::new(*x1, *y1), Vector2::new(*x2, *y2), *thick, to_rl(*color));
        }
        DrawCmd::Text { text, x, y, size, color, font } => {
            if let Some(id) = font {
                if let Some(Some(f)) = fonts.get(*id) {
                    d.draw_text_ex(f, text, Vector2::new(*x, *y), *size as f32, 1.0, to_rl(*color));
                    return;
                }
            }
            d.draw_text(text, *x as i32, *y as i32, *size, to_rl(*color));
        }
        DrawCmd::Sprite { id, x, y, scale, rotation } => {
            if let Some(Some(tex)) = textures.get(*id) {
                if *rotation == 0.0 {
                    d.draw_texture_ex(tex, Vector2::new(*x, *y), 0.0, *scale, Color::WHITE);
                } else {
                    // Rotate about the sprite's center.
                    let w = tex.width() as f32;
                    let h = tex.height() as f32;
                    let src = Rectangle::new(0.0, 0.0, w, h);
                    let dst = Rectangle::new(*x, *y, w * scale, h * scale);
                    let origin = Vector2::new(w * scale / 2.0, h * scale / 2.0);
                    d.draw_texture_pro(tex, src, dst, origin, *rotation, Color::WHITE);
                }
            }
        }
        DrawCmd::SpriteRect { id, x, y, w, h } => {
            if let Some(Some(tex)) = textures.get(*id) {
                let src = Rectangle::new(0.0, 0.0, tex.width() as f32, tex.height() as f32);
                let dst = Rectangle::new(*x, *y, *w, *h);
                d.draw_texture_pro(tex, src, dst, Vector2::zero(), 0.0, Color::WHITE);
            }
        }
    }
}

fn to_rl(c: PtColor) -> Color {
    Color::new(c.0, c.1, c.2, c.3)
}

/// Copy this frame's keyboard/mouse state into the bridge for the input
/// builtins to read.
fn sync_input(g: &mut GfxBridge, rl: &RaylibHandle) {
    g.keys_down.clear();
    g.keys_pressed.clear();
    for (name, key) in keymap() {
        if rl.is_key_down(*key) {
            g.keys_down.insert(name.to_string());
        }
        if rl.is_key_pressed(*key) {
            g.keys_pressed.insert(name.to_string());
        }
    }
    g.mouse_x = rl.get_mouse_x() as f32;
    g.mouse_y = rl.get_mouse_y() as f32;
    g.mouse_down = rl.is_mouse_button_down(MouseButton::MOUSE_BUTTON_LEFT);
    g.mouse_pressed = rl.is_mouse_button_pressed(MouseButton::MOUSE_BUTTON_LEFT);
}

/// PlainText key names → Raylib keys. Names are what `key_down("...")` expects.
fn keymap() -> &'static [(&'static str, KeyboardKey)] {
    use KeyboardKey::*;
    &[
        ("up", KEY_UP),
        ("down", KEY_DOWN),
        ("left", KEY_LEFT),
        ("right", KEY_RIGHT),
        ("space", KEY_SPACE),
        ("enter", KEY_ENTER),
        ("escape", KEY_ESCAPE),
        ("tab", KEY_TAB),
        ("w", KEY_W),
        ("a", KEY_A),
        ("s", KEY_S),
        ("d", KEY_D),
        ("q", KEY_Q),
        ("e", KEY_E),
        ("r", KEY_R),
        ("f", KEY_F),
        ("j", KEY_J),
        ("k", KEY_K),
        ("l", KEY_L),
        ("z", KEY_Z),
        ("x", KEY_X),
        ("c", KEY_C),
    ]
}

/// Evaluate a numeric property from a block header (only literals and unary
/// minus are supported here, which is all a window size ever needs).
fn prop_number(props: &[(String, Expr)], name: &str) -> Option<f64> {
    let (_, expr) = props.iter().find(|(n, _)| n == name)?;
    const_number(expr)
}

fn const_number(expr: &Expr) -> Option<f64> {
    match expr {
        Expr::Number(n, _) => Some(*n),
        Expr::Unary { op: crate::ast::UnaryOp::Neg, expr, .. } => const_number(expr).map(|n| -n),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slider(min: f32, max: f32, step: f32) -> ui::Control {
        ui::Control {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 20.0,
            kind: ui::ControlKind::Slider,
            callback: None,
            bind: None,
            checked: false,
            number: 0.0,
            min,
            max,
            step,
            text: String::new(),
            font_size: 20,
        }
    }

    #[test]
    fn slider_maps_and_snaps() {
        let s = slider(0.0, 100.0, 1.0);
        assert_eq!(slider_value(&s, 0.0), Some(0.0)); // left edge
        assert_eq!(slider_value(&s, 100.0), Some(100.0)); // right edge
        assert_eq!(slider_value(&s, 50.0), Some(50.0)); // middle
        // Off-track clicks clamp into range.
        assert_eq!(slider_value(&s, -20.0), Some(0.0));
        assert_eq!(slider_value(&s, 999.0), Some(100.0));
    }

    #[test]
    fn slider_reversed_range_does_not_panic() {
        // min > max must not panic on the internal clamp, and must stay in range.
        let s = slider(100.0, 0.0, 1.0);
        let v = slider_value(&s, 50.0).unwrap();
        assert!(v >= 0.0 && v <= 100.0);
    }

    #[test]
    fn slider_step_sheds_float_noise() {
        // A 0.1 step should yield a clean decimal, not 0.30000000000000004.
        let s = slider(0.0, 1.0, 0.1);
        let v = slider_value(&s, 30.0).unwrap(); // 30% of [0,1] = 0.3
        assert_eq!(v, 0.3);
    }
}
