//! Colors, sizes and text styles of the window. Two palettes after GitHub's
//! light and dark themes; the light one matches the webview version on
//! main. The choice persists in ~/.config/commit-review/theme.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use blit_desktop::color::Color;
use blit_desktop::text::{FontId, TextStyle};

/// The interface face and the code face, as registered in `ui::fonts`.
pub const SANS: FontId = FontId(0);
pub const MONO: FontId = FontId(1);

pub struct Palette {
    pub background: Color,
    pub surface: Color,
    pub raised: Color,
    pub border: Color,
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub danger: Color,
    pub warning: Color,
    pub success: Color,
    pub purple: Color,
    pub white: Color,
    pub add_line: Color,
    pub add_number: Color,
    pub del_line: Color,
    pub del_number: Color,
    pub hunk: Color,
    pub selected: Color,
    pub mark_danger: Color,
    pub mark_warning: Color,
}

const LIGHT: Palette = Palette {
    background: rgb(255, 255, 255),
    surface: rgb(240, 240, 240),
    raised: rgb(246, 246, 246),
    border: rgb(211, 211, 211),
    text: rgb(31, 35, 40),
    muted: rgb(101, 109, 118),
    accent: rgb(47, 111, 235),
    accent_hover: rgb(9, 105, 218),
    danger: rgb(209, 36, 47),
    warning: rgb(191, 135, 0),
    success: rgb(26, 127, 55),
    purple: rgb(130, 80, 223),
    white: rgb(255, 255, 255),
    add_line: rgb(230, 255, 236),
    add_number: rgb(204, 255, 216),
    del_line: rgb(255, 235, 233),
    del_number: rgb(255, 206, 203),
    hunk: rgb(221, 244, 255),
    selected: rgb(255, 248, 197),
    mark_danger: rgba(209, 36, 47, 110),
    mark_warning: rgba(191, 135, 0, 110),
};

const DARK: Palette = Palette {
    background: rgb(13, 17, 23),
    surface: rgb(22, 27, 34),
    raised: rgb(33, 38, 45),
    border: rgb(48, 54, 61),
    text: rgb(230, 237, 243),
    muted: rgb(139, 148, 158),
    accent: rgb(31, 111, 235),
    accent_hover: rgb(56, 139, 253),
    danger: rgb(248, 81, 73),
    warning: rgb(210, 153, 34),
    success: rgb(63, 185, 80),
    purple: rgb(163, 113, 247),
    white: rgb(255, 255, 255),
    add_line: rgba(46, 160, 67, 38),
    add_number: rgba(46, 160, 67, 77),
    del_line: rgba(248, 81, 73, 38),
    del_number: rgba(248, 81, 73, 77),
    hunk: rgba(56, 139, 253, 38),
    selected: rgba(187, 128, 9, 77),
    mark_danger: rgba(248, 81, 73, 110),
    mark_warning: rgba(210, 153, 34, 110),
};

static DARK_MODE: AtomicBool = AtomicBool::new(false);

pub fn colors() -> &'static Palette {
    if is_dark() { &DARK } else { &LIGHT }
}

pub fn is_dark() -> bool {
    DARK_MODE.load(Ordering::Relaxed)
}

/// Switches the palette and remembers the choice for the next window.
pub fn set_dark(dark: bool) {
    DARK_MODE.store(dark, Ordering::Relaxed);
    if let Some(path) = preference() {
        std::fs::create_dir_all(path.parent().expect("preference path has a directory")).ok();
        std::fs::write(path, if dark { "dark" } else { "light" }).ok();
    }
}

/// Reads the remembered choice; light without one, like the webview version.
pub fn load() {
    let dark = preference()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .is_some_and(|choice| choice.trim() == "dark");
    DARK_MODE.store(dark, Ordering::Relaxed);
}

fn preference() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config").join("commit-review").join("theme"))
}

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
