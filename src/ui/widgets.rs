//! Small widgets the views share: buttons, tags, a checkbox, scroll areas.

use blit::{Interaction, Sense, Sides, Widget, WidgetId};
use blit_desktop::atom::Rectangle;
use blit_desktop::color::Color;
use blit_desktop::layout::{flex, Align};
use blit_desktop::style::{Border, BorderRadius};
use blit_desktop::text::{TextOptions, TextStyle, TextWrap};
use blit_desktop::widget::{scroll, Text};
use blit_desktop::{BoundsClip, DesktopPlatform, Ui};

use super::theme;

/// A line of text, clipped where it does not fit.
pub fn text(value: &str, style: TextStyle, color: Color) -> Text<'_> {
    Text::new(value).style(style).color(color)
}

/// Text that wraps at word boundaries to the width it is given.
pub fn wrapped(value: &str, style: TextStyle, color: Color) -> Text<'_> {
    text(value, style, color).options(TextOptions { wrap: TextWrap::Word, ..TextOptions::default() })
}

/// A bordered surface behind a group of content.
pub fn panel(background: Color) -> Rectangle {
    Rectangle::new()
        .background(background)
        .border(Border::solid(1.0, theme::colors().border))
        .radius(BorderRadius::uniform(theme::RADIUS))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Plain,
    Primary,
    Danger,
    Warning,
    /// No border until hovered: icons and links.
    Quiet,
}

/// A labeled button; its response is whether it was clicked.
pub struct Button<'a> {
    id: WidgetId,
    label: &'a str,
    look: Look,
    style: TextStyle,
}

impl<'a> Button<'a> {
    pub fn new(id: WidgetId, label: &'a str) -> Self {
        Self { id, label, look: Look::Plain, style: theme::sans(13.0) }
    }

    pub fn look(mut self, look: Look) -> Self {
        self.look = look;
        self
    }

    pub fn style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }
}

impl Widget<DesktopPlatform> for Button<'_> {
    type Response = bool;

    fn build(self, mut ui: Ui<'_>) -> bool {
        let interaction = ui.interact(self.id, Sense::CLICK);
        let (background, border, color) = colors(self.look, interaction);
        let padding = if self.look == Look::Quiet { Sides::xy(6.0, 2.0) } else { Sides::xy(12.0, 5.0) };
        let mut button = ui.widget_id(self.id).layout(flex::row().padding(padding).align(Align::Center));
        button.insert(
            Rectangle::new()
                .background(background)
                .border(border.map_or(Border::None, |color| Border::solid(1.0, color)))
                .radius(BorderRadius::uniform(theme::RADIUS)),
        );
        button.child(flex::item()).insert(text(self.label, self.style, color));
        interaction.clicked
    }
}

fn colors(look: Look, interaction: Interaction) -> (Color, Option<Color>, Color) {
    let lit = interaction.hovered || interaction.active;
    match look {
        Look::Plain => (if lit { theme::colors().border } else { theme::colors().raised }, Some(theme::colors().border), theme::colors().text),
        Look::Primary => (if lit { theme::colors().accent_hover } else { theme::colors().accent }, None, theme::colors().white),
        Look::Danger => (if lit { theme::colors().raised } else { theme::colors().surface }, Some(theme::colors().danger), theme::colors().danger),
        Look::Warning => (if lit { theme::colors().raised } else { theme::colors().surface }, Some(theme::colors().warning), theme::colors().warning),
        Look::Quiet => (if lit { theme::colors().raised } else { Color::TRANSPARENT }, None, theme::colors().muted),
    }
}

/// A small rounded label: Pending, Outdated, a file status.
pub struct Tag<'a> {
    pub label: &'a str,
    pub color: Color,
}

impl Widget<DesktopPlatform> for Tag<'_> {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let mut tag = ui.layout(flex::row().padding(Sides::xy(7.0, 1.0)));
        tag.insert(Rectangle::new().border(Border::solid(1.0, self.color)).radius(BorderRadius::uniform(10.0)));
        tag.child(flex::item()).insert(text(self.label, theme::sans(11.0), self.color));
    }
}

/// A checkbox with its label; its response is whether it was toggled.
pub struct Checkbox<'a> {
    pub id: WidgetId,
    pub label: &'a str,
    pub checked: bool,
}

impl Widget<DesktopPlatform> for Checkbox<'_> {
    type Response = bool;

    fn build(self, mut ui: Ui<'_>) -> bool {
        let interaction = ui.interact(self.id, Sense::CLICK);
        let mut row = ui
            .widget_id(self.id)
            .layout(flex::row().padding(Sides::xy(8.0, 3.0)).gap(6.0).align(Align::Center));
        row.insert(
            Rectangle::new()
                .background(if interaction.hovered { theme::colors().raised } else { Color::TRANSPARENT })
                .border(Border::solid(1.0, theme::colors().border))
                .radius(BorderRadius::uniform(theme::RADIUS)),
        );
        row.child(flex::item().fixed(14.0, 14.0)).build(|ui: Ui<'_>| {
            let mut tick = ui.layout(flex::row().align(Align::Center).justify(blit_desktop::layout::Justify::Center));
            tick.insert(
                Rectangle::new()
                    .background(if self.checked { theme::colors().accent } else { Color::TRANSPARENT })
                    .border(Border::solid(1.0, if self.checked { theme::colors().accent } else { theme::colors().muted }))
                    .radius(BorderRadius::uniform(3.0)),
            );
            if self.checked {
                tick.child(flex::item()).insert(text("✓", theme::bold(11.0), theme::colors().white));
            }
        });
        row.child(flex::item()).insert(text(self.label, theme::sans(theme::SMALL), theme::colors().text));
        interaction.clicked
    }
}

/// The thumb of a scroll area, drawn over the content.
#[derive(Clone, Copy, Default)]
pub struct Thumb;

pub type ScrollArea<'a, C = ()> = scroll::Area<'a, DesktopPlatform, BoundsClip, Thumb, C>;

impl scroll::Scrollbar for Thumb {
    const HAS_TRACK: bool = false;
    const HAS_THUMB: bool = true;

    type Track = ();
    type Thumb = Rectangle;

    fn config(&self) -> scroll::Config {
        scroll::Config::new().scroll_speed(1.0).scrollbar_thickness(6.0).minimum_thumb_extent(24.0)
    }

    fn into_content(self, active: bool) -> (Self::Track, Self::Thumb) {
        let color = if active { theme::colors().muted } else { theme::colors().border };
        ((), Rectangle::new().background(color).radius(BorderRadius::uniform(3.0)))
    }
}
