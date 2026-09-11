//! The diff review, in the style of GitHub's "Files changed": a file tree
//! with a filter beside the files, each with its header, comments and diff.

use std::collections::{BTreeMap, HashSet};

use blit::{Input, PointerButton, Sides, Sizing, WidgetId};
use blit_desktop::atom::Rectangle;
use blit_desktop::layout::{flex, single, Align};
use blit_desktop::style::{Border, BorderRadius};
use blit_desktop::text::TextStyle;
use blit_desktop::widget::{scroll, text_input, TextInput};
use blit_desktop::{BoundsClip, Ui};

use crate::diff::{FileDiff, Kind, Status};
use crate::message::Scope;
use crate::state::{CommentAt, FileReview, Restored};
use crate::text::{self, Anchor};

use super::lines;
use super::theme;
use super::widgets::{self, panel, Button, Checkbox, Look, ScrollArea, Tag};

pub fn scope_label(scope: Scope) -> &'static str {
    match scope {
        Scope::Staged => "Staged changes only: plain git commit",
        Scope::Tracked => "Tracked files as they are: git commit -a",
        Scope::Worktree => "Working tree, untracked files included: git add runs first",
    }
}

pub(super) struct File {
    pub(super) diff: FileDiff,
    pub(super) viewed: bool,
    pub(super) collapsed: bool,
    /// Pending comments, sent with the decision.
    pub(super) comments: Vec<Comment>,
    /// Comments of an earlier attempt whose lines changed: not sent unless reopened.
    pub(super) outdated: Vec<Outdated>,
}

pub(super) struct Comment {
    pub(super) anchor: Anchor,
    pub(super) text: String,
    /// Carried over from an earlier attempt.
    pub(super) earlier: bool,
}

pub(super) struct Outdated {
    pub(super) quote: Vec<String>,
    pub(super) text: String,
}

/// The comment box being written; one at a time.
pub(super) struct Form {
    pub(super) file: usize,
    pub(super) anchor: Anchor,
    pub(super) text: String,
    pub(super) state: text_input::State,
    /// The index of the comment being edited, in its file.
    pub(super) editing: Option<usize>,
    /// Takes the keyboard focus once built.
    pub(super) focus: bool,
}

impl Form {
    pub(super) fn new(file: usize, anchor: Anchor, text: String, editing: Option<usize>) -> Self {
        let end = text.len();
        let state = text_input::State { cursor: end, anchor: end, offset_x: 0.0 };
        Form { file, anchor, text, state, editing, focus: true }
    }
}

/// Lines selected by dragging a "+" button, flat indices in one file.
#[derive(Clone, Copy)]
pub(super) struct Drag {
    pub(super) file: usize,
    pub(super) start: usize,
    pub(super) end: usize,
}

impl From<crate::Change> for File {
    fn from(change: crate::Change) -> Self {
        let mut file = File {
            collapsed: change.viewed,
            viewed: change.viewed,
            diff: change.diff,
            comments: Vec::new(),
            outdated: Vec::new(),
        };
        for restored in change.restored {
            match restored {
                Restored::Lines { start, end, text } => {
                    file.comments.push(Comment { anchor: Anchor::Lines { start, end }, text, earlier: true })
                }
                Restored::File { text } => file.comments.push(Comment { anchor: Anchor::File, text, earlier: true }),
                Restored::Outdated { quote, text } => file.outdated.push(Outdated { quote, text }),
            }
        }
        file
    }
}

#[derive(Default)]
pub struct Review {
    /// Read from git when the reviewer first opens the review.
    files: Option<Result<Vec<File>, String>>,
    tree: scroll::State,
    list: scroll::State,
    filter: String,
    filter_state: text_input::State,
    closed_dirs: HashSet<String>,
    drag: Option<Drag>,
    form: Option<Form>,
    /// A file picked in the tree, scrolled to once laid out.
    reveal: Option<usize>,
}

