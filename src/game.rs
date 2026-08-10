//! Runs a PlainText `game` block on a real Raylib window.
//!
//! This is the only module that touches Raylib. Each frame it (1) copies input
//! state into the shared [`GfxBridge`], (2) runs the `update`/`draw` hooks in
//! the interpreter, then (3) replays the draw commands the `draw` hook produced
//! onto the window.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use raylib::core::audio::{Music, RaylibAudio, Sound};
use raylib::core::drawing::{RaylibDraw, RaylibScissorModeExt};
use raylib::prelude::*;

use crate::ast::{Expr, GameDecl, Program, WindowDecl};
use crate::diagnostics::Diagnostic;
use crate::gfx::{
    fade_volume, Color as PtColor, DrawCmd, GfxBridge, MusicCmd, SoundCmd,
};
use crate::interpreter::Interpreter;
use crate::ui;
use crate::value::Value;

struct MusicFade {
    id: usize,
    start: f32,
    target: f32,
    elapsed: f32,
    duration: f32,
}

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
    let mut music: Vec<Option<Music<'static>>> = Vec::new();
    let mut fonts: Vec<Option<Font>> = Vec::new();
    let mut looping_sounds: HashMap<usize, bool> = HashMap::new();
    let mut music_volumes: HashMap<usize, f32> = HashMap::new();
    let mut music_fades: Vec<MusicFade> = Vec::new();

    // Load anything queued during init before the first hook runs.
    load_pending(
        &mut rl, &thread, audio, &bridge, &mut textures, &mut sounds, &mut music, &mut fonts,
    );

    if let Some(h) = start {
        interp.run_hook(&scope, h, vec![])?;
        load_pending(
            &mut rl, &thread, audio, &bridge, &mut textures, &mut sounds, &mut music, &mut fonts,
        );
    }

    while !rl.window_should_close() {
        sync_input(&mut bridge.borrow_mut(), &rl);

        let dt = rl.get_frame_time() as f64;
        if let Some(h) = update {
            interp.run_hook(&scope, h, vec![Value::Number(dt)])?;
        }
        // Fire any due `after`/`every` timers.
        interp.tick_timers(dt)?;

        // Fulfill any newly requested assets, then apply audio commands.
        load_pending(
            &mut rl, &thread, audio, &bridge, &mut textures, &mut sounds, &mut music, &mut fonts,
        );
        drain_audio(
            &bridge,
            &sounds,
            &mut music,
            &mut looping_sounds,
            &mut music_volumes,
            &mut music_fades,
        );
        tick_audio(
            dt as f32,
            &sounds,
            &mut music,
            &looping_sounds,
            &mut music_volumes,
            &mut music_fades,
        );

        bridge.borrow_mut().draw.clear();
        if let Some(h) = draw {
            interp.run_hook(&scope, h, vec![])?;
        }
        // Particles live in world space and draw after the program's `on draw`.
        let particle_cmds = {
            let mut g = bridge.borrow_mut();
            crate::gfx::tick_particles(&mut g.particles, dt as f32)
        };
        bridge.borrow_mut().draw.extend(particle_cmds);

        let mut d = rl.begin_drawing(&thread);
        // Default background if the program didn't clear the screen itself.
        d.clear_background(Color::new(245, 245, 245, 255));
        let (cam_x, cam_y) = {
            let b = bridge.borrow();
            (b.camera_x, b.camera_y)
        };
        for cmd in bridge.borrow().draw.iter() {
            render_one(&mut d, cmd, &textures, &fonts, cam_x, cam_y);
        }
    }
    Ok(())
}

/// Fulfill queued sprite/sound/music/font load requests, recording sprite sizes
/// back into the bridge so `sprite_width`/`sprite_height` work.
fn load_pending(
    rl: &mut RaylibHandle,
    thread: &RaylibThread,
    audio: Option<&'static RaylibAudio>,
    bridge: &Rc<RefCell<GfxBridge>>,
    textures: &mut Vec<Option<Texture2D>>,
    sounds: &mut Vec<Option<Sound<'static>>>,
    music: &mut Vec<Option<Music<'static>>>,
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

    let music_reqs: Vec<(usize, String)> = bridge.borrow_mut().music_loads.drain(..).collect();
    for (id, path) in music_reqs {
        let mut track = audio.and_then(|a| a.new_music(&path).ok());
        if let Some(m) = track.as_mut() {
            m.looping = true; // background music loops by default
        }
        grow_into(music, id, track);
    }

    let font_reqs: Vec<(usize, String)> = bridge.borrow_mut().font_loads.drain(..).collect();
    for (id, path) in font_reqs {
        let font = rl.load_font(thread, &path).ok();
        grow_into(fonts, id, font);
    }
}

