//! What every screen shares: the interface scale, fonts, colors, panels and buttons.
//!
//! The interface is laid out for a 1280 × 800 window and drawn through a camera that
//! scales it to the real one, so that it keeps its proportions on any screen. Text is
//! rasterized at its final size in pixels, so it stays sharp however large it gets.

use std::cell::Cell;
use std::sync::OnceLock;

use macroquad::prelude::*;

pub const PANEL: Color = Color::new(0.06, 0.05, 0.08, 0.86);
pub const PANEL_EDGE: Color = Color::new(1.0, 1.0, 1.0, 0.12);
pub const TEXT: Color = Color::new(0.92, 0.9, 0.86, 1.0);
pub const DIM: Color = Color::new(0.6, 0.58, 0.55, 1.0);
pub const GOOD: Color = Color::new(0.45, 0.9, 0.45, 1.0);
pub const BAD: Color = Color::new(1.0, 0.4, 0.35, 1.0);
pub const GOLDEN: Color = Color::new(1.0, 0.82, 0.25, 1.0);
pub const BACKGROUND: Color = Color::new(0.05, 0.04, 0.06, 1.0);
const HIGHLIGHT: Color = Color::new(1.0, 0.82, 0.25, 0.16);

/// Size of the window the interface is laid out for.
const LAYOUT: Vec2 = vec2(1280.0, 800.0);
pub const MIN_USER_SCALE: f32 = 0.6;
pub const MAX_USER_SCALE: f32 = 2.0;

#[derive(Clone, Copy)]
pub enum Face {
    Regular,
    Bold,
    /// The serif of titles.
    Title,
}

struct Fonts {
    regular: Font,
    bold: Font,
    title: Font,
}

static FONTS: OnceLock<Fonts> = OnceLock::new();

thread_local! {
    /// The player's own enlargement, on top of the fit to the window.
    static USER_SCALE: Cell<f32> = const { Cell::new(1.0) };
}

pub fn load_fonts() {
    let load = |bytes| load_ttf_font_from_bytes(bytes).expect("embedded font");
    let fonts = Fonts {
        regular: load(include_bytes!("../assets/DejaVuSans.ttf")),
        bold: load(include_bytes!("../assets/DejaVuSans-Bold.ttf")),
        title: load(include_bytes!("../assets/DejaVuSerif-Bold.ttf")),
    };
    let _ = FONTS.set(fonts);
}

fn font(face: Face) -> Option<&'static Font> {
    let fonts = FONTS.get()?;
    Some(match face {
        Face::Regular => &fonts.regular,
        Face::Bold => &fonts.bold,
        Face::Title => &fonts.title,
    })
}

pub fn user_scale() -> f32 {
    USER_SCALE.get()
}

pub fn set_user_scale(s: f32) {
    USER_SCALE.set(s.clamp(MIN_USER_SCALE, MAX_USER_SCALE));
}

// macroquad scales its drawing space by the display's DPI factor (read from Xft.dpi).
// Under XWayland the desktop already enlarges the whole window, so that factor would
// apply twice: everything is drawn in the window's real pixels instead.

/// The window size, in real pixels.
pub fn pixels() -> Vec2 {
    vec2(screen_width(), screen_height()) * screen_dpi_scale()
}

pub fn mouse_pixels() -> Vec2 {
    Vec2::from(mouse_position()) * screen_dpi_scale()
}

/// Real pixels per interface unit.
pub fn scale() -> f32 {
    let fit = pixels() / LAYOUT;
    (fit.x.min(fit.y) * user_scale()).max(0.25)
}

/// The window size, in interface units.
pub fn size() -> Vec2 {
    pixels() / scale()
}

pub fn mouse() -> Vec2 {
    mouse_pixels() / scale()
}

/// Maps drawing coordinates onto interface units.
pub fn ui_camera() -> Camera2D {
    let s = size();
    Camera2D { target: s / 2.0, zoom: vec2(2.0, 2.0) / s, ..Default::default() }
}

