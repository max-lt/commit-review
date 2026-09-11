//! Colors, sizes and text styles of the window: the light palette of the
//! webview version on main, itself after GitHub's light theme.

use blit_desktop::color::Color;
use blit_desktop::text::{FontId, TextStyle};

/// The interface face and the code face, as registered in `ui::fonts`.
pub const SANS: FontId = FontId(0);
pub const MONO: FontId = FontId(1);

pub const BACKGROUND: Color = rgb(255, 255, 255);
pub const SURFACE: Color = rgb(240, 240, 240);
pub const RAISED: Color = rgb(246, 246, 246);
pub const BORDER: Color = rgb(211, 211, 211);
pub const TEXT: Color = rgb(31, 35, 40);
pub const MUTED: Color = rgb(101, 109, 118);
pub const ACCENT: Color = rgb(47, 111, 235);
pub const ACCENT_HOVER: Color = rgb(9, 105, 218);
pub const DANGER: Color = rgb(209, 36, 47);
pub const WARNING: Color = rgb(191, 135, 0);
pub const SUCCESS: Color = rgb(26, 127, 55);
pub const PURPLE: Color = rgb(130, 80, 223);
pub const WHITE: Color = rgb(255, 255, 255);

pub const ADD_LINE: Color = rgb(230, 255, 236);
pub const ADD_NUMBER: Color = rgb(204, 255, 216);
pub const DEL_LINE: Color = rgb(255, 235, 233);
pub const DEL_NUMBER: Color = rgb(255, 206, 203);
pub const HUNK: Color = rgb(221, 244, 255);
pub const SELECTED: Color = rgb(255, 248, 197);
pub const MARK_DANGER: Color = rgba(209, 36, 47, 110);
pub const MARK_WARNING: Color = rgba(191, 135, 0, 110);

pub const RADIUS: f32 = 6.0;
pub const GAP: f32 = 12.0;
pub const BODY: f32 = 14.0;
pub const SMALL: f32 = 12.0;
pub const CODE: f32 = 12.0;
/// Height of one diff line, numbers and code alike.
pub const LINE: f32 = 20.0;

const fn rgb(red: u8, green: u8, blue: u8) -> Color {
    Color::from_rgba8(red, green, blue, 255)
}

const fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Color {
    Color::from_rgba8(red, green, blue, alpha)
}

pub fn sans(size: f32) -> TextStyle {
    TextStyle { font: SANS, size, ..TextStyle::default() }
}

pub fn bold(size: f32) -> TextStyle {
    TextStyle { font: SANS, size, weight: 600, ..TextStyle::default() }
}

pub fn mono(size: f32) -> TextStyle {
    TextStyle { font: MONO, size, ..TextStyle::default() }
}
