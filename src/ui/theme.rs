//! Colors, sizes and text styles of the window, after GitHub's dark theme.

use blit_desktop::color::Color;
use blit_desktop::text::{FontId, TextStyle};

/// The interface face and the code face, as registered in `ui::fonts`.
pub const SANS: FontId = FontId(0);
pub const MONO: FontId = FontId(1);

pub const BACKGROUND: Color = rgb(13, 17, 23);
pub const SURFACE: Color = rgb(22, 27, 34);
pub const RAISED: Color = rgb(33, 38, 45);
pub const BORDER: Color = rgb(48, 54, 61);
pub const TEXT: Color = rgb(230, 237, 243);
pub const MUTED: Color = rgb(139, 148, 158);
pub const ACCENT: Color = rgb(31, 111, 235);
pub const ACCENT_HOVER: Color = rgb(56, 139, 253);
pub const DANGER: Color = rgb(248, 81, 73);
pub const WARNING: Color = rgb(210, 153, 34);
pub const SUCCESS: Color = rgb(63, 185, 80);
pub const PURPLE: Color = rgb(163, 113, 247);
pub const WHITE: Color = rgb(255, 255, 255);

pub const ADD_LINE: Color = rgba(46, 160, 67, 38);
pub const ADD_NUMBER: Color = rgba(46, 160, 67, 77);
pub const DEL_LINE: Color = rgba(248, 81, 73, 38);
pub const DEL_NUMBER: Color = rgba(248, 81, 73, 77);
pub const HUNK: Color = rgba(56, 139, 253, 38);
pub const SELECTED: Color = rgba(187, 128, 9, 77);
pub const MARK_DANGER: Color = rgba(248, 81, 73, 110);
pub const MARK_WARNING: Color = rgba(210, 153, 34, 110);

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
