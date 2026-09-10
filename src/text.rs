//! Text the window shows and the agent reads: the decision, the pending
//! comments quoted against the diff, the finding badges and marks.

use std::ops::Range;

use crate::diff::{FileDiff, Line};
use crate::message::{Finding, Kind};

/// The deny reason when the reviewer wrote nothing.
pub const DEFAULT_DENY: &str = "Commit denied by human review, no reason given. Do not retry the commit: ask the reviewer what should change.";

/// Where a comment sits: the whole file, or flat line indices, `end` included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    File,
    Lines { start: usize, end: usize },
}

/// The text a decision carries: the notes, then each comment.
pub fn decision(accept: bool, notes: &str, comments: &[String]) -> String {
    let parts: Vec<&str> = std::iter::once(notes.trim())
        .chain(comments.iter().map(String::as_str))
        .filter(|part| !part.is_empty())
        .collect();
    let body = parts.join("\n\n");
    match (accept, body.is_empty()) {
        (false, true) => DEFAULT_DENY.to_string(),
        (false, false) => format!("Commit denied by human review. Reason:\n{body}"),
        (true, true) => String::new(),
        (true, false) => format!("Commit accepted by human review, with notes:\n{body}"),
    }
}

/// One comment as the agent reads it: where, the quoted lines, the text.
pub fn comment(file: &FileDiff, anchor: Anchor, text: &str) -> String {
    match anchor {
        Anchor::File => format!("{}\n{text}", location(file, anchor)),
        Anchor::Lines { start, end } => {
            let quoted: Vec<String> = lines(file, start, end)
                .map(|line| format!("> {}", line.quoted()))
                .collect();
            format!("{}\n{}\n{text}", location(file, anchor), quoted.join("\n"))
        }
    }
}

/// `src/main.rs:L42-L45`, or `src/main.rs (file comment)`.
pub fn location(file: &FileDiff, anchor: Anchor) -> String {
    match anchor {
        Anchor::File => format!("{} (file comment)", file.path),
        Anchor::Lines { start, end } => {
            let lines: Vec<&Line> = lines(file, start, end).collect();
            format!("{}:{}", file.path, line_ref(&lines))
        }
    }
}

fn lines(file: &FileDiff, start: usize, end: usize) -> impl Iterator<Item = &Line> {
    file.lines().skip(start).take((end + 1).saturating_sub(start))
}

/// `L12`, `L12-L15`, or old-file numbers when only deleted lines are quoted.
fn line_ref(lines: &[&Line]) -> String {
    let new: Vec<u32> = lines.iter().filter_map(|line| line.new).collect();
    if let (Some(first), Some(last)) = (new.first(), new.last()) {
        return if first == last { format!("L{first}") } else { format!("L{first}-L{last}") };
    }
    let old: Vec<u32> = lines.iter().filter_map(|line| line.old).collect();
    match old.as_slice() {
        [] => String::new(),
        [only] => format!("old L{only}"),
        [first, .., last] => format!("old L{first}-L{last}"),
    }
}

/// `body: 2 links`: what a badge shows and adds to the notes.
pub fn badge(field: &str, kind: Kind, count: usize) -> String {
    let label = match (kind, count) {
        (Kind::NonAscii, _) => "non-ASCII",
        (Kind::Email, 1) => "email",
        (Kind::Email, _) => "emails",
        (Kind::Link, 1) => "link",
        (Kind::Link, _) => "links",
        (Kind::CoAuthoredBy, _) => "Co-authored-by",
    };
    format!("{field}: {count} {label}")
}

/// The text as shown, with the byte range of each mark. A character that
/// would be invisible (controls, exotic spaces) shows its code point; single
/// characters win over the spans they fall in, being the more precise mark.
pub fn marked(text: &str, findings: &[Finding]) -> (String, Vec<(Range<usize>, Kind)>) {
    let chars: Vec<char> = text.chars().collect();
    let mut kinds: Vec<Option<Kind>> = vec![None; chars.len()];
    for finding in findings.iter().filter(|f| f.kind != Kind::NonAscii) {
        for kind in kinds.iter_mut().take(finding.end).skip(finding.start) {
            *kind = Some(finding.kind);
        }
    }
    for finding in findings.iter().filter(|f| f.kind == Kind::NonAscii) {
        if let Some(kind) = kinds.get_mut(finding.start) {
            *kind = Some(Kind::NonAscii);
        }
    }
    let mut shown = String::new();
    let mut marks = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let kind = kinds[i];
        let start = shown.len();
        while i < chars.len() && kinds[i] == kind {
            if kind == Some(Kind::NonAscii) {
                shown.push_str(&visible(chars[i]));
            } else {
                shown.push(chars[i]);
            }
            i += 1;
        }
        if let Some(kind) = kind {
            marks.push((start..shown.len(), kind));
        }
    }
    (shown, marks)
}

fn visible(c: char) -> String {
    let code = c as u32;
    if code < 33 || code == 127 || c.is_whitespace() {
        format!("<U+{code:04X}>")
    } else {
        c.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::parse;
    use crate::message::findings;

    const DIFF: &str = "diff --git a/src/main.rs b/src/main.rs\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -41,3 +41,3 @@\n ctx\n-old line\n+new line\n";

    #[test]
    fn comments_quote_their_lines() {
        let file = &parse(DIFF)[0];
        assert_eq!(
            comment(file, Anchor::Lines { start: 1, end: 2 }, "the comment"),
            "src/main.rs:L42\n> -old line\n> +new line\nthe comment"
        );
        assert_eq!(comment(file, Anchor::File, "whole file"), "src/main.rs (file comment)\nwhole file");
        assert_eq!(location(file, Anchor::Lines { start: 0, end: 2 }), "src/main.rs:L41-L42");
        assert_eq!(location(file, Anchor::Lines { start: 1, end: 1 }), "src/main.rs:old L42");
    }

    #[test]
    fn decision_carries_notes_then_comments() {
        assert_eq!(decision(false, "  ", &[]), DEFAULT_DENY);
        assert_eq!(decision(true, "", &[]), "");
        assert_eq!(
            decision(false, "split it\n", &["a.rs:L1\n> +x\nwhy?".to_string()]),
            "Commit denied by human review. Reason:\nsplit it\n\na.rs:L1\n> +x\nwhy?"
        );
        assert_eq!(decision(true, "ok", &[]), "Commit accepted by human review, with notes:\nok");
    }

    #[test]
    fn badges_count_per_kind() {
        assert_eq!(badge("body", Kind::Link, 2), "body: 2 links");
        assert_eq!(badge("subject", Kind::Email, 1), "subject: 1 email");
        assert_eq!(badge("body", Kind::NonAscii, 3), "body: 3 non-ASCII");
    }

    #[test]
    fn marks_show_invisible_characters() {
        let text = "a\tb see https://x.y";
        let (shown, marks) = marked(text, &findings(text));
        assert_eq!(shown, "a<U+0009>b see https://x.y");
        assert_eq!(marks, vec![(1..9, Kind::NonAscii), (15..26, Kind::Link)]);
        let (shown, marks) = marked("caf\u{e9}", &findings("caf\u{e9}"));
        assert_eq!(shown, "caf\u{e9}");
        assert_eq!(marks, vec![(3..5, Kind::NonAscii)]);
    }
}