/// Font size and scale that rasterize `size` units of text at its size in real pixels;
/// macroquad multiplies the font size by the DPI factor itself.
fn text_params(size: f32, face: Face, color: Color) -> TextParams<'static> {
    let px = (size * scale() / screen_dpi_scale()).round().max(1.0);
    TextParams { font: font(face), font_size: px as u16, font_scale: size / px, color, ..Default::default() }
}

pub fn text(t: &str, x: f32, y: f32, size: f32, color: Color) {
    text_in(t, x, y, size, Face::Regular, color);
}

pub fn text_in(t: &str, x: f32, y: f32, size: f32, face: Face, color: Color) {
    draw_text_ex(t, x, y, text_params(size, face, color));
}

pub fn measure(t: &str, size: f32, face: Face) -> f32 {
    let p = text_params(size, face, WHITE);
    measure_text(t, p.font, p.font_size, p.font_scale).width
}

/// Text centered on `x`.
pub fn centered(t: &str, x: f32, y: f32, size: f32, face: Face, color: Color) {
    text_in(t, x - measure(t, size, face) / 2.0, y, size, face, color);
}

/// Text with a drop shadow, for titles over busy backgrounds.
pub fn shadowed(t: &str, x: f32, y: f32, size: f32, face: Face, color: Color) {
    let off = (size / 18.0).max(1.5);
    let shadow = Color::new(0.0, 0.0, 0.0, 0.75 * color.a);
    text_in(t, x + off, y + off, size, face, shadow);
    text_in(t, x, y, size, face, color);
}

pub fn panel(r: Rect) {
    draw_rectangle(r.x, r.y, r.w, r.h, PANEL);
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, PANEL_EDGE);
}

/// A panel that lights up when selected, with a golden edge.
pub fn card(r: Rect, selected: bool) {
    panel(r);
    if selected {
        draw_rectangle(r.x, r.y, r.w, r.h, HIGHLIGHT);
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 3.0, GOLDEN);
    }
}

pub fn sprite(sheet: &Texture2D, index: u32, at: Vec2, size: f32, tint: Color) {
    let params = DrawTextureParams {
        dest_size: Some(vec2(size, size)),
        source: Some(crate::sprites::source(index)),
        ..Default::default()
    };
    draw_texture_ex(sheet, at.x, at.y, tint, params);
}

/// A rectangle fading from one color at `from` side to another at the opposite side;
/// `vertical` fades from top to bottom, otherwise from left to right.
pub fn gradient(r: Rect, from: Color, to: Color, vertical: bool) {
    let v = |x, y, c: Color| Vertex::new(x, y, 0.0, 0.0, 0.0, c);
    let (a, b, c, d) = if vertical { (from, from, to, to) } else { (from, to, to, from) };
    let mesh = Mesh {
        vertices: vec![v(r.x, r.y, a), v(r.x + r.w, r.y, b), v(r.x + r.w, r.y + r.h, c), v(r.x, r.y + r.h, d)],
        indices: vec![0, 1, 2, 0, 2, 3],
        texture: None,
    };
    draw_mesh(&mesh);
}

/// Darkens the edges of the screen, drawing the eye to the middle.
pub fn vignette(strength: f32) {
    let s = size();
    let dark = Color::new(0.0, 0.0, 0.0, strength);
    let clear = Color::new(0.0, 0.0, 0.0, 0.0);
    let (bx, by) = (s.x * 0.3, s.y * 0.3);
    gradient(Rect::new(0.0, 0.0, s.x, by), dark, clear, true);
    gradient(Rect::new(0.0, s.y - by, s.x, by), clear, dark, true);
    gradient(Rect::new(0.0, 0.0, bx, s.y), dark, clear, false);
    gradient(Rect::new(s.x - bx, 0.0, bx, s.y), clear, dark, false);
}