impl Review {
    pub fn open(&mut self, command: Option<&str>) {
        if self.files.is_none() {
            self.files = Some(crate::changes(command).map(|changes| changes.into_iter().map(File::from).collect()));
        }
    }

    fn loaded(&self) -> &[File] {
        match &self.files {
            Some(Ok(files)) => files,
            _ => &[],
        }
    }

    pub fn pending(&self) -> usize {
        self.loaded().iter().map(|file| file.comments.len()).sum()
    }

    pub fn viewed(&self) -> (usize, usize) {
        let files = self.loaded();
        (files.iter().filter(|file| file.viewed).count(), files.len())
    }

    /// The pending comments as the agent reads them.
    pub fn comment_texts(&self) -> Vec<String> {
        self.loaded()
            .iter()
            .flat_map(|file| file.comments.iter().map(|comment| text::comment(&file.diff, comment.anchor, &comment.text)))
            .collect()
    }

    /// What to keep for the next attempt; nothing when no diff was shown.
    pub fn reviews(&self) -> Option<(Vec<FileDiff>, Vec<FileReview>)> {
        let files = self.loaded();
        if files.is_empty() {
            return None;
        }
        let reviews = files
            .iter()
            .map(|file| FileReview {
                path: file.diff.path.clone(),
                viewed: file.viewed,
                comments: file
                    .comments
                    .iter()
                    .map(|comment| {
                        let (start, end) = match comment.anchor {
                            Anchor::File => (None, None),
                            Anchor::Lines { start, end } => (Some(start), Some(end)),
                        };
                        CommentAt { start, end, text: comment.text.clone() }
                    })
                    .collect(),
            })
            .collect();
        Some((files.iter().map(|file| file.diff.clone()).collect(), reviews))
    }
}

