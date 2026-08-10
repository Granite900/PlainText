//! The UI layout + draw engine for `window` blocks.
//!
//! It's Raylib-free (like `gfx.rs`): the interpreter turns the widget AST into
//! a tree of [`UiNode`]s each frame, this module lays them out and emits
//! [`DrawCmd`]s plus a list of clickable [`Control`]s, and `game.rs` renders the
//! commands and dispatches clicks. Layout is a small flexbox-lite: `column`
//! stacks children vertically, `row` horizontally, with `padding`, `spacing`,
//! and cross-axis `align`.

use std::collections::HashMap;

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
const ACCENT: Color = Color(70, 120, 220, 255);
const FIELD_BG: Color = Color(255, 255, 255, 255);
const FIELD_BORDER: Color = Color(170, 170, 180, 255);
const TRACK: Color = Color(200, 200, 208, 255);
const ROW_HOVER: Color = Color(230, 236, 250, 255);
const ROW_SELECTED: Color = Color(200, 216, 245, 255);
const FIELD_H: f32 = 34.0;
const FIELD_W: f32 = 220.0;
const BOX: f32 = 22.0;
const SLIDER_H: f32 = 24.0;
const LIST_ROW_H: f32 = 28.0;
const SCROLL_DEFAULT_H: f32 = 160.0;
const LIST_DEFAULT_H: f32 = 140.0;

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
    Scroll,
    List,
    Dropdown,
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
    pub sprite: Option<usize>,
    pub font: Option<usize>,
    pub checked: bool,
    pub number: f32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub on_change: Option<Value>,
    pub bind: Option<String>,
    /// Rows for `list` / `dropdown` (`items:`).
    pub items: Vec<String>,
    /// Selected index for list/dropdown (`-1` = none).
    pub selected: i32,
    /// Multi-line `text_field` (`multiline: true`).
    pub multiline: bool,
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
            items: Vec::new(),
            selected: -1,
            multiline: false,
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum ControlKind {
    Button,
    Checkbox,
    Slider,
    TextField,
    Scroll,
    List,
    Dropdown,
}

#[derive(Clone)]
pub struct Control {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub kind: ControlKind,
    pub callback: Option<Value>,
    pub bind: Option<String>,
    pub checked: bool,
    pub number: f32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub text: String,
    pub font_size: i32,
    pub items: Vec<String>,
    pub multiline: bool,
    /// Full content height (for scroll / list / open dropdown menu).
    pub content_h: f32,
    pub row_h: f32,
}

/// Per-frame extras the runner passes into [`collect`].
pub struct UiDrawState<'a> {
    pub focused: Option<usize>,
    pub caret: usize,
    pub scrolls: &'a HashMap<usize, f32>,
    pub open_dropdown: Option<usize>,
}

fn text_width(text: &str, size: i32) -> f32 {
    text.chars().count() as f32 * size as f32 * 0.5
}

