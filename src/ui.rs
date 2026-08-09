//! The UI layout + draw engine for `window` blocks.
//!
//! It's Raylib-free (like `gfx.rs`): the interpreter turns the widget AST into
//! a tree of [`UiNode`]s each frame, this module lays them out and emits
//! [`DrawCmd`]s plus a list of clickable [`Button`]s, and `game.rs` renders the
//! commands and dispatches clicks. Layout is a small flexbox-lite: `column`
//! stacks children vertically, `row` horizontally, with `padding`, `spacing`,
//! and cross-axis `align`.

use crate::gfx::{Color, DrawCmd};
use crate::value::Value;

/// Bare words usable as `align:` values, predeclared as Text globals so
/// `align: center` reads without quotes (like named colors).
pub fn align_words() -> &'static [&'static str] {
    &["center", "left", "right", "top", "bottom", "middle"]
}

// Default theme.
const DEFAULT_FONT: i32 = 20;
const TEXT_COLOR: Color = Color(30, 30, 40, 255);
const BUTTON_BG: Color = Color(70, 120, 220, 255);
const BUTTON_HOVER: Color = Color(90, 145, 245, 255);
const BUTTON_TEXT: Color = Color(255, 255, 255, 255);
const BTN_PAD_X: f32 = 16.0;
const BTN_PAD_Y: f32 = 10.0;
// Shared accent/field theme for the interactive widgets.
const ACCENT: Color = Color(70, 120, 220, 255);
const FIELD_BG: Color = Color(255, 255, 255, 255);
const FIELD_BORDER: Color = Color(170, 170, 180, 255);
const TRACK: Color = Color(200, 200, 208, 255);
const FIELD_H: f32 = 34.0;
const FIELD_W: f32 = 220.0;
const BOX: f32 = 22.0; // checkbox square side
const SLIDER_H: f32 = 24.0;

#[derive(Clone, Copy, PartialEq)]
pub enum UiKind {
    Column,
    Row,
    Text,
    Button,
    Spacer,
    TextField,
    Checkbox,
    Slider,
    Image,
}

#[derive(Clone, Copy)]
pub enum Align {
    Start,
    Center,
    End,
}

pub struct UiProps {
    pub padding: f32,
    pub spacing: f32,
    pub align: Align,
    pub font_size: i32,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub color: Option<Color>,
    pub bg: Option<Color>,
}

impl Default for UiProps {
    fn default() -> UiProps {
        UiProps {
            padding: 0.0,
            spacing: 0.0,
            align: Align::Start,
            font_size: DEFAULT_FONT,
            width: None,
            height: None,
            color: None,
            bg: None,
        }
    }
}