pub fn build(ui: Ui<'_>, review: &mut Review, user: &str, consumed: &mut bool) {
    let Review { files, tree, list, filter, filter_state, closed_dirs, drag, form, reveal } = review;
    let mut row = ui.layout(flex::row().gap(16.0));
    let files = match files {
        Some(Ok(files)) => files,
        Some(Err(error)) => {
            let message = format!("git error: {error}");
            let shown = widgets::wrapped(&message, theme::mono(theme::CODE), theme::colors().danger);
            row.child(flex::item().width(Sizing::grow())).insert(shown);
            return;
        }
        None => return,
    };
    // A drag over the "+" buttons ends where the pointer is released.
    if let (Some(selected), Input::PointerUp { button: PointerButton::Primary, .. }) = (*drag, *row.input()) {
        *drag = None;
        let anchor = Anchor::Lines { start: selected.start.min(selected.end), end: selected.start.max(selected.end) };
        *form = Some(Form::new(selected.file, anchor, String::new(), None));
    }
    let query = filter.trim().to_lowercase();
    let shown: Vec<bool> = files.iter().map(|file| file.diff.path.to_lowercase().contains(&query)).collect();

    let mut picked = None;
    row.child(flex::item().width(Sizing::fixed(240.0)).height(Sizing::grow())).build(|ui: Ui<'_>| {
        let mut pane = ui.layout(flex::column().gap(8.0));
        pane.child(flex::item()).build(|ui: Ui<'_>| {
            let mut field = ui.layout(single::layout().padding(Sides::xy(10.0, 6.0)));
            field.insert(panel(theme::colors().surface));
            let input = TextInput::new(filter_state, WidgetId::new("filter"), filter)
                .style(theme::sans(13.0))
                .color(theme::colors().text)
                .placeholder("Filter files...")
                .placeholder_color(theme::colors().muted)
                .selection_background(theme::colors().selected)
                .cursor_background(theme::colors().text);
            field.child(single::item().width(Sizing::grow())).build(input);
        });
        pane.child(flex::item().grow()).build(ScrollArea::new(tree, BoundsClip).build(|ui: Ui<'_>| {
            let mut entries = ui.layout(flex::column().gap(1.0));
            for entry in tree_entries(files, &shown, closed_dirs) {
                let (id, label) = match &entry {
                    Entry::Dir { path, name, depth, open } => {
                        (WidgetId::new(("tree dir", path)), format!("{}{}  {name}", indent(*depth), if *open { "▾" } else { "▸" }))
                    }
                    Entry::File { index, name, depth } => {
                        let tick = if files[*index].viewed { "  ✓" } else { "" };
                        (WidgetId::new(("tree file", *index)), format!("{}    {name}{tick}", indent(*depth)))
                    }
                };
                let button = Button::new(id, &label).look(Look::Quiet).style(theme::sans(13.0));
                if entries.child(flex::item()).build(button) {
                    match entry {
                        Entry::Dir { path, .. } => {
                            if !closed_dirs.remove(&path) {
                                closed_dirs.insert(path);
                            }
                        }
                        Entry::File { index, .. } => picked = Some(index),
                    }
                }
            }
        }));
    });

    if let Some(index) = picked {
        files[index].collapsed = false;
        *reveal = Some(index);
    }
    if let Some(index) = *reveal {
        let content = list.id.child("content");
        if let (Some(target), Some(top)) = (row.geometry(file_id(index)), row.geometry(content)) {
            list.scroll_to(target.y - top.y);
            *reveal = None;
        }
        row.request_frame();
    }

    // Rows are built only near the viewport, measured in content coordinates
    // from last frame's layout; a margin of one viewport keeps the next
    // screen ready while scrolling.
    let margin = list.viewport_extent.max(400.0);
    let (window_top, window_bottom) = (list.offset - margin, list.offset + list.viewport_extent + margin);
    let content_top = row.geometry(list.id.child("content")).map(|content| content.y);
    row.child(flex::item().grow()).build(ScrollArea::new(list, BoundsClip).build(|ui: Ui<'_>| {
        let mut column = ui.layout(flex::column().gap(GAP).padding(Sides::new().right(10.0)));
        if files.is_empty() {
            let note = widgets::text("(nothing to commit in this scope)", theme::sans(theme::BODY), theme::colors().muted);
            column.child(flex::item()).insert(note);
        }
        let mut estimate = 0.0;
        for (index, file) in files.iter_mut().enumerate() {
            if !shown[index] {
                continue;
            }
            let top = match (column.geometry(file_id(index)), content_top) {
                (Some(area), Some(content)) => area.y - content,
                _ => estimate,
            };
            estimate = top + lines::height_estimate(file) + GAP;
            let place = lines::Place { top, window_top, window_bottom };
            column
                .child(flex::item())
                .widget_id(file_id(index))
                .build(|ui: Ui<'_>| file_box(ui, index, file, drag, form, user, consumed, place));
        }
    }));
}

/// Space between file boxes.
const GAP: f32 = 14.0;

fn file_id(index: usize) -> WidgetId {
    WidgetId::new(("file", index))
}

fn indent(depth: usize) -> String {
    "    ".repeat(depth)
}

