//! The review window, drawn with blit: the summary or the diff review, the
//! notes for the agent, and the decision.

mod lines;
mod marked;
mod review;
mod summary;
mod text_area;
mod theme;
mod widgets;

use std::cell::RefCell;

use blit::{Input, Key, Sides, Sizing, WidgetId};
use blit_cpu::{BackendFontFaceId, FontFace, RendererConfig, TextLayoutEngine};
use blit_desktop::atom::Rectangle;
use blit_desktop::layout::{flex, single, Align};
use blit_desktop::widget::text_input;
use blit_desktop::{Application, Config, DesktopPlatform, EventLoopProxy, Root, Ui};
use blit_text::{FontStyle, SystemFontRequest};

use crate::{Context, Output};
use text_area::TextArea;
use widgets::{panel, text, Button, Look};

thread_local! {
    /// What `run` hands over to the app, which blit builds without arguments.
    static LAUNCH: RefCell<Option<(Option<String>, Output)>> = const { RefCell::new(None) };
}

/// Candidate families, the first one installed wins: interface, then code.
const SANS: &[&str] = &["Inter", "Cantarell", "Noto Sans", "DejaVu Sans", "Liberation Sans", "Helvetica Neue", "Helvetica", "Arial"];
const MONO: &[&str] = &["JetBrains Mono", "Fira Code", "DejaVu Sans Mono", "Noto Sans Mono", "Liberation Mono", "SF Mono", "Menlo", "Monaco"];

const ACCEPT: &str = if cfg!(target_os = "macos") { "Accept   cmd+enter" } else { "Accept   ctrl+enter" };

/// Opens the window. Every decision leaves the process from inside it, so
/// this returns only when the window is closed without one.
pub fn run(command: Option<String>, output: Output) -> Result<(), String> {
    theme::load();
    let mut engine: Box<dyn TextLayoutEngine> = Box::new(blit_text_cosmic::Backend::new());
    let fonts = fonts(engine.as_mut())?;
    LAUNCH.with(|launch| *launch.borrow_mut() = Some((command, output)));
    blit_desktop::run::<App>(Config {
        title: "Commit review".into(),
        width: 1100,
        height: 800,
        renderer: RendererConfig {
            fonts,
            text_cache_capacity: 1 << 20,
            layout_cache_capacity: 2 << 20,
            glyph_cache_capacity: 1 << 20,
            shadow_cache_capacity: 1 << 19,
        },
        text: engine,
    })
    .map_err(|e| e.to_string())
}

/// A regular and a semibold face for each of the two fonts, from the system.
fn fonts(engine: &mut dyn TextLayoutEngine) -> Result<Vec<FontFace>, String> {
    let mut faces = Vec::new();
    for (id, families) in [(theme::SANS, SANS), (theme::MONO, MONO)] {
        let (family, regular) = families
            .iter()
            .find_map(|family| Some((*family, system_font(engine, family, 400)?)))
            .ok_or_else(|| format!("no font found among {}", families.join(", ")))?;
        let bold = system_font(engine, family, 600).unwrap_or(regular);
        for (weight, face) in [(400, regular), (600, bold)] {
            faces.push(FontFace { id, weight, stretch: 100, style: FontStyle::Normal, face });
        }
    }
    Ok(faces)
}

fn system_font(engine: &mut dyn TextLayoutEngine, family: &str, weight: u16) -> Option<BackendFontFaceId> {
    engine.system_font(SystemFontRequest { family, weight, stretch: 100, style: FontStyle::Normal }).ok()
}

enum Action {
    Toggle,
    Deny,
    Accept,
}

pub struct App {
    command: Option<String>,
    output: Output,
    context: Result<Context, String>,
    summary: summary::Summary,
    review: review::Review,
    reviewing: bool,
    notes: String,
    notes_state: text_input::State,
    started: bool,
}

impl Application for App {
    type Input = ();

    fn new(_: EventLoopProxy<()>, _: Root<Self>, _: &mut DesktopPlatform) -> Self {
        let (command, output) = LAUNCH.with(|launch| launch.borrow_mut().take()).expect("launch set by run");
        App {
            context: crate::context(command.as_deref()),
            command,
            output,
            summary: summary::Summary::default(),
            review: review::Review::default(),
            reviewing: false,
            notes: String::new(),
            notes_state: text_input::State::default(),
            started: false,
        }
    }

    fn input(&mut self, _: ()) {}