pub struct UiNode {
    pub kind: UiKind,
    pub text: Option<String>,
    pub props: UiProps,
    pub children: Vec<UiNode>,
    pub callback: Option<Value>,
    /// Optional sprite drawn as the button/image face (stretched to the rect).
    pub sprite: Option<usize>,
    /// Optional custom font id from `load_font`.
    pub font: Option<usize>,
    // Interactive-widget state (read from the program's variables each frame):
    /// Checkbox on/off.
    pub checked: bool,
    /// Slider current value and its range/step.
    pub number: f32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    /// `on_change` handler for text_field / checkbox / slider.
    pub on_change: Option<Value>,
    /// Name of a variable to write back to (the `bind:` shorthand).
    pub bind: Option<String>,
    // Filled in by layout:
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl UiNode {
    pub fn new(kind: UiKind) -> UiNode {
        UiNode {
            kind,
            text: None,
            props: UiProps::default(),
            children: Vec::new(),
            callback: None,
            sprite: None,
            font: None,
            checked: false,
            number: 0.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            on_change: None,
            bind: None,
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        }
    }
}

/// What kind of interactive region a [`Control`] is, so the window runner knows
/// how to respond to input.
#[derive(Clone, Copy, PartialEq)]
pub enum ControlKind {
    Button,
    Checkbox,
    Slider,
    TextField,
}

/// A laid-out interactive region: a button, checkbox, slider, or text field.
/// The window runner hit-tests these against the mouse/keyboard and writes any
/// change back to the program (via `bind` and/or `callback`).
pub struct Control {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub kind: ControlKind,
    /// Button `on_click`, or `on_change` for the stateful widgets.
    pub callback: Option<Value>,
    pub bind: Option<String>,
    pub checked: bool,
    pub number: f32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub text: String,
}

/// Approximate width of a string in Raylib's default font at `size`. Raylib
/// isn't available here, so we estimate ~0.5em per character — good enough for
/// left/center layout without a measurement round-trip.
fn text_width(text: &str, size: i32) -> f32 {
    text.chars().count() as f32 * size as f32 * 0.5
}

/// Lay out the root widgets, centered as a vertical stack in the window.
pub fn layout_root(roots: &mut [UiNode], screen_w: i32, screen_h: i32) {
    let mut total_h = 0.0;
    let mut max_w = 0.0f32;
    for (i, n) in roots.iter_mut().enumerate() {
        measure(n);
        total_h += n.h;
        if i > 0 {
            total_h += 8.0;
        }
        max_w = max_w.max(n.w);
    }
    let mut y = ((screen_h as f32 - total_h) / 2.0).max(0.0);
    for n in roots.iter_mut() {
        let x = ((screen_w as f32 - n.w) / 2.0).max(0.0);
        position(n, x, y);
        y += n.h + 8.0;
    }
}

fn measure(node: &mut UiNode) {
    for c in node.children.iter_mut() {
        measure(c);
    }
    let p = &node.props;
    match node.kind {
        UiKind::Text => {
            let t = node.text.as_deref().unwrap_or("");
            node.w = p.width.unwrap_or_else(|| text_width(t, p.font_size));
            node.h = p.height.unwrap_or(p.font_size as f32);
        }
        UiKind::Button => {
            let t = node.text.as_deref().unwrap_or("");
            node.w = p.width.unwrap_or_else(|| text_width(t, p.font_size) + BTN_PAD_X * 2.0);
            node.h = p.height.unwrap_or(p.font_size as f32 + BTN_PAD_Y * 2.0);
        }
        UiKind::Spacer => {
            node.w = p.width.unwrap_or(0.0);
            node.h = p.height.unwrap_or(0.0);
        }
        UiKind::TextField => {
            node.w = p.width.unwrap_or(FIELD_W);
            node.h = p.height.unwrap_or(FIELD_H);
        }
        UiKind::Checkbox => {
            let label = node.text.as_deref().unwrap_or("");
            let label_w = if label.is_empty() { 0.0 } else { 8.0 + text_width(label, p.font_size) };
            node.w = p.width.unwrap_or(BOX + label_w);
            node.h = p.height.unwrap_or(BOX.max(p.font_size as f32));
        }
        UiKind::Slider => {
            node.w = p.width.unwrap_or(FIELD_W);
            node.h = p.height.unwrap_or(SLIDER_H);
        }
        UiKind::Image => {
            // Defaults are filled from the sprite's natural size by the builder.
            node.w = p.width.unwrap_or(64.0);
            node.h = p.height.unwrap_or(64.0);
        }
        UiKind::Column => {
            let n = node.children.len();
            let inner_w = node.children.iter().map(|c| c.w).fold(0.0, f32::max);
            let inner_h: f32 = node.children.iter().map(|c| c.h).sum::<f32>()
                + if n > 1 { p.spacing * (n as f32 - 1.0) } else { 0.0 };
            node.w = p.width.unwrap_or(inner_w + p.padding * 2.0);
            node.h = p.height.unwrap_or(inner_h + p.padding * 2.0);
        }
        UiKind::Row => {
            let n = node.children.len();
            let inner_w: f32 = node.children.iter().map(|c| c.w).sum::<f32>()
                + if n > 1 { p.spacing * (n as f32 - 1.0) } else { 0.0 };
            let inner_h = node.children.iter().map(|c| c.h).fold(0.0, f32::max);
            node.w = p.width.unwrap_or(inner_w + p.padding * 2.0);
            node.h = p.height.unwrap_or(inner_h + p.padding * 2.0);
        }
    }
}

fn position(node: &mut UiNode, x: f32, y: f32) {
    node.x = x;
    node.y = y;
    let pad = node.props.padding;
    let spacing = node.props.spacing;
    let align = node.props.align;
    match node.kind {
        UiKind::Column => {
            let inner_w = node.w - pad * 2.0;
            let mut cy = y + pad;
            for child in node.children.iter_mut() {
                let cx = match align {
                    Align::Start => x + pad,
                    Align::Center => x + pad + (inner_w - child.w) / 2.0,
                    Align::End => x + node.w - pad - child.w,
                };
                position(child, cx, cy);
                cy += child.h + spacing;
            }
        }
        UiKind::Row => {
            let inner_h = node.h - pad * 2.0;
            let mut cx = x + pad;
            for child in node.children.iter_mut() {
                let cy = match align {
                    Align::Start => y + pad,
                    Align::Center => y + pad + (inner_h - child.h) / 2.0,
                    Align::End => y + node.h - pad - child.h,
                };
                position(child, cx, cy);
                cx += child.w + spacing;
            }
        }
        _ => {}
    }
}

/// Walk the laid-out tree, emitting draw commands and collecting interactive
/// controls. `focused` is the index (into `controls`) of the text field that
/// currently has keyboard focus, so it can be drawn with a caret.
pub fn collect(
    roots: &[UiNode],
    mouse: (f32, f32),
    focused: Option<usize>,
    out: &mut Vec<DrawCmd>,
    controls: &mut Vec<Control>,
) {
    for n in roots {
        draw_node(n, mouse, focused, out, controls);
    }
}

fn draw_node(
    node: &UiNode,
    mouse: (f32, f32),
    focused: Option<usize>,
    out: &mut Vec<DrawCmd>,
    controls: &mut Vec<Control>,
) {
    match node.kind {
        UiKind::TextField => {
            let idx = controls.len();
            let is_focused = focused == Some(idx);
            // Border (accent when focused), then the field background inset by 2px.
            let border = if is_focused { ACCENT } else { FIELD_BORDER };
            out.push(DrawCmd::Rect { x: node.x, y: node.y, w: node.w, h: node.h, color: border });
            out.push(DrawCmd::Rect {
                x: node.x + 2.0, y: node.y + 2.0, w: node.w - 4.0, h: node.h - 4.0, color: FIELD_BG,
            });
            let t = node.text.clone().unwrap_or_default();
            // Keep the visible string inside the field so long input doesn't spill.
            let max_chars = ((node.w - 16.0) / (node.props.font_size as f32 * 0.5)).floor().max(0.0) as usize;
            let shown = if t.chars().count() > max_chars {
                t.chars().skip(t.chars().count() - max_chars).collect::<String>()
            } else {
                t.clone()
            };
            let ty = node.y + (node.h - node.props.font_size as f32) / 2.0;
            out.push(DrawCmd::Text {
                text: shown.clone(), x: node.x + 8.0, y: ty, size: node.props.font_size,
                color: node.props.color.unwrap_or(TEXT_COLOR), font: node.font,
            });
            if is_focused {
                let caret_x = node.x + 8.0 + text_width(&shown, node.props.font_size) + 1.0;
                out.push(DrawCmd::Line {
                    x1: caret_x, y1: node.y + 6.0, x2: caret_x, y2: node.y + node.h - 6.0,
                    thick: 1.5, color: TEXT_COLOR,
                });
            }
            controls.push(Control {
                x: node.x, y: node.y, w: node.w, h: node.h, kind: ControlKind::TextField,
                callback: node.on_change.clone(), bind: node.bind.clone(),
                checked: false, number: 0.0, min: 0.0, max: 0.0, step: 0.0, text: t,
            });
        }
        UiKind::Checkbox => {
            let box_y = node.y + (node.h - BOX) / 2.0;
            out.push(DrawCmd::Rect { x: node.x, y: box_y, w: BOX, h: BOX, color: FIELD_BORDER });
            out.push(DrawCmd::Rect {
                x: node.x + 2.0, y: box_y + 2.0, w: BOX - 4.0, h: BOX - 4.0, color: FIELD_BG,
            });
            if node.checked {
                out.push(DrawCmd::Rect {
                    x: node.x + 5.0, y: box_y + 5.0, w: BOX - 10.0, h: BOX - 10.0, color: ACCENT,
                });
            }
            if let Some(label) = &node.text {
                out.push(DrawCmd::Text {
                    text: label.clone(), x: node.x + BOX + 8.0,
                    y: node.y + (node.h - node.props.font_size as f32) / 2.0,
                    size: node.props.font_size, color: node.props.color.unwrap_or(TEXT_COLOR),
                    font: node.font,
                });
            }
            controls.push(Control {
                x: node.x, y: node.y, w: node.w, h: node.h, kind: ControlKind::Checkbox,
                callback: node.on_change.clone(), bind: node.bind.clone(),
                checked: node.checked, number: 0.0, min: 0.0, max: 0.0, step: 0.0, text: String::new(),
            });
        }
        UiKind::Slider => {
            let cy = node.y + node.h / 2.0;
            let range = (node.max - node.min).max(0.0001);
            let frac = ((node.number - node.min) / range).clamp(0.0, 1.0);
            let knob_x = node.x + frac * node.w;
            // Track, then the filled portion, then the knob.
            out.push(DrawCmd::Rect { x: node.x, y: cy - 3.0, w: node.w, h: 6.0, color: TRACK });
            out.push(DrawCmd::Rect { x: node.x, y: cy - 3.0, w: knob_x - node.x, h: 6.0, color: ACCENT });
            out.push(DrawCmd::Circle { x: knob_x, y: cy, r: node.h / 2.0, color: ACCENT });
            controls.push(Control {
                x: node.x, y: node.y, w: node.w, h: node.h, kind: ControlKind::Slider,
                callback: node.on_change.clone(), bind: node.bind.clone(),
                checked: false, number: node.number, min: node.min, max: node.max,
                step: node.step, text: String::new(),
            });
        }
        UiKind::Image => {
            if let Some(id) = node.sprite {
                out.push(DrawCmd::SpriteRect { id, x: node.x, y: node.y, w: node.w, h: node.h });
            }
        }
        UiKind::Text => {
            out.push(DrawCmd::Text {
                text: node.text.clone().unwrap_or_default(),
                x: node.x,
                y: node.y,
                size: node.props.font_size,
                color: node.props.color.unwrap_or(TEXT_COLOR),
                font: node.font,
            });
        }
        UiKind::Button => {
            let hovered = point_in(mouse, node);
            if let Some(id) = node.sprite {
                out.push(DrawCmd::SpriteRect {
                    id,
                    x: node.x,
                    y: node.y,
                    w: node.w,
                    h: node.h,
                });
                if hovered {
                    out.push(DrawCmd::Rect {
                        x: node.x,
                        y: node.y,
                        w: node.w,
                        h: node.h,
                        color: Color(255, 255, 255, 40),
                    });
                }
            } else {
                let bg = if hovered {
                    node.props.bg.map(brighten).unwrap_or(BUTTON_HOVER)
                } else {
                    node.props.bg.unwrap_or(BUTTON_BG)
                };
                out.push(DrawCmd::Rect { x: node.x, y: node.y, w: node.w, h: node.h, color: bg });
            }
            let t = node.text.clone().unwrap_or_default();
            let tw = text_width(&t, node.props.font_size);
            out.push(DrawCmd::Text {
                text: t,
                x: node.x + (node.w - tw) / 2.0,
                y: node.y + (node.h - node.props.font_size as f32) / 2.0,
                size: node.props.font_size,
                color: node.props.color.unwrap_or(BUTTON_TEXT),
                font: node.font,
            });
            if let Some(cb) = &node.callback {
                controls.push(Control {
                    x: node.x, y: node.y, w: node.w, h: node.h, kind: ControlKind::Button,
                    callback: Some(cb.clone()), bind: None, checked: false, number: 0.0,
                    min: 0.0, max: 0.0, step: 0.0, text: String::new(),
                });
            }
        }
        UiKind::Spacer => {}
        UiKind::Column | UiKind::Row => {
            if let Some(bg) = node.props.bg {
                out.push(DrawCmd::Rect { x: node.x, y: node.y, w: node.w, h: node.h, color: bg });
            }
            for child in &node.children {
                draw_node(child, mouse, focused, out, controls);
            }
        }
    }
}

fn point_in(p: (f32, f32), node: &UiNode) -> bool {
    p.0 >= node.x && p.0 <= node.x + node.w && p.1 >= node.y && p.1 <= node.y + node.h
}

fn brighten(c: Color) -> Color {
    let up = |v: u8| ((v as u16 + 30).min(255)) as u8;
    Color(up(c.0), up(c.1), up(c.2), c.3)
}