#[allow(clippy::too_many_arguments)]
fn file_box(
    ui: Ui<'_>,
    index: usize,
    file: &mut File,
    drag: &mut Option<Drag>,
    form: &mut Option<Form>,
    user: &str,
    consumed: &mut bool,
    place: lines::Place,
) {
    let mut boxed = ui.layout(flex::column());
    boxed.insert(Rectangle::new().border(Border::solid(1.0, theme::colors().border)).radius(BorderRadius::uniform(theme::RADIUS)));
    boxed
        .child(flex::item())
        .widget_id(lines::header_id(index))
        .build(|ui: Ui<'_>| header(ui, index, file, form));
    if !file.collapsed {
        boxed.child(flex::item()).build(|ui: Ui<'_>| lines::body(ui, index, file, drag, form, user, consumed, place));
    }
}

fn header(ui: Ui<'_>, index: usize, file: &mut File, form: &mut Option<Form>) {
    let mut row = ui.layout(flex::row().padding(Sides::xy(10.0, 6.0)).gap(10.0).align(Align::Center));
    let radius = if file.collapsed {
        BorderRadius::uniform(theme::RADIUS)
    } else {
        BorderRadius::new().top_left(theme::RADIUS).top_right(theme::RADIUS)
    };
    row.insert(Rectangle::new().background(theme::colors().surface).radius(radius));
    let chevron = if file.collapsed { "▸" } else { "▾" };
    if row.child(flex::item()).build(Button::new(WidgetId::new(("chevron", index)), chevron).look(Look::Quiet)) {
        file.collapsed = !file.collapsed;
    }
    let (status, color) = match file.diff.status {
        Status::Added => ("added", theme::colors().success),
        Status::Deleted => ("deleted", theme::colors().danger),
        Status::Renamed => ("renamed", theme::colors().purple),
        Status::Modified => ("modified", theme::colors().warning),
    };
    row.child(flex::item()).build(Tag { label: status, color });
    let path = match &file.diff.old_path {
        Some(old) => format!("{old} -> {}", file.diff.path),
        None => file.diff.path.clone(),
    };
    let bold = TextStyle { weight: 600, ..theme::mono(theme::CODE) };
    row.child(flex::item().width(Sizing::grow())).insert(widgets::text(&path, bold, theme::colors().text));
    let added = file.diff.lines().filter(|line| line.kind == Kind::Add).count();
    let removed = file.diff.lines().filter(|line| line.kind == Kind::Del).count();
    row.child(flex::item()).insert(widgets::text(&format!("+{added}"), theme::mono(theme::CODE), theme::colors().success));
    row.child(flex::item()).insert(widgets::text(&format!("-{removed}"), theme::mono(theme::CODE), theme::colors().danger));
    if row.child(flex::item()).build(Checkbox { id: WidgetId::new(("viewed", index)), label: "Viewed", checked: file.viewed }) {
        file.viewed = !file.viewed;
        file.collapsed = file.viewed;
    }
    let comment = Button::new(WidgetId::new(("file comment", index)), "Comment").look(Look::Quiet).style(theme::sans(theme::SMALL));
    if row.child(flex::item()).build(comment) {
        file.collapsed = false;
        *form = Some(Form::new(index, Anchor::File, String::new(), None));
    }
}

enum Entry {
    Dir { path: String, name: String, depth: usize, open: bool },
    File { index: usize, name: String, depth: usize },
}

#[derive(Default)]
struct Node {
    dirs: BTreeMap<String, Node>,
    files: Vec<(String, usize)>,
}

/// The tree rows for the shown files: directories first, then files, both
/// sorted, without the contents of closed directories.
fn tree_entries(files: &[File], shown: &[bool], closed: &HashSet<String>) -> Vec<Entry> {
    let mut root = Node::default();
    for (index, file) in files.iter().enumerate().filter(|(index, _)| shown[*index]) {
        let mut parts: Vec<&str> = file.diff.path.split('/').collect();
        let name = parts.pop().unwrap_or_default();
        let node = parts.iter().fold(&mut root, |node, dir| node.dirs.entry(dir.to_string()).or_default());
        node.files.push((name.to_string(), index));
    }
    let mut entries = Vec::new();
    walk(&root, "", 0, closed, &mut entries);
    entries
}

fn walk(node: &Node, prefix: &str, depth: usize, closed: &HashSet<String>, entries: &mut Vec<Entry>) {
    for (name, child) in &node.dirs {
        let path = format!("{prefix}{name}/");
        let open = !closed.contains(&path);
        entries.push(Entry::Dir { path: path.clone(), name: name.clone(), depth, open });
        if open {
            walk(child, &path, depth + 1, closed, entries);
        }
    }
    let mut files = node.files.clone();
    files.sort();
    entries.extend(files.into_iter().map(|(name, index)| Entry::File { index, name, depth }));
}