fn drain_audio(
    bridge: &Rc<RefCell<GfxBridge>>,
    sounds: &[Option<Sound<'static>>],
    music: &mut [Option<Music<'static>>],
    looping_sounds: &mut HashMap<usize, bool>,
    music_volumes: &mut HashMap<usize, f32>,
    music_fades: &mut Vec<MusicFade>,
) {
    let sound_cmds: Vec<SoundCmd> = bridge.borrow_mut().sound_cmds.drain(..).collect();
    for cmd in sound_cmds {
        match cmd {
            SoundCmd::Play { id, looping } => {
                looping_sounds.insert(id, looping);
                if let Some(Some(s)) = sounds.get(id) {
                    s.play();
                }
            }
            SoundCmd::Stop(id) => {
                looping_sounds.remove(&id);
                if let Some(Some(s)) = sounds.get(id) {
                    s.stop();
                }
            }
            SoundCmd::SetVolume { id, volume } => {
                if let Some(Some(s)) = sounds.get(id) {
                    s.set_volume(volume);
                }
            }
            SoundCmd::SetPitch { id, pitch } => {
                if let Some(Some(s)) = sounds.get(id) {
                    s.set_pitch(pitch);
                }
            }
            SoundCmd::SetPan { id, pan } => {
                if let Some(Some(s)) = sounds.get(id) {
                    s.set_pan(pan);
                }
            }
        }
    }

    let music_cmds: Vec<MusicCmd> = bridge.borrow_mut().music_cmds.drain(..).collect();
    for cmd in music_cmds {
        match cmd {
            MusicCmd::Play(id) => {
                if let Some(Some(m)) = music.get_mut(id) {
                    m.looping = true;
                    m.play_stream();
                }
            }
            MusicCmd::Stop(id) => {
                music_fades.retain(|f| f.id != id);
                if let Some(Some(m)) = music.get_mut(id) {
                    m.stop_stream();
                }
            }
            MusicCmd::SetVolume { id, volume } => {
                music_fades.retain(|f| f.id != id);
                music_volumes.insert(id, volume);
                if let Some(Some(m)) = music.get(id) {
                    m.set_volume(volume);
                }
            }
            MusicCmd::SetPitch { id, pitch } => {
                if let Some(Some(m)) = music.get(id) {
                    m.set_pitch(pitch);
                }
            }
            MusicCmd::SetPan { id, pan } => {
                if let Some(Some(m)) = music.get(id) {
                    m.set_pan(pan);
                }
            }
            MusicCmd::Fade { id, target, seconds } => {
                let start = music_volumes.get(&id).copied().unwrap_or(1.0);
                music_fades.retain(|f| f.id != id);
                if seconds <= 0.0 {
                    music_volumes.insert(id, target);
                    if let Some(Some(m)) = music.get(id) {
                        m.set_volume(target);
                    }
                } else {
                    music_fades.push(MusicFade {
                        id,
                        start,
                        target,
                        elapsed: 0.0,
                        duration: seconds,
                    });
                }
            }
        }
    }
}

fn tick_audio(
    dt: f32,
    sounds: &[Option<Sound<'static>>],
    music: &mut [Option<Music<'static>>],
    looping_sounds: &HashMap<usize, bool>,
    music_volumes: &mut HashMap<usize, f32>,
    music_fades: &mut Vec<MusicFade>,
) {
    // Restart finished looping sound effects.
    for (&id, &looping) in looping_sounds.iter() {
        if !looping {
            continue;
        }
        if let Some(Some(s)) = sounds.get(id) {
            if !s.is_playing() {
                s.play();
            }
        }
    }

    // Keep streamed music buffers filled.
    for slot in music.iter_mut() {
        if let Some(m) = slot.as_mut() {
            if m.is_stream_playing() {
                m.update_stream();
            }
        }
    }

    // Advance volume fades.
    let mut i = 0;
    while i < music_fades.len() {
        let fade = &mut music_fades[i];
        fade.elapsed += dt;
        let (vol, done) = fade_volume(fade.start, fade.target, fade.elapsed, fade.duration);
        music_volumes.insert(fade.id, vol);
        if let Some(Some(m)) = music.get(fade.id) {
            m.set_volume(vol);
        }
        if done {
            music_fades.remove(i);
        } else {
            i += 1;
        }
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
    load_pending(
        &mut rl,
        &thread,
        None,
        &bridge,
        &mut textures,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut fonts,
    );

    // UI state that lives across frames but not in the program. Indices into
    // the per-frame `controls` list stay stable while the widget tree shape does.
    let mut focused: Option<usize> = None;
    let mut dragging: Option<usize> = None;
    let mut caret: usize = 0;
    let mut scrolls: HashMap<usize, f32> = HashMap::new();
    let mut open_dropdown: Option<usize> = None;

    while !rl.window_should_close() {
        sync_input(&mut bridge.borrow_mut(), &rl);
        load_pending(
            &mut rl,
            &thread,
            None,
            &bridge,
            &mut textures,
            &mut Vec::new(),
            &mut Vec::new(),
            &mut fonts,
        );

        let mut typed = String::new();
        while let Some(ch) = rl.get_char_pressed() {
            typed.push(ch);
        }
        let ctrl = rl.is_key_down(KeyboardKey::KEY_LEFT_CONTROL)
            || rl.is_key_down(KeyboardKey::KEY_RIGHT_CONTROL);
        let shift = rl.is_key_down(KeyboardKey::KEY_LEFT_SHIFT)
            || rl.is_key_down(KeyboardKey::KEY_RIGHT_SHIFT);
        let edit = TextEdit {
            backspace: key_repeat(&rl, KeyboardKey::KEY_BACKSPACE),
            delete: key_repeat(&rl, KeyboardKey::KEY_DELETE),
            left: key_repeat(&rl, KeyboardKey::KEY_LEFT),
            right: key_repeat(&rl, KeyboardKey::KEY_RIGHT),
            up: key_repeat(&rl, KeyboardKey::KEY_UP),
            down: key_repeat(&rl, KeyboardKey::KEY_DOWN),
            home: rl.is_key_pressed(KeyboardKey::KEY_HOME),
            end: rl.is_key_pressed(KeyboardKey::KEY_END),
            enter: rl.is_key_pressed(KeyboardKey::KEY_ENTER)
                || rl.is_key_pressed(KeyboardKey::KEY_KP_ENTER),
            paste: ctrl && rl.is_key_pressed(KeyboardKey::KEY_V),
            clipboard: rl.get_clipboard_text().unwrap_or_default(),
        };
        if ctrl {
            typed.clear();
        }

        let wheel = rl.get_mouse_wheel_move();
        let tab = rl.is_key_pressed(KeyboardKey::KEY_TAB);

        let (mouse, pressed, down) = {
            let b = bridge.borrow();
            ((b.mouse_x, b.mouse_y), b.mouse_pressed, b.mouse_down)
        };

        let mut nodes = interp.build_widgets(&window.root, &scope)?;
        ui::layout_root(&mut nodes, width, height);
        let mut cmds = Vec::new();
        let mut controls = Vec::new();
        let draw_state = ui::UiDrawState {
            focused,
            caret,
            scrolls: &scrolls,
            open_dropdown,
        };
        ui::collect_frame(&mut nodes, mouse, &draw_state, &mut cmds, &mut controls);

        if !down {
            dragging = None;
        }

        if wheel != 0.0 {
            if let Some(i) = controls
                .iter()
                .enumerate()
                .rev()
                .find(|(_, c)| {
                    point_in(mouse, c)
                        && matches!(
                            c.kind,
                            ui::ControlKind::Scroll | ui::ControlKind::List | ui::ControlKind::Dropdown
                        )
                })
                .map(|(i, _)| i)
            {
                let c = &controls[i];
                let view_h = match c.kind {
                    ui::ControlKind::Dropdown if c.checked => (c.h - 34.0).max(1.0),
                    _ => c.h,
                };
                let cur = scrolls.get(&i).copied().unwrap_or(0.0);
                let next = ui::clamp_scroll(cur - wheel * 24.0, c.content_h, view_h);
                scrolls.insert(i, next);
            }
        }

        if tab {
            let focusable: Vec<usize> = controls
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    matches!(
                        c.kind,
                        ui::ControlKind::TextField
                            | ui::ControlKind::List
                            | ui::ControlKind::Dropdown
                            | ui::ControlKind::Checkbox
                            | ui::ControlKind::Slider
                    )
                })
                .map(|(i, _)| i)
                .collect();
            if !focusable.is_empty() {
                let pos = focused.and_then(|f| focusable.iter().position(|&i| i == f));
                let next = if shift {
                    match pos {
                        Some(0) | None => *focusable.last().unwrap(),
                        Some(p) => focusable[p - 1],
                    }
                } else {
                    match pos {
                        Some(p) if p + 1 < focusable.len() => focusable[p + 1],
                        _ => focusable[0],
                    }
                };
                focused = Some(next);
                caret = controls.get(next).map(|c| c.text.chars().count()).unwrap_or(0);
                if controls[next].kind != ui::ControlKind::Dropdown {
                    open_dropdown = None;
                }
            }
        }

        if pressed {
            let hit = controls
                .iter()
                .enumerate()
                .rev()
                .find(|(_, c)| point_in(mouse, c))
                .map(|(i, _)| i);

            if let Some(open_i) = open_dropdown {
                if hit != Some(open_i) {
                    open_dropdown = None;
                }
            }

            focused = None;
            if let Some(i) = hit {
                match controls[i].kind {
                    ui::ControlKind::Button => {
                        open_dropdown = None;
                        if let Some(cb) = controls[i].callback.clone() {
                            interp.call_callback(&cb)?;
                        }
                    }
                    ui::ControlKind::TextField => {
                        open_dropdown = None;
                        focused = Some(i);
                        caret = if controls[i].multiline {
                            caret_from_xy(
                                &controls[i],
                                mouse,
                                scrolls.get(&i).copied().unwrap_or(0.0),
                            )
                        } else {
                            caret_from_x(&controls[i], mouse.0)
                        };
                    }
                    ui::ControlKind::Checkbox => {
                        open_dropdown = None;
                        focused = Some(i);
                        let new_val = !controls[i].checked;
                        write_back(&mut interp, &scope, &controls[i], Value::Bool(new_val))?;
                    }
                    ui::ControlKind::Slider => {
                        open_dropdown = None;
                        focused = Some(i);
                        dragging = Some(i);
                        if let Some(v) = slider_value(&controls[i], mouse.0) {
                            write_back(&mut interp, &scope, &controls[i], Value::Number(v))?;
                        }
                    }
                    ui::ControlKind::Scroll => {
                        open_dropdown = None;
                    }
                    ui::ControlKind::List => {
                        open_dropdown = None;
                        focused = Some(i);
                        let scroll = scrolls.get(&i).copied().unwrap_or(0.0);
                        if let Some(row) = ui::list_row_at(&controls[i], mouse.1, scroll, 0.0) {
                            write_back(&mut interp, &scope, &controls[i], Value::Number(row as f64))?;
                        }
                    }
                    ui::ControlKind::Dropdown => {
                        focused = Some(i);
                        let was_open = open_dropdown == Some(i);
                        if was_open {
                            let scroll = scrolls.get(&i).copied().unwrap_or(0.0);
                            let header_h = 34.0;
                            if let Some(row) = ui::list_row_at(&controls[i], mouse.1, scroll, header_h) {
                                write_back(
                                    &mut interp,
                                    &scope,
                                    &controls[i],
                                    Value::Number(row as f64),
                                )?;
                                open_dropdown = None;
                            } else if mouse.1 <= controls[i].y + header_h {
                                open_dropdown = None;
                            }
                        } else {
                            open_dropdown = Some(i);
                        }
                    }
                }
            }
        }

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

        if let Some(i) = focused {
            if let Some(c) = controls.get(i).cloned() {
                if c.kind == ui::ControlKind::TextField {
                    let mut chars: Vec<char> = c.text.chars().collect();
                    let mut pos = caret.min(chars.len());
                    let mut changed = false;
                    let max_w = c.w - 16.0;

                    if edit.left {
                        pos = pos.saturating_sub(1);
                    }
                    if edit.right {
                        pos = (pos + 1).min(chars.len());
                    }
                    if c.multiline && edit.up {
                        let s: String = chars.iter().collect();
                        pos = ui::move_caret_vertical(&s, pos, max_w, c.font_size, true);
                    }
                    if c.multiline && edit.down {
                        let s: String = chars.iter().collect();
                        pos = ui::move_caret_vertical(&s, pos, max_w, c.font_size, false);
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
                    if c.multiline && edit.enter {
                        chars.insert(pos, '\n');
                        pos += 1;
                        changed = true;
                    }
                    if edit.paste {
                        for ch in edit.clipboard.chars() {
                            if ch == '\n' || ch == '\r' {
                                if c.multiline && ch == '\n' {
                                    chars.insert(pos, '\n');
                                    pos += 1;
                                    changed = true;
                                }
                                continue;
                            }
                            if ch.is_control() {
                                continue;
                            }
                            chars.insert(pos, ch);
                            pos += 1;
                            changed = true;
                        }
                    }
                    for ch in typed.chars() {
                        if ch == '\n' || ch == '\r' {
                            continue;
                        }
                        chars.insert(pos, ch);
                        pos += 1;
                        changed = true;
                    }

                    caret = pos;
                    if c.multiline {
                        let s: String = chars.iter().collect();
                        let line_h = c.font_size as f32 + 4.0;
                        let (li, _) = ui::caret_line_col(&s, caret, max_w, c.font_size);
                        let cur = scrolls.get(&i).copied().unwrap_or(0.0);
                        let caret_y = li as f32 * line_h;
                        let view = (c.h - 8.0).max(line_h);
                        let mut next = cur;
                        if caret_y < cur {
                            next = caret_y;
                        } else if caret_y + line_h > cur + view {
                            next = caret_y + line_h - view;
                        }
                        let content_h =
                            ui::wrap_lines_with_breaks(&s, max_w, c.font_size).len() as f32 * line_h;
                        scrolls.insert(i, ui::clamp_scroll(next, content_h, view));
                    }
                    if changed {
                        let s: String = chars.iter().collect();
                        write_back(&mut interp, &scope, &c, Value::text(s))?;
                    }
                }
            } else {
                focused = None;
            }
        }

        let mut d = rl.begin_drawing(&thread);
        d.clear_background(bg);
        render_cmds(&mut d, &cmds, &textures, &fonts, 0.0, 0.0);
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
    up: bool,
    down: bool,
    home: bool,
    end: bool,
    enter: bool,
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

/// Map a click to a caret index in a multiline field.
fn caret_from_xy(c: &ui::Control, mouse: (f32, f32), scroll: f32) -> usize {
    let line_h = c.font_size as f32 + 4.0;
    let max_w = c.w - 16.0;
    let lines = ui::wrap_lines_with_breaks(&c.text, max_w, c.font_size);
    let rel_y = (mouse.1 - (c.y + 6.0) + scroll).max(0.0);
    let li = ((rel_y / line_h).floor() as usize).min(lines.len().saturating_sub(1));
    let em = (c.font_size as f32 * 0.5).max(1.0);
    let rel_x = (mouse.0 - (c.x + 8.0)).max(0.0);
    let col = ((rel_x / em).round() as usize).min(lines[li].0.chars().count());
    let mut at = 0usize;
    for (i, (_, consumed)) in lines.iter().enumerate() {
        if i == li {
            return at + col;
        }
        at += consumed;
    }
    c.text.chars().count()
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

fn render_cmds(
    d: &mut RaylibDrawHandle,
    cmds: &[DrawCmd],
    textures: &[Option<Texture2D>],
    fonts: &[Option<Font>],
    cam_x: f32,
    cam_y: f32,
) {
    let mut i = 0;
    while i < cmds.len() {
        match &cmds[i] {
            DrawCmd::ScissorBegin { x, y, w, h } => {
                i += 1;
                let mut end = i;
                while end < cmds.len() && !matches!(cmds[end], DrawCmd::ScissorEnd) {
                    end += 1;
                }
                d.draw_scissor_mode(*x, *y, *w, *h, |mut sd| {
                    for cmd in &cmds[i..end] {
                        render_one(&mut sd, cmd, textures, fonts, cam_x, cam_y);
                    }
                });
                i = end + if end < cmds.len() { 1 } else { 0 };
            }
            DrawCmd::ScissorEnd => i += 1,
            other => {
                render_one(d, other, textures, fonts, cam_x, cam_y);
                i += 1;
            }
        }
    }
}

fn render_one(
    d: &mut impl RaylibDraw,
    cmd: &DrawCmd,
    textures: &[Option<Texture2D>],
    fonts: &[Option<Font>],
    cam_x: f32,
    cam_y: f32,
) {
    match cmd {
        DrawCmd::Clear(c) => d.clear_background(to_rl(*c)),
        DrawCmd::Circle { x, y, r, color } => {
            d.draw_circle_v(Vector2::new(*x - cam_x, *y - cam_y), *r, to_rl(*color));
        }
        DrawCmd::Rect { x, y, w, h, color } => {
            d.draw_rectangle_rec(Rectangle::new(*x - cam_x, *y - cam_y, *w, *h), to_rl(*color));
        }
        DrawCmd::Line { x1, y1, x2, y2, thick, color } => {
            d.draw_line_ex(
                Vector2::new(*x1 - cam_x, *y1 - cam_y),
                Vector2::new(*x2 - cam_x, *y2 - cam_y),
                *thick,
                to_rl(*color),
            );
        }
        DrawCmd::Text { text, x, y, size, color, font } => {
            let sx = *x - cam_x;
            let sy = *y - cam_y;
            if let Some(id) = font {
                if let Some(Some(f)) = fonts.get(*id) {
                    d.draw_text_ex(f, text, Vector2::new(sx, sy), *size as f32, 1.0, to_rl(*color));
                    return;
                }
            }
            d.draw_text(text, sx as i32, sy as i32, *size, to_rl(*color));
        }
        DrawCmd::ScreenText { text, x, y, size, color, font } => {
            if let Some(id) = font {
                if let Some(Some(f)) = fonts.get(*id) {
                    d.draw_text_ex(f, text, Vector2::new(*x, *y), *size as f32, 1.0, to_rl(*color));
                    return;
                }
            }
            d.draw_text(text, *x as i32, *y as i32, *size, to_rl(*color));
        }
        DrawCmd::ScreenRect { x, y, w, h, color } => {
            d.draw_rectangle_rec(Rectangle::new(*x, *y, *w, *h), to_rl(*color));
        }
        DrawCmd::Sprite { id, x, y, scale, rotation } => {
            let sx = *x - cam_x;
            let sy = *y - cam_y;
            if let Some(Some(tex)) = textures.get(*id) {
                if *rotation == 0.0 {
                    d.draw_texture_ex(tex, Vector2::new(sx, sy), 0.0, *scale, Color::WHITE);
                } else {
                    let w = tex.width() as f32;
                    let h = tex.height() as f32;
                    let src = Rectangle::new(0.0, 0.0, w, h);
                    let dst = Rectangle::new(sx, sy, w * scale, h * scale);
                    let origin = Vector2::new(w * scale / 2.0, h * scale / 2.0);
                    d.draw_texture_pro(tex, src, dst, origin, *rotation, Color::WHITE);
                }
            }
        }
        DrawCmd::SpriteRect { id, x, y, w, h } => {
            let sx = *x - cam_x;
            let sy = *y - cam_y;
            if let Some(Some(tex)) = textures.get(*id) {
                let src = Rectangle::new(0.0, 0.0, tex.width() as f32, tex.height() as f32);
                let dst = Rectangle::new(sx, sy, *w, *h);
                d.draw_texture_pro(tex, src, dst, Vector2::zero(), 0.0, Color::WHITE);
            }
        }
        DrawCmd::SpriteFrame {
            id,
            frame,
            cell_w,
            cell_h,
            x,
            y,
            scale,
            flip_x,
        } => {
            let sx = *x - cam_x;
            let sy = *y - cam_y;
            if let Some(Some(tex)) = textures.get(*id) {
                let Some((src_x, src_y, mut src_w, src_h)) =
                    crate::gfx::sheet_frame_src(tex.width(), tex.height(), *cell_w, *cell_h, *frame)
                else {
                    return;
                };
                if *flip_x {
                    src_w = -src_w;
                }
                let src = Rectangle::new(src_x, src_y, src_w, src_h);
                let dw = (*cell_w as f32) * scale;
                let dh = (*cell_h as f32) * scale;
                let dst = Rectangle::new(sx, sy, dw, dh);
                d.draw_texture_pro(tex, src, dst, Vector2::zero(), 0.0, Color::WHITE);
            }
        }
        DrawCmd::ScissorBegin { .. } | DrawCmd::ScissorEnd => {}
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
            items: Vec::new(),
            multiline: false,
            content_h: 0.0,
            row_h: 0.0,
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
