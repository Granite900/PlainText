//! The bridge between the interpreter and the Raylib window.
//!
//! The interpreter never touches Raylib directly. Instead, drawing builtins
//! push [`DrawCmd`]s into the shared [`GfxBridge`], and `game.rs` replays them
//! onto the real window each frame. Input flows the other way: `game.rs` fills
//! in the key/mouse state each frame, and input builtins read it. Keeping this
//! type Raylib-free means `interpreter.rs` has no Raylib dependency.

use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug)]
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
    /// One cell from a sprite sheet (row-major frame index).
    SpriteFrame {
        id: usize,
        frame: i32,
        cell_w: i32,
        cell_h: i32,
        x: f32,
        y: f32,
        scale: f32,
        flip_x: bool,
    },
    /// HUD text — screen space (ignores the camera).
    ScreenText { text: String, x: f32, y: f32, size: i32, color: Color, font: Option<usize> },
    /// HUD rectangle — screen space (ignores the camera).
    ScreenRect { x: f32, y: f32, w: f32, h: f32, color: Color },
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

/// Source rectangle for a row-major sprite-sheet frame.
/// Returns `None` when the frame is out of range or the cell size is invalid.
pub fn sheet_frame_src(
    tex_w: i32,
    tex_h: i32,
    cell_w: i32,
    cell_h: i32,
    frame: i32,
) -> Option<(f32, f32, f32, f32)> {
    if cell_w <= 0 || cell_h <= 0 || frame < 0 || tex_w < cell_w || tex_h < cell_h {
        return None;
    }
    let cols = tex_w / cell_w;
    let rows = tex_h / cell_h;
    if cols <= 0 || rows <= 0 {
        return None;
    }
    let count = cols * rows;
    if frame >= count {
        return None;
    }
    let col = frame % cols;
    let row = frame / cols;
    Some((
        (col * cell_w) as f32,
        (row * cell_h) as f32,
        cell_w as f32,
        cell_h as f32,
    ))
}

/// How many cells fit in a texture for the given cell size.
pub fn sheet_frame_count(tex_w: i32, tex_h: i32, cell_w: i32, cell_h: i32) -> i32 {
    if cell_w <= 0 || cell_h <= 0 || tex_w < cell_w || tex_h < cell_h {
        return 0;
    }
    (tex_w / cell_w) * (tex_h / cell_h)
}

/// Clamp camera top-left so the view stays inside a world rectangle
/// `(min_x, min_y, max_x, max_y)`. If the world is smaller than the screen on an
/// axis, the camera sticks to that axis's minimum.
pub fn clamp_camera(
    cam_x: f32,
    cam_y: f32,
    screen_w: i32,
    screen_h: i32,
    bounds: Option<(f32, f32, f32, f32)>,
) -> (f32, f32) {
    let Some((min_x, min_y, max_x, max_y)) = bounds else {
        return (cam_x, cam_y);
    };
    let sw = screen_w as f32;
    let sh = screen_h as f32;
    let max_cam_x = (max_x - sw).max(min_x);
    let max_cam_y = (max_y - sh).max(min_y);
    (cam_x.clamp(min_x, max_cam_x), cam_y.clamp(min_y, max_cam_y))
}

/// One short-lived particle from [`spawn_burst`].
#[derive(Clone, Debug)]
pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub life: f32,
    pub max_life: f32,
    pub color: Color,
    pub size: f32,
}

/// Spawn a radial burst of particles (world space).
pub fn spawn_burst(
    out: &mut Vec<Particle>,
    x: f32,
    y: f32,
    color: Color,
    count: i32,
    speed: f32,
    life: f32,
) {
    let n = count.max(0) as usize;
    if n == 0 || !(life > 0.0) {
        return;
    }
    let tau = std::f32::consts::TAU;
    for i in 0..n {
        let angle = (i as f32 / n as f32) * tau + 0.35;
        let spd = speed * (0.55 + (i % 5) as f32 * 0.1);
        out.push(Particle {
            x,
            y,
            vx: angle.cos() * spd,
            vy: angle.sin() * spd - speed * 0.25,
            life,
            max_life: life,
            color,
            size: 3.0 + (i % 3) as f32,
        });
    }
}

/// Advance particles; remove dead ones. Returns draw cmds in world space.
pub fn tick_particles(particles: &mut Vec<Particle>, dt: f32) -> Vec<DrawCmd> {
    let mut cmds = Vec::new();
    let mut i = 0;
    while i < particles.len() {
        let p = &mut particles[i];
        p.life -= dt;
        if p.life <= 0.0 {
            particles.swap_remove(i);
            continue;
        }
        p.vy += 400.0 * dt; // light gravity
        p.x += p.vx * dt;
        p.y += p.vy * dt;
        let t = (p.life / p.max_life).clamp(0.0, 1.0);
        let a = (p.color.3 as f32 * t) as u8;
        cmds.push(DrawCmd::Circle {
            x: p.x,
            y: p.y,
            r: p.size * (0.4 + 0.6 * t),
            color: Color(p.color.0, p.color.1, p.color.2, a),
        });
        i += 1;
    }
    cmds
}