/// Wrap `text` into lines that fit `max_w` pixels (same ~0.5em estimate as layout).
pub fn wrap_lines(text: &str, max_w: f32, font_size: i32) -> Vec<String> {
    let em = (font_size as f32 * 0.5).max(1.0);
    let max_chars = ((max_w / em).floor() as usize).max(1);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }
        let chars: Vec<char> = paragraph.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let end = (i + max_chars).min(chars.len());
            lines.push(chars[i..end].iter().collect());
            i = end;
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Clamp a scroll offset so content stays in view.
pub fn clamp_scroll(offset: f32, content_h: f32, view_h: f32) -> f32 {
    let max = (content_h - view_h).max(0.0);
    offset.clamp(0.0, max)
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
        position(n, x, y, 0.0);
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
            let default_h = if node.multiline { FIELD_H * 3.0 } else { FIELD_H };
            node.h = p.height.unwrap_or(default_h);
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
            node.w = p.width.unwrap_or(64.0);
            node.h = p.height.unwrap_or(64.0);
        }
        UiKind::List => {
            node.w = p.width.unwrap_or(FIELD_W);
            node.h = p.height.unwrap_or(LIST_DEFAULT_H);
        }
        UiKind::Dropdown => {
            node.w = p.width.unwrap_or(FIELD_W);
            node.h = p.height.unwrap_or(FIELD_H);
        }
        UiKind::Column | UiKind::Scroll => {
            let n = node.children.len();
            let inner_w = node.children.iter().map(|c| c.w).fold(0.0, f32::max);
            let inner_h: f32 = node.children.iter().map(|c| c.h).sum::<f32>()
                + if n > 1 { p.spacing * (n as f32 - 1.0) } else { 0.0 };
            node.w = p.width.unwrap_or(inner_w + p.padding * 2.0);
            if node.kind == UiKind::Scroll {
                node.h = p.height.unwrap_or(SCROLL_DEFAULT_H);
            } else {
                node.h = p.height.unwrap_or(inner_h + p.padding * 2.0);
            }
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

fn position(node: &mut UiNode, x: f32, y: f32, scroll_y: f32) {
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
                position(child, cx, cy, 0.0);
                cy += child.h + spacing;
            }
        }
        UiKind::Scroll => {
            let inner_w = node.w - pad * 2.0;
            let mut cy = y + pad - scroll_y;
            for child in node.children.iter_mut() {
                let cx = match align {
                    Align::Start => x + pad,
                    Align::Center => x + pad + (inner_w - child.w) / 2.0,
                    Align::End => x + node.w - pad - child.w,
                };
                position(child, cx, cy, 0.0);
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
                position(child, cx, cy, 0.0);
                cx += child.w + spacing;
            }
        }
        _ => {}
    }
}

/// Content height of a column-like stack (padding + children + spacing).
fn stack_content_h(node: &UiNode) -> f32 {
    let n = node.children.len();
    let inner: f32 = node.children.iter().map(|c| c.h).sum();
    let gaps = if n > 1 { node.props.spacing * (n as f32 - 1.0) } else { 0.0 };
    inner + gaps + node.props.padding * 2.0
}

pub fn field_capacity(w: f32, font_size: i32) -> usize {
    ((w - 16.0) / (font_size as f32 * 0.5)).floor().max(1.0) as usize
}

/// First layout pass uses scroll 0; call [`apply_scroll_offsets`] then re-position scrolls.
pub fn apply_scroll_offsets(roots: &mut [UiNode], scrolls: &HashMap<usize, f32>, controls: &[Control]) {
    // Re-position scroll children using the control index → offset map.
    // We walk in the same order `collect` assigns Scroll control indices by
    // replaying the tree; simpler: for each Scroll control, find matching node by rect.
    for (idx, c) in controls.iter().enumerate() {
        if c.kind != ControlKind::Scroll {
            continue;
        }
        let offset = scrolls.get(&idx).copied().unwrap_or(0.0);
        if let Some(node) = find_node_at(roots, c.x, c.y, UiKind::Scroll) {
            position(node, c.x, c.y, offset);
        }
    }
}

fn find_node_at<'a>(nodes: &'a mut [UiNode], x: f32, y: f32, kind: UiKind) -> Option<&'a mut UiNode> {
    for n in nodes.iter_mut() {
        if n.kind == kind && (n.x - x).abs() < 0.5 && (n.y - y).abs() < 0.5 {
            return Some(n);
        }
        if let Some(found) = find_node_at(&mut n.children, x, y, kind) {
            return Some(found);
        }
    }
    None
}