    fn render(&mut self, ui: Ui<'_>) {
        let notes_id = WidgetId::new("notes");
        let mut root = ui.layout(flex::column().padding(Sides::xy(24.0, 18.0)).gap(theme::GAP));
        root.insert(Rectangle::new().background(theme::colors().background));
        if !self.started {
            self.started = true;
            root.focus(notes_id);
        }

        let title = match &self.context {
            Ok(Context { amend: Some(amend), .. }) => {
                format!("The agent wants to amend {}", amend.head.split(' ').next().unwrap_or_default())
            }
            _ => "The agent wants to commit".to_string(),
        };
        root.child(flex::item()).insert(text(&title, theme::bold(16.0), theme::colors().text));
        root.child(flex::item()).build(|ui: Ui<'_>| {
            let mut line = ui.layout(flex::row().gap(16.0).align(Align::End));
            let repo = self.context.as_ref().map_or("", |context| context.repo.as_str());
            line.child(flex::item().width(Sizing::grow())).insert(text(repo, theme::sans(theme::SMALL), theme::colors().muted));
            if self.reviewing {
                if let Ok(context) = &self.context {
                    line.child(flex::item()).insert(text(review::scope_label(context.scope), theme::sans(theme::SMALL), theme::colors().muted));
                }
                let (viewed, total) = self.review.viewed();
                line.child(flex::item()).insert(text(&format!("{viewed} / {total} viewed"), theme::sans(theme::SMALL), theme::colors().muted));
            }
            let label = if theme::is_dark() { "Light" } else { "Dark" };
            let switch = Button::new(WidgetId::new("theme"), label).look(Look::Quiet).style(theme::sans(theme::SMALL));
            if line.child(flex::item()).build(switch) {
                theme::set_dark(!theme::is_dark());
            }
        });

        let mut consumed = false;
        let user = self.context.as_ref().map_or("You", |context| context.user.as_str());
        if self.reviewing {
            let review = &mut self.review;
            root.child(flex::item().grow()).build(|ui: Ui<'_>| review::build(ui, review, user, &mut consumed));
        } else if let Some(line) = root
            .child(flex::item().grow())
            .build(|ui: Ui<'_>| summary::build(ui, &mut self.summary, &self.context))
        {
            append_line(&mut self.notes, &line);
            self.notes_state.cursor = self.notes.len();
            self.notes_state.anchor = self.notes.len();
            root.focus(notes_id);
        }

        let notes = root.child(flex::item()).build(|ui: Ui<'_>| {
            let mut section = ui.layout(flex::column().gap(4.0));
            section.child(flex::item()).insert(text(
                "NOTES FOR THE AGENT   optional, sent with either decision",
                theme::sans(11.0),
                theme::colors().muted,
            ));
            section.child(flex::item()).build(|ui: Ui<'_>| {
                let mut field = ui.layout(single::layout().padding(Sides::xy(12.0, 8.0)));
                field.insert(panel(theme::colors().surface));
                field.child(single::item().grow()).build(TextArea {
                    state: &mut self.notes_state,
                    id: notes_id,
                    value: &mut self.notes,
                    placeholder: "e.g. split the refactor from the fix; the message does not say why",
                    rows: 3,
                })
            })
        });

        let mut action = None;
        if notes.escaped {
            action = Some(Action::Deny);
        } else if notes.submitted {
            action = Some(Action::Accept);
        }
        root.child(flex::item()).build(|ui: Ui<'_>| {
            let mut row = ui.layout(flex::row().gap(10.0).align(Align::Center));
            let pending = match self.review.pending() {
                0 => String::new(),
                1 => "1 pending comment".to_string(),
                n => format!("{n} pending comments"),
            };
            row.child(flex::item().width(Sizing::grow())).insert(text(&pending, theme::sans(theme::SMALL), theme::colors().muted));
            let toggle = if self.reviewing { "Summary" } else { "Review changes" };
            if row.child(flex::item()).build(Button::new(WidgetId::new("toggle review"), toggle)) {
                action = Some(Action::Toggle);
            }
            if row.child(flex::item()).build(Button::new(WidgetId::new("deny"), "Deny   esc").look(Look::Danger)) {
                action = Some(Action::Deny);
            }
            if row.child(flex::item()).build(Button::new(WidgetId::new("accept"), ACCEPT).look(Look::Primary)) {
                action = Some(Action::Accept);
            }
        });

        if let (false, Input::Key(key)) = (consumed, *root.input()) {
            let command = key.modifiers.control() || key.modifiers.super_key();
            match key.key {
                Key::Escape if key.pressed => action = action.or(Some(Action::Deny)),
                Key::Enter if key.pressed && command => action = action.or(Some(Action::Accept)),
                _ => {}
            }
        }
        match action {
            Some(Action::Toggle) => {
                self.reviewing = !self.reviewing;
                if self.reviewing {
                    self.review.open(self.command.as_deref());
                }
                root.request_frame();
            }
            Some(Action::Deny) => self.decide(false),
            Some(Action::Accept) => self.decide(true),
            None => {}
        }
    }
}

impl App {
    /// Leaves with the decision, the notes and the pending comments.
    fn decide(&self, accept: bool) -> ! {
        let text = crate::text::decision(accept, &self.notes, &self.review.comment_texts());
        match self.review.reviews() {
            Some((files, reviews)) => crate::decide(self.output, accept, &text, Some((&files, reviews))),
            None => crate::decide(self.output, accept, &text, None),
        }
    }
}

/// Adds a line to the notes, on a line of its own.
fn append_line(notes: &mut String, line: &str) {
    if !notes.is_empty() && !notes.ends_with('\n') {
        notes.push('\n');
    }
    notes.push_str(line);
    notes.push('\n');
}