/// Shared mutable state between the interpreter and the frame loop.
pub struct GfxBridge {
    pub screen_w: i32,
    pub screen_h: i32,
    /// Top-left of the camera in world space. World draws subtract this.
    pub camera_x: f32,
    pub camera_y: f32,
    /// Optional world rectangle the camera view must stay inside.
    pub camera_bounds: Option<(f32, f32, f32, f32)>,
    /// Live particles from `burst(...)`.
    pub particles: Vec<Particle>,
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
    /// Cell size for sprite-sheet ids (`cell_width`, `cell_height`).
    pub sheet_meta: HashMap<usize, (i32, i32)>,
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
            camera_x: 0.0,
            camera_y: 0.0,
            camera_bounds: None,
            particles: Vec::new(),
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
            sheet_meta: HashMap::new(),
            sound_loads: Vec::new(),
            sound_cmds: Vec::new(),
            music_loads: Vec::new(),
            music_cmds: Vec::new(),
            next_font_id: 0,
            font_loads: Vec::new(),
        }
    }

    /// Apply [`camera_bounds`](Self::camera_bounds) to the current camera.
    pub fn apply_camera_bounds(&mut self) {
        let (x, y) = clamp_camera(
            self.camera_x,
            self.camera_y,
            self.screen_w,
            self.screen_h,
            self.camera_bounds,
        );
        self.camera_x = x;
        self.camera_y = y;
    }

    /// Reserve a sprite id and queue its file for loading.
    pub fn queue_sprite(&mut self, path: String) -> usize {
        let id = self.next_sprite_id;
        self.next_sprite_id += 1;
        self.sprite_loads.push((id, path));
        id
    }

    /// Reserve a sprite-sheet id (same texture id space) with cell metadata.
    pub fn queue_sprite_sheet(&mut self, path: String, cell_w: i32, cell_h: i32) -> usize {
        let id = self.queue_sprite(path);
        self.sheet_meta.insert(id, (cell_w, cell_h));
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
    use super::{
        clamp_camera, fade_volume, sheet_frame_count, sheet_frame_src, spawn_burst, Color,
    };

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

    #[test]
    fn sheet_frame_layout_row_major() {
        // 128×32 texture, 32×32 cells → 4 frames in one row.
        assert_eq!(sheet_frame_count(128, 32, 32, 32), 4);
        assert_eq!(sheet_frame_src(128, 32, 32, 32, 0), Some((0.0, 0.0, 32.0, 32.0)));
        assert_eq!(sheet_frame_src(128, 32, 32, 32, 2), Some((64.0, 0.0, 32.0, 32.0)));
        assert_eq!(sheet_frame_src(128, 32, 32, 32, 4), None);
        assert_eq!(sheet_frame_src(128, 32, 32, 32, -1), None);
    }

    #[test]
    fn sheet_frame_wraps_rows() {
        // 64×64, 32×32 → 2×2 grid.
        assert_eq!(sheet_frame_count(64, 64, 32, 32), 4);
        assert_eq!(sheet_frame_src(64, 64, 32, 32, 3), Some((32.0, 32.0, 32.0, 32.0)));
    }

    #[test]
    fn camera_bounds_clamp_edges() {
        let bounds = Some((0.0, 0.0, 1000.0, 600.0));
        // Screen 640×360 → max cam (360, 240).
        assert_eq!(clamp_camera(-50.0, -10.0, 640, 360, bounds), (0.0, 0.0));
        assert_eq!(clamp_camera(900.0, 500.0, 640, 360, bounds), (360.0, 240.0));
        assert_eq!(clamp_camera(100.0, 50.0, 640, 360, bounds), (100.0, 50.0));
    }

    #[test]
    fn camera_bounds_small_world() {
        let bounds = Some((0.0, 0.0, 200.0, 100.0));
        // World smaller than screen → stick to min.
        assert_eq!(clamp_camera(50.0, 50.0, 640, 360, bounds), (0.0, 0.0));
    }

    #[test]
    fn burst_spawns_count() {
        let mut ps = Vec::new();
        spawn_burst(&mut ps, 0.0, 0.0, Color(255, 0, 0, 255), 12, 120.0, 0.4);
        assert_eq!(ps.len(), 12);
        spawn_burst(&mut ps, 0.0, 0.0, Color(255, 0, 0, 255), 0, 120.0, 0.4);
        assert_eq!(ps.len(), 12);
    }
}