/// Two-phase collect: first build controls with scroll=0 positions to get indices,
/// apply offsets, then draw. The window runner uses [`collect_frame`] instead.
pub fn collect_frame(
    roots: &mut [UiNode],
    mouse: (f32, f32),
    state: &UiDrawState<'_>,
    out: &mut Vec<DrawCmd>,
    controls: &mut Vec<Control>,
) {
    // Pass 1: assign control indices / measure scroll content with zero offset.
    let mut probe = Vec::new();
    for n in roots.iter() {
        collect_controls_only(n, &mut probe);
    }
    apply_scroll_offsets(roots, state.scrolls, &probe);

    // Pass 2: draw + emit real controls (same order as probe for Scroll indices).
    for n in roots.iter() {
        draw_node(n, mouse, state, None, out, controls);
    }
}

fn collect_controls_only(node: &UiNode, controls: &mut Vec<Control>) {
    match node.kind {
        UiKind::TextField | UiKind::Checkbox | UiKind::Slider | UiKind::List | UiKind::Dropdown => {
            controls.push(dummy_control(node));
        }
        UiKind::Button => {
            if node.callback.is_some() {
                controls.push(dummy_control(node));
            }
        }
        UiKind::Scroll => {
            controls.push(dummy_control(node));
            for child in &node.children {
                collect_controls_only(child, controls);
            }
        }
        UiKind::Column | UiKind::Row => {
            for child in &node.children {
                collect_controls_only(child, controls);
            }
        }
        _ => {}
    }
}

fn dummy_control(node: &UiNode) -> Control {
    let kind = match node.kind {
        UiKind::Button => ControlKind::Button,
        UiKind::Checkbox => ControlKind::Checkbox,
        UiKind::Slider => ControlKind::Slider,
        UiKind::TextField => ControlKind::TextField,
        UiKind::Scroll => ControlKind::Scroll,
        UiKind::List => ControlKind::List,
        UiKind::Dropdown => ControlKind::Dropdown,
        _ => ControlKind::Button,
    };
    Control {
        x: node.x,
        y: node.y,
        w: node.w,
        h: node.h,
        kind,
        callback: None,
        bind: None,
        checked: false,
        number: 0.0,
        min: 0.0,
        max: 0.0,
        step: 0.0,
        text: String::new(),
        font_size: node.props.font_size,
        items: Vec::new(),
        multiline: node.multiline,
        content_h: stack_content_h(node),
        row_h: LIST_ROW_H,
    }
}

