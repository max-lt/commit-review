//! The summary: the commit message with its findings, what an amend keeps,
//! the changed files and the exact command.

use blit::{Sides, Sizing, WidgetId};
use blit_desktop::color::Color;
use blit_desktop::layout::{flex, single, Align};
use blit_desktop::text::{TextOptions, TextWrap};
use blit_desktop::widget::scroll;
use blit_desktop::{BoundsClip, Ui};

use crate::message::Kind;
use crate::{text, Context};

use super::marked::Marked;
use super::theme;
use super::widgets::{self, panel, Button, Look, ScrollArea};

#[derive(Default)]
pub struct Summary {
    body: scroll::State,
    stat: scroll::State,
    status: scroll::State,
    command: scroll::State,
    command_open: bool,
}

/// Builds the summary; returns the text of the badge the reviewer clicked,
/// for the notes.
pub fn build(ui: Ui<'_>, summary: &mut Summary, context: &Result<Context, String>) -> Option<String> {
    let mut column = ui.layout(flex::column().gap(theme::GAP));
    let context = match context {
        Ok(context) => context,
        Err(error) => {
            column.child(flex::item()).build(|ui: Ui<'_>| {
                boxed(ui, &format!("git error: {error}"), theme::colors().danger);
            });
            return None;
        }
    };
    let Summary { body, stat, status, command, command_open } = summary;
    let mut clicked = None;

    column.child(flex::item()).build(|ui: Ui<'_>| {
        let mut section = ui.layout(flex::column().gap(4.0));
        let heading = match &context.amend {
            Some(amend) if amend.message_kept => "COMMIT MESSAGE   kept as is",
            Some(_) => "COMMIT MESSAGE   replaces the current one",
            None => "COMMIT MESSAGE",
        };
        section.child(flex::item()).build(|ui: Ui<'_>| {
            let mut row = ui.layout(flex::row().gap(8.0).align(Align::Center));
            row.child(flex::item()).insert(widgets::text(heading, theme::sans(11.0), theme::colors().muted));
            let Some(findings) = &context.findings else {
                return;
            };
            for (field, found) in [("subject", &findings.subject), ("body", &findings.body)] {
                for kind in [Kind::NonAscii, Kind::Email, Kind::Link, Kind::CoAuthoredBy] {
                    let count = found.iter().filter(|finding| finding.kind == kind).count();
                    if count == 0 {
                        continue;
                    }
                    let badge = text::badge(field, kind, count);
                    let look = if kind == Kind::NonAscii { Look::Danger } else { Look::Warning };
                    let id = WidgetId::new(("badge", field, kind as u8));
                    let button = Button::new(id, &badge).look(look).style(theme::sans(11.0));
                    if row.child(flex::item()).build(button) {
                        clicked = Some(badge.clone());
                    }
                }
            }
        });
        section.child(flex::item()).build(|ui: Ui<'_>| {
            let mut field = ui.layout(single::layout().padding(Sides::xy(12.0, 8.0)));
            field.insert(panel(theme::colors().surface));
            let item = single::item().width(Sizing::grow());
            match (&context.message, &context.findings) {
                (Some(message), Some(findings)) => {
                    let (shown, marks) = text::marked(&message.subject, &findings.subject);
                    field.child(item).insert(Marked { text: &shown, marks: &marks, style: theme::bold(15.0), wrap: TextWrap::Word });
                }
                _ => {
                    let note = if context.command.is_some() {
                        "(message not recognized, see the exact command)"
                    } else {
                        "(manual launch, no command given)"
                    };
                    field.child(item).insert(widgets::text(note, theme::sans(theme::BODY), theme::colors().muted));
                }
            }
        });
        if let (Some(message), Some(findings)) = (&context.message, &context.findings) {
            if !message.body.is_empty() {
                let (shown, marks) = text::marked(&message.body, &findings.body);
                section.child(flex::item().height(Sizing::fit_range(0.0, 160.0))).build(|ui: Ui<'_>| {
                    framed(ui, body, |ui: Ui<'_>| {
                        let mut content = ui.layout(single::layout());
                        let marked = Marked { text: &shown, marks: &marks, style: theme::mono(theme::CODE), wrap: TextWrap::Word };
                        content.child(single::item().width(Sizing::grow())).insert(marked);
                    });
                });
            }
        }
    });

    if let Some(amend) = &context.amend {
        column.child(flex::item()).build(|ui: Ui<'_>| {
            let mut section = ui.layout(flex::column().gap(4.0));
            let heading = format!("ALREADY IN HEAD   {}", amend.head);
            section.child(flex::item()).insert(widgets::text(&heading, theme::sans(11.0), theme::colors().muted));
            section.child(flex::item().height(Sizing::fit_range(0.0, 120.0))).build(|ui: Ui<'_>| {
                code(ui, stat, &amend.stat, TextWrap::None);
            });
        });
    }

    column.child(flex::item().grow()).build(|ui: Ui<'_>| {
        let mut section = ui.layout(flex::column().gap(4.0));
        let (heading, empty) = if context.amend.is_some() {
            ("WORKING TREE", "(none: message or metadata only)")
        } else {
            ("FILES", "(no changes)")
        };
        section.child(flex::item()).insert(widgets::text(heading, theme::sans(11.0), theme::colors().muted));
        let shown = if context.status.is_empty() { empty } else { context.status.as_str() };
        section.child(flex::item().grow()).build(|ui: Ui<'_>| code(ui, status, shown, TextWrap::None));
    });

    if let Some(exact) = &context.command {
        column.child(flex::item()).build(|ui: Ui<'_>| {
            let mut section = ui.layout(flex::column().gap(4.0));
            section.child(flex::item()).build(|ui: Ui<'_>| {
                let mut row = ui.layout(flex::row());
                let label = if *command_open { "▾  Exact command" } else { "▸  Exact command" };
                let button = Button::new(WidgetId::new("exact command"), label).look(Look::Quiet).style(theme::sans(theme::SMALL));
                if row.child(flex::item()).build(button) {
                    *command_open = !*command_open;
                }
            });
            if *command_open {
                section.child(flex::item().height(Sizing::fit_range(0.0, 120.0))).build(|ui: Ui<'_>| {
                    code(ui, command, exact, TextWrap::Character);
                });
            }
        });
    }
    clicked
}

/// Monospace text in a bordered box that scrolls.
fn code(ui: Ui<'_>, state: &mut scroll::State, value: &str, wrap: TextWrap) {
    framed(ui, state, |ui: Ui<'_>| {
        let mut content = ui.layout(single::layout());
        let options = TextOptions { wrap, ..TextOptions::default() };
        let shown = widgets::text(value, theme::mono(theme::CODE), theme::colors().text).options(options);
        content.child(single::item().width(Sizing::grow())).insert(shown);
    });
}

/// A bordered box whose content scrolls.
fn framed<C: blit::Widget<blit_desktop::DesktopPlatform>>(ui: Ui<'_>, state: &mut scroll::State, content: C) {
    let mut field = ui.layout(single::layout().padding(Sides::xy(12.0, 8.0)));
    field.insert(panel(theme::colors().surface));
    field.child(single::item().grow()).build(ScrollArea::new(state, BoundsClip).build(content));
}

fn boxed(ui: Ui<'_>, value: &str, color: Color) {
    let mut field = ui.layout(single::layout().padding(Sides::xy(12.0, 8.0)));
    field.insert(panel(theme::colors().surface));
    field.child(single::item().width(Sizing::grow())).insert(widgets::wrapped(value, theme::mono(theme::CODE), color));
}
