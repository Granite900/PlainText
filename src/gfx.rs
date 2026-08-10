//! The bridge between the interpreter and the Raylib window.
//!
//! The interpreter never touches Raylib directly. Instead, drawing builtins
//! push [`DrawCmd`]s into the shared [`GfxBridge`], and `game.rs` replays them
//! onto the real window each frame. Input flows the other way: `game.rs` fills
//! in the key/mouse state each frame, and input builtins read it. Keeping this
//! type Raylib-free means `interpreter.rs` has no Raylib dependency.

use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy)]
pub struct Color(pub u8, pub u8, pub u8, pub u8);

/// Named colors predeclared as globals in every program (as `[r, g, b]` lists).
pub fn named_colors() -> &'static [(&'static str, (u8, u8, u8))] {
    &[
        ("black", (0, 0, 0)),
        ("white", (255, 255, 255)),
        ("red", (230, 41, 55)),
        ("green", (0, 175, 80)),
        ("blue", (0, 121, 241)),
        ("yellow", (253, 249, 0)),
        ("orange", (255, 161, 0)),
        ("purple", (135, 60, 190)),
        ("pink", (255, 109, 194)),
        ("brown", (127, 106, 79)),
        ("gray", (130, 130, 130)),
        ("grey", (130, 130, 130)),
        ("darkgray", (80, 80, 80)),
        ("lightgray", (200, 200, 200)),
        ("skyblue", (102, 191, 255)),
        ("gold", (255, 203, 0)),
    ]
}

pub enum DrawCmd {
    Clear(Color),
    Circle { x: f32, y: f32, r: f32, color: Color },
    Rect { x: f32, y: f32, w: f32, h: f32, color: Color },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, thick: f32, color: Color },
    Text { text: String, x: f32, y: f32, size: i32, color: Color, font: Option<usize> },
    /// Draw a loaded sprite. `x`/`y` is the top-left; `rotation` is in degrees
    /// about the sprite's center.
    Sprite { id: usize, x: f32, y: f32, scale: f32, rotation: f32 },
    /// Draw a sprite stretched to a destination rectangle (used by UI buttons).
    SpriteRect { id: usize, x: f32, y: f32, w: f32, h: f32 },
    /// Clip following draws to this screen rectangle until [`DrawCmd::ScissorEnd`].
    ScissorBegin { x: i32, y: i32, w: i32, h: i32 },
    ScissorEnd,
}

/// Queued sound-effect commands (ids from [`GfxBridge::queue_sound`]).
#[derive(Clone, Debug)]
pub enum SoundCmd {
    Play { id: usize, looping: bool },
    Stop(usize),
    SetVolume { id: usize, volume: f32 },
    SetPitch { id: usize, pitch: f32 },
    SetPan { id: usize, pan: f32 },
}

/// Queued streamed-music commands (ids from [`GfxBridge::queue_music`]).
#[derive(Clone, Debug)]
pub enum MusicCmd {
    Play(usize),
    Stop(usize),
    SetVolume { id: usize, volume: f32 },
    SetPitch { id: usize, pitch: f32 },
    SetPan { id: usize, pan: f32 },
    Fade { id: usize, target: f32, seconds: f32 },
}

/// Linear volume fade: returns `(volume, finished)`.
pub fn fade_volume(start: f32, target: f32, elapsed: f32, duration: f32) -> (f32, bool) {
    if !(duration > 0.0) {
        return (target, true);
    }
    let t = (elapsed / duration).clamp(0.0, 1.0);
    (start + (target - start) * t, t >= 1.0)
}

/// Shared mutable state between the interpreter and the frame loop.
pub struct GfxBridge {
    pub screen_w: i32,
    pub screen_h: i32,
    /// Draw commands accumulated during the current `on draw()` hook.
    pub draw: Vec<DrawCmd>,
    /// Keys held this frame / newly pressed this frame (PlainText names).
    pub keys_down: HashSet<String>,
    pub keys_pressed: HashSet<String>,
    pub mouse_x: f32,
    pub mouse_y: f32,
    pub mouse_down: bool,
    pub mouse_pressed: bool,
    // Assets. The interpreter allocates ids and queues load requests; the game
    // runner (which owns the Raylib context) fulfills them and reports sizes.
    next_sprite_id: usize,
    next_sound_id: usize,
    next_music_id: usize,
    pub sprite_loads: Vec<(usize, String)>,
    pub sprite_sizes: HashMap<usize, (i32, i32)>,
    pub sound_loads: Vec<(usize, String)>,
    pub sound_cmds: Vec<SoundCmd>,
    pub music_loads: Vec<(usize, String)>,
    pub music_cmds: Vec<MusicCmd>,
    next_font_id: usize,
    pub font_loads: Vec<(usize, String)>,
}

impl GfxBridge {
    pub fn new(w: i32, h: i32) -> GfxBridge {
        GfxBridge {
            screen_w: w,
            screen_h: h,
            draw: Vec::new(),
            keys_down: HashSet::new(),
            keys_pressed: HashSet::new(),
            mouse_x: 0.0,
            mouse_y: 0.0,
            mouse_down: false,
            mouse_pressed: false,
            next_sprite_id: 0,
            next_sound_id: 0,
            next_music_id: 0,
            sprite_loads: Vec::new(),
            sprite_sizes: HashMap::new(),
            sound_loads: Vec::new(),
            sound_cmds: Vec::new(),
            music_loads: Vec::new(),
            music_cmds: Vec::new(),
            next_font_id: 0,
            font_loads: Vec::new(),
        }
    }

    /// Reserve a sprite id and queue its file for loading.
    pub fn queue_sprite(&mut self, path: String) -> usize {
        let id = self.next_sprite_id;
        self.next_sprite_id += 1;
        self.sprite_loads.push((id, path));
        id
    }

    /// Reserve a sound id and queue its file for loading.
    pub fn queue_sound(&mut self, path: String) -> usize {
        let id = self.next_sound_id;
        self.next_sound_id += 1;
        self.sound_loads.push((id, path));
        id
    }

    /// Reserve a music id and queue its file for streamed loading.
    pub fn queue_music(&mut self, path: String) -> usize {
        let id = self.next_music_id;
        self.next_music_id += 1;
        self.music_loads.push((id, path));
        id
    }

    /// Reserve a font id and queue its file for loading.
    pub fn queue_font(&mut self, path: String) -> usize {
        let id = self.next_font_id;
        self.next_font_id += 1;
        self.font_loads.push((id, path));
        id
    }
}

#[cfg(test)]
mod tests {
    use super::fade_volume;

    #[test]
    fn fade_reaches_target() {
        let (v, done) = fade_volume(0.0, 1.0, 1.0, 1.0);
        assert!((v - 1.0).abs() < 1e-5);
        assert!(done);
    }

    #[test]
    fn fade_midpoint() {
        let (v, done) = fade_volume(0.2, 0.8, 0.5, 1.0);
        assert!((v - 0.5).abs() < 1e-5);
        assert!(!done);
    }

    #[test]
    fn fade_zero_duration_snaps() {
        let (v, done) = fade_volume(0.9, 0.1, 0.0, 0.0);
        assert!((v - 0.1).abs() < 1e-5);
        assert!(done);
    }
}