fn draw_node(
    node: &UiNode,
    mouse: (f32, f32),
    state: &UiDrawState<'_>,
    clip: Option<(f32, f32, f32, f32)>,
    out: &mut Vec<DrawCmd>,
    controls: &mut Vec<Control>,
) {
    match node.kind {
        UiKind::TextField => {
            let idx = controls.len();
            let is_focused = state.focused == Some(idx);
            let border = if is_focused { ACCENT } else { FIELD_BORDER };
            out.push(DrawCmd::Rect { x: node.x, y: node.y, w: node.w, h: node.h, color: border });
            out.push(DrawCmd::Rect {
                x: node.x + 2.0, y: node.y + 2.0, w: node.w - 4.0, h: node.h - 4.0, color: FIELD_BG,
            });
            let t = node.text.clone().unwrap_or_default();
            let fs = node.props.font_size;
            if node.multiline {
                draw_multiline_field(node, &t, is_focused, state.caret, state.scrolls.get(&idx).copied().unwrap_or(0.0), out);
            } else {
                draw_single_line_field(node, &t, is_focused, state.caret, out);
            }
            if control_hits_clip(node.x, node.y, node.w, node.h, clip) {
                controls.push(Control {
                    x: node.x, y: node.y, w: node.w, h: node.h, kind: ControlKind::TextField,
                    callback: node.on_change.clone(), bind: node.bind.clone(),
                    checked: false, number: 0.0, min: 0.0, max: 0.0, step: 0.0, text: t,
                    font_size: fs, items: Vec::new(), multiline: node.multiline,
                    content_h: 0.0, row_h: fs as f32 + 4.0,
                });
            }
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
            if control_hits_clip(node.x, node.y, node.w, node.h, clip) {
                controls.push(Control {
                    x: node.x, y: node.y, w: node.w, h: node.h, kind: ControlKind::Checkbox,
                    callback: node.on_change.clone(), bind: node.bind.clone(),
                    checked: node.checked, number: 0.0, min: 0.0, max: 0.0, step: 0.0, text: String::new(),
                    font_size: node.props.font_size, items: Vec::new(), multiline: false,
                    content_h: 0.0, row_h: 0.0,
                });
            }
        }
        UiKind::Slider => {
            let cy = node.y + node.h / 2.0;
            let range = (node.max - node.min).max(0.0001);
            let frac = ((node.number - node.min) / range).clamp(0.0, 1.0);
            let knob_x = node.x + frac * node.w;
            out.push(DrawCmd::Rect { x: node.x, y: cy - 3.0, w: node.w, h: 6.0, color: TRACK });
            out.push(DrawCmd::Rect { x: node.x, y: cy - 3.0, w: knob_x - node.x, h: 6.0, color: ACCENT });
            out.push(DrawCmd::Circle { x: knob_x, y: cy, r: node.h / 2.0, color: ACCENT });
            if control_hits_clip(node.x, node.y, node.w, node.h, clip) {
                controls.push(Control {
                    x: node.x, y: node.y, w: node.w, h: node.h, kind: ControlKind::Slider,
                    callback: node.on_change.clone(), bind: node.bind.clone(),
                    checked: false, number: node.number, min: node.min, max: node.max,
                    step: node.step, text: String::new(), font_size: node.props.font_size,
                    items: Vec::new(), multiline: false, content_h: 0.0, row_h: 0.0,
                });
            }
        }
        UiKind::Image => {
            if let Some(id) = node.sprite {
                out.push(DrawCmd::SpriteRect { id, x: node.x, y: node.y, w: node.w, h: node.h });
            }
        }
        UiKind::Text => {
            out.push(DrawCmd::Text {
                text: node.text.clone().unwrap_or_default(),
                x: node.x, y: node.y, size: node.props.font_size,
                color: node.props.color.unwrap_or(TEXT_COLOR), font: node.font,
            });
        }
        UiKind::Button => {
            let hovered = point_in(mouse, node);
            if let Some(id) = node.sprite {
                out.push(DrawCmd::SpriteRect { id, x: node.x, y: node.y, w: node.w, h: node.h });
                if hovered {
                    out.push(DrawCmd::Rect {
                        x: node.x, y: node.y, w: node.w, h: node.h, color: Color(255, 255, 255, 40),
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
                text: t, x: node.x + (node.w - tw) / 2.0,
                y: node.y + (node.h - node.props.font_size as f32) / 2.0,
                size: node.props.font_size, color: node.props.color.unwrap_or(BUTTON_TEXT),
                font: node.font,
            });
            if node.callback.is_some() && control_hits_clip(node.x, node.y, node.w, node.h, clip) {
                controls.push(Control {
                    x: node.x, y: node.y, w: node.w, h: node.h, kind: ControlKind::Button,
                    callback: node.callback.clone(), bind: None, checked: false, number: 0.0,
                    min: 0.0, max: 0.0, step: 0.0, text: String::new(), font_size: node.props.font_size,
                    items: Vec::new(), multiline: false, content_h: 0.0, row_h: 0.0,
                });
            }
        }
        UiKind::Spacer => {}
        UiKind::List => {
            draw_list_widget(node, mouse, state, out, controls);
        }
        UiKind::Dropdown => {
            draw_dropdown_widget(node, mouse, state, out, controls);
        }
        UiKind::Scroll => {
            let idx = controls.len();
            let content_h = stack_content_h(node);
            let scroll = clamp_scroll(
                state.scrolls.get(&idx).copied().unwrap_or(0.0),
                content_h,
                node.h,
            );
            out.push(DrawCmd::Rect {
                x: node.x, y: node.y, w: node.w, h: node.h,
                color: node.props.bg.unwrap_or(Color(248, 248, 250, 255)),
            });
            out.push(DrawCmd::Rect {
                x: node.x, y: node.y, w: node.w, h: node.h, color: FIELD_BORDER,
            });
            // inset fill
            out.push(DrawCmd::Rect {
                x: node.x + 1.0, y: node.y + 1.0, w: node.w - 2.0, h: node.h - 2.0,
                color: node.props.bg.unwrap_or(Color(248, 248, 250, 255)),
            });
            controls.push(Control {
                x: node.x, y: node.y, w: node.w, h: node.h, kind: ControlKind::Scroll,
                callback: None, bind: None, checked: false, number: scroll, min: 0.0,
                max: (content_h - node.h).max(0.0), step: 0.0, text: String::new(),
                font_size: node.props.font_size, items: Vec::new(), multiline: false,
                content_h, row_h: 0.0,
            });
            out.push(DrawCmd::ScissorBegin {
                x: node.x as i32,
                y: node.y as i32,
                w: node.w as i32,
                h: node.h as i32,
            });
            let child_clip = Some((node.x, node.y, node.w, node.h));
            let _ = scroll; // children already positioned with offset in apply_scroll_offsets
            for child in &node.children {
                draw_node(child, mouse, state, child_clip, out, controls);
            }
            out.push(DrawCmd::ScissorEnd);
        }
        UiKind::Column | UiKind::Row => {
            if let Some(bg) = node.props.bg {
                out.push(DrawCmd::Rect { x: node.x, y: node.y, w: node.w, h: node.h, color: bg });
            }
            for child in &node.children {
                draw_node(child, mouse, state, clip, out, controls);
            }
        }
    }
}

fn draw_single_line_field(node: &UiNode, t: &str, is_focused: bool, caret: usize, out: &mut Vec<DrawCmd>) {
    let chars: Vec<char> = t.chars().collect();
    let len = chars.len();
    let fs = node.props.font_size;
    let max_chars = field_capacity(node.w, fs);
    let pos = caret.min(len);
    let start = if is_focused && pos > max_chars { pos - max_chars } else { 0 };
    let end = (start + max_chars).min(len);
    let shown: String = chars[start..end].iter().collect();
    let ty = node.y + (node.h - fs as f32) / 2.0;
    out.push(DrawCmd::Text {
        text: shown, x: node.x + 8.0, y: ty, size: fs,
        color: node.props.color.unwrap_or(TEXT_COLOR), font: node.font,
    });
    if is_focused {
        let before: String = chars[start..pos].iter().collect();
        let caret_x = node.x + 8.0 + text_width(&before, fs) + 1.0;
        out.push(DrawCmd::Line {
            x1: caret_x, y1: node.y + 6.0, x2: caret_x, y2: node.y + node.h - 6.0,
            thick: 1.5, color: TEXT_COLOR,
        });
    }
}

fn draw_multiline_field(
    node: &UiNode,
    t: &str,
    is_focused: bool,
    caret: usize,
    scroll: f32,
    out: &mut Vec<DrawCmd>,
) {
    let fs = node.props.font_size;
    let line_h = fs as f32 + 4.0;
    let lines = wrap_lines(t, node.w - 16.0, fs);
    let scroll = clamp_scroll(scroll, lines.len() as f32 * line_h, node.h - 8.0);
    out.push(DrawCmd::ScissorBegin {
        x: (node.x + 2.0) as i32,
        y: (node.y + 2.0) as i32,
        w: (node.w - 4.0) as i32,
        h: (node.h - 4.0) as i32,
    });
    let mut y = node.y + 6.0 - scroll;
    let mut char_at = 0usize;
    let caret_pos = caret.min(t.chars().count());
    for (li, line) in lines.iter().enumerate() {
        out.push(DrawCmd::Text {
            text: line.clone(), x: node.x + 8.0, y, size: fs,
            color: node.props.color.unwrap_or(TEXT_COLOR), font: node.font,
        });
        let line_len = line.chars().count();
        if is_focused && caret_pos >= char_at && caret_pos <= char_at + line_len {
            let col = caret_pos - char_at;
            let before: String = line.chars().take(col).collect();
            let caret_x = node.x + 8.0 + text_width(&before, fs) + 1.0;
            out.push(DrawCmd::Line {
                x1: caret_x, y1: y, x2: caret_x, y2: y + fs as f32,
                thick: 1.5, color: TEXT_COLOR,
            });
        }
        char_at += line_len;
        // Account for the newline between wrapped paragraphs / hard breaks.
        if li + 1 < lines.len() {
            // wrap_lines splits on `\n` into paragraphs; char index must skip the `\n`.
            // We approximate: if the next segment came from a hard break, source has `\n`.
            // Count newlines in original by walking — simpler: use char_at against full string.
        }
        y += line_h;
        // After each visual line from wrap, if we're still in the same paragraph,
        // there is no newline char. Hard breaks are empty-line markers from split.
        // Fix caret mapping properly via helper used by game.rs too.
        let _ = char_at;
    }
    // Recompute caret with a proper index map:
    let (line_i, col) = caret_line_col(t, caret_pos, node.w - 16.0, fs);
    let y_caret = node.y + 6.0 - scroll + line_i as f32 * line_h;
    if is_focused {
        let line = lines.get(line_i).map(String::as_str).unwrap_or("");
        let before: String = line.chars().take(col).collect();
        let caret_x = node.x + 8.0 + text_width(&before, fs) + 1.0;
        // Clear any wrong caret from the loop by redrawing the correct one on top.
        out.push(DrawCmd::Line {
            x1: caret_x, y1: y_caret, x2: caret_x, y2: y_caret + fs as f32,
            thick: 1.5, color: TEXT_COLOR,
        });
    }
    out.push(DrawCmd::ScissorEnd);
}

/// Map a caret character index to (line, column) for wrapped multiline text.
pub fn caret_line_col(text: &str, caret: usize, max_w: f32, font_size: i32) -> (usize, usize) {
    let lines = wrap_lines_with_breaks(text, max_w, font_size);
    let mut at = 0usize;
    for (i, (line, consumed)) in lines.iter().enumerate() {
        if caret <= at + consumed {
            return (i, caret.saturating_sub(at).min(line.chars().count()));
        }
        at += consumed;
    }
    let last = lines.len().saturating_sub(1);
    let col = lines.last().map(|(l, _)| l.chars().count()).unwrap_or(0);
    (last, col)
}

/// Like [`wrap_lines`], but also returns how many source characters each visual
/// line consumed (including a trailing `\n` when the break was hard).
pub fn wrap_lines_with_breaks(text: &str, max_w: f32, font_size: i32) -> Vec<(String, usize)> {
    let em = (font_size as f32 * 0.5).max(1.0);
    let max_chars = ((max_w / em).floor() as usize).max(1);
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    if text.is_empty() {
        return vec![(String::new(), 0)];
    }
    while chars.peek().is_some() {
        let mut line = String::new();
        let mut consumed = 0usize;
        while let Some(&c) = chars.peek() {
            if c == '\n' {
                chars.next();
                consumed += 1;
                break;
            }
            if line.chars().count() >= max_chars {
                break;
            }
            line.push(c);
            chars.next();
            consumed += 1;
        }
        out.push((line, consumed));
    }
    if out.is_empty() {
        out.push((String::new(), 0));
    }
    out
}

/// Move caret up/down one visual line. Returns new caret index.
pub fn move_caret_vertical(text: &str, caret: usize, max_w: f32, font_size: i32, up: bool) -> usize {
    let lines = wrap_lines_with_breaks(text, max_w, font_size);
    let (li, col) = caret_line_col(text, caret, max_w, font_size);
    let target = if up { li.saturating_sub(1) } else { (li + 1).min(lines.len().saturating_sub(1)) };
    let mut at = 0usize;
    for (i, (line, consumed)) in lines.iter().enumerate() {
        if i == target {
            let col = col.min(line.chars().count());
            return at + col;
        }
        at += consumed;
    }
    caret
}

fn draw_list_rows(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    items: &[String],
    selected: i32,
    scroll: f32,
    mouse: (f32, f32),
    font_size: i32,
    font: Option<usize>,
    out: &mut Vec<DrawCmd>,
) {
    let row_h = LIST_ROW_H;
    let content_h = items.len() as f32 * row_h;
    let scroll = clamp_scroll(scroll, content_h, h);
    out.push(DrawCmd::Rect { x, y, w, h, color: FIELD_BORDER });
    out.push(DrawCmd::Rect { x: x + 1.0, y: y + 1.0, w: w - 2.0, h: h - 2.0, color: FIELD_BG });
    out.push(DrawCmd::ScissorBegin { x: x as i32, y: y as i32, w: w as i32, h: h as i32 });
    for (i, item) in items.iter().enumerate() {
        let ry = y + i as f32 * row_h - scroll;
        if ry + row_h < y || ry > y + h {
            continue;
        }
        let hovered = mouse.0 >= x && mouse.0 <= x + w && mouse.1 >= ry && mouse.1 <= ry + row_h;
        let bg = if i as i32 == selected {
            ROW_SELECTED
        } else if hovered {
            ROW_HOVER
        } else {
            FIELD_BG
        };
        out.push(DrawCmd::Rect { x: x + 1.0, y: ry, w: w - 2.0, h: row_h, color: bg });
        out.push(DrawCmd::Text {
            text: item.clone(),
            x: x + 8.0,
            y: ry + (row_h - font_size as f32) / 2.0,
            size: font_size,
            color: TEXT_COLOR,
            font,
        });
    }
    out.push(DrawCmd::ScissorEnd);
}

fn draw_list_widget(
    node: &UiNode,
    mouse: (f32, f32),
    state: &UiDrawState<'_>,
    out: &mut Vec<DrawCmd>,
    controls: &mut Vec<Control>,
) {
    let idx = controls.len();
    let scroll = state.scrolls.get(&idx).copied().unwrap_or(0.0);
    let content_h = node.items.len() as f32 * LIST_ROW_H;
    draw_list_rows(
        node.x, node.y, node.w, node.h, &node.items, node.selected, scroll, mouse,
        node.props.font_size, node.font, out,
    );
    controls.push(Control {
        x: node.x, y: node.y, w: node.w, h: node.h, kind: ControlKind::List,
        callback: node.on_change.clone(), bind: node.bind.clone(),
        checked: false, number: node.selected as f32, min: 0.0, max: 0.0, step: 0.0,
        text: String::new(), font_size: node.props.font_size, items: node.items.clone(),
        multiline: false, content_h, row_h: LIST_ROW_H,
    });
}

fn draw_dropdown_widget(
    node: &UiNode,
    mouse: (f32, f32),
    state: &UiDrawState<'_>,
    out: &mut Vec<DrawCmd>,
    controls: &mut Vec<Control>,
) {
    let idx = controls.len();
    let open = state.open_dropdown == Some(idx);
    let label = if node.selected >= 0 {
        node.items
            .get(node.selected as usize)
            .cloned()
            .unwrap_or_else(|| "Choose…".into())
    } else {
        "Choose…".into()
    };
    let hovered = point_in(mouse, node);
    let border = if open || hovered { ACCENT } else { FIELD_BORDER };
    out.push(DrawCmd::Rect { x: node.x, y: node.y, w: node.w, h: node.h, color: border });
    out.push(DrawCmd::Rect {
        x: node.x + 2.0, y: node.y + 2.0, w: node.w - 4.0, h: node.h - 4.0, color: FIELD_BG,
    });
    out.push(DrawCmd::Text {
        text: label,
        x: node.x + 8.0,
        y: node.y + (node.h - node.props.font_size as f32) / 2.0,
        size: node.props.font_size,
        color: TEXT_COLOR,
        font: node.font,
    });
    // Chevron
    out.push(DrawCmd::Text {
        text: if open { "^".into() } else { "v".into() },
        x: node.x + node.w - 22.0,
        y: node.y + (node.h - node.props.font_size as f32) / 2.0,
        size: node.props.font_size,
        color: TEXT_COLOR,
        font: None,
    });

    let menu_h = (node.items.len() as f32 * LIST_ROW_H).min(LIST_DEFAULT_H);
    let content_h = node.items.len() as f32 * LIST_ROW_H;
    controls.push(Control {
        x: node.x, y: node.y, w: node.w, h: node.h, kind: ControlKind::Dropdown,
        callback: node.on_change.clone(), bind: node.bind.clone(),
        checked: open, number: node.selected as f32, min: 0.0, max: 0.0, step: 0.0,
        text: String::new(), font_size: node.props.font_size, items: node.items.clone(),
        multiline: false, content_h, row_h: LIST_ROW_H,
    });

    if open {
        let menu_y = node.y + node.h;
        let scroll = state.scrolls.get(&idx).copied().unwrap_or(0.0);
        draw_list_rows(
            node.x, menu_y, node.w, menu_h, &node.items, node.selected, scroll, mouse,
            node.props.font_size, node.font, out,
        );
        // Expand hit area: store menu geometry in number/max hack — use separate fields.
        // Overwrite the control we just pushed with taller hit box covering header+menu.
        if let Some(c) = controls.last_mut() {
            c.h = node.h + menu_h;
            c.content_h = content_h;
        }
    }
}

fn control_hits_clip(x: f32, y: f32, w: f32, h: f32, clip: Option<(f32, f32, f32, f32)>) -> bool {
    let Some((cx, cy, cw, ch)) = clip else {
        return true;
    };
    x + w > cx && x < cx + cw && y + h > cy && y < cy + ch
}

fn point_in(p: (f32, f32), node: &UiNode) -> bool {
    p.0 >= node.x && p.0 <= node.x + node.w && p.1 >= node.y && p.1 <= node.y + node.h
}

fn brighten(c: Color) -> Color {
    let up = |v: u8| ((v as u16 + 30).min(255)) as u8;
    Color(up(c.0), up(c.1), up(c.2), c.3)
}

/// Which row was clicked in a list/dropdown menu (`None` if on header-only area).
pub fn list_row_at(c: &Control, mouse_y: f32, scroll: f32, header_h: f32) -> Option<i32> {
    let y0 = c.y + header_h;
    if mouse_y < y0 || mouse_y > c.y + c.h {
        return None;
    }
    let rel = mouse_y - y0 + scroll;
    if rel < 0.0 {
        return None;
    }
    let row = (rel / c.row_h).floor() as i32;
    if row >= 0 && (row as usize) < c.items.len() {
        Some(row)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_lines_splits_long_text() {
        let lines = wrap_lines("abcdefghij", 20.0, 20); // ~2 chars per line (0.5em*20=10, 20/10=2)
        assert!(lines.len() >= 4, "got {:?}", lines);
    }

    #[test]
    fn clamp_scroll_bounds() {
        assert_eq!(clamp_scroll(-10.0, 200.0, 50.0), 0.0);
        assert_eq!(clamp_scroll(999.0, 200.0, 50.0), 150.0);
        assert_eq!(clamp_scroll(10.0, 40.0, 50.0), 0.0);
    }

    #[test]
    fn caret_vertical_moves_between_lines() {
        let text = "hi\nthere";
        let caret = 1; // in "hi"
        let down = move_caret_vertical(text, caret, 200.0, 20, false);
        assert!(down > caret);
        let up = move_caret_vertical(text, down, 200.0, 20, true);
        assert_eq!(up, caret);
    }
}
