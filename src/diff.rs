//! The changes a commit will contain, as parsed unified diffs.

use crate::git;
use crate::message::Scope;

/// Hash of git's empty tree: what a root commit is measured against.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

#[derive(serde::Serialize, Debug, PartialEq, Eq)]
pub struct FileDiff {
    pub path: String,
    /// Previous path of a renamed file.
    pub old_path: Option<String>,
    pub status: Status,
    pub binary: bool,
    pub hunks: Vec<Hunk>,
}

#[derive(serde::Serialize, Debug, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Added,
    Deleted,
    Modified,
    Renamed,
}

#[derive(serde::Serialize, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub header: String,
    pub lines: Vec<Line>,
}

#[derive(serde::Serialize, Debug, PartialEq, Eq)]
pub struct Line {
    pub kind: Kind,
    /// Line number in the old file, absent for an added line.
    pub old: Option<u32>,
    /// Line number in the new file, absent for a deleted line.
    pub new: Option<u32>,
    pub text: String,
}

#[derive(serde::Serialize, Debug, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Context,
    Add,
    Del,
}

pub fn changes(scope: Scope, amend: bool) -> Result<Vec<FileDiff>, String> {
    let base = base(amend);
    let mut args = vec!["diff", "--no-color", "--no-ext-diff", "--find-renames", "-U3"];
    if scope == Scope::Staged {
        args.push("--cached");
    }
    args.push(&base);
    let mut files = parse(&git::diff(&args)?);
    if scope == Scope::Worktree {
        for path in git::run(&["ls-files", "--others", "--exclude-standard"])?.lines() {
            let text = git::diff(&["diff", "--no-color", "--no-index", "/dev/null", path])?;
            files.append(&mut parse(&text));
        }
    }
    Ok(files)
}

/// The commit the changes are measured against; the empty tree when there
/// is none, as for a root commit.
fn base(amend: bool) -> String {
    let rev = if amend { "HEAD~1" } else { "HEAD" };
    git::run(&["rev-parse", "--verify", "--quiet", rev]).unwrap_or_else(|_| EMPTY_TREE.to_string())
}

pub fn parse(diff: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    let (mut old_no, mut new_no) = (0, 0);
    for line in diff.lines() {
        if let Some(header) = line.strip_prefix("diff --git ") {
            files.push(FileDiff {
                path: path_from_header(header),
                old_path: None,
                status: Status::Modified,
                binary: false,
                hunks: Vec::new(),
            });
            continue;
        }
        let Some(file) = files.last_mut() else {
            continue;
        };
        if let Some(rest) = line.strip_prefix("@@ ") {
            (old_no, new_no) = hunk_starts(rest);
            file.hunks.push(Hunk { header: line.to_string(), lines: Vec::new() });
        } else if let Some(hunk) = file.hunks.last_mut() {
            let (kind, old, new) = match line.chars().next() {
                Some('+') => (Kind::Add, None, Some(new_no)),
                Some('-') => (Kind::Del, Some(old_no), None),
                Some(' ') => (Kind::Context, Some(old_no), Some(new_no)),
                // "\ No newline at end of file"
                _ => continue,
            };
            old_no += u32::from(old.is_some());
            new_no += u32::from(new.is_some());
            hunk.lines.push(Line { kind, old, new, text: line[1..].to_string() });
        } else if line == "--- /dev/null" {
            file.status = Status::Added;
        } else if line == "+++ /dev/null" {
            file.status = Status::Deleted;
        } else if let Some(path) = line.strip_prefix("+++ b/") {
            file.path = path.to_string();
        } else if let Some(path) = line.strip_prefix("rename from ") {
            file.status = Status::Renamed;
            file.old_path = Some(path.to_string());
        } else if line.starts_with("Binary files ") {
            file.binary = true;
        }
    }
    files
}

/// New path from `a/old b/new`; the `+++` line overrides it when present.
fn path_from_header(header: &str) -> String {
    header
        .split_once(" b/")
        .map(|(_, new)| new)
        .unwrap_or(header)
        .to_string()
}

/// Start line numbers of `-old,count +new,count @@ heading`.
fn hunk_starts(rest: &str) -> (u32, u32) {
    let mut parts = rest.split(' ');
    let mut start = |sign: char| -> u32 {
        parts
            .next()
            .and_then(|p| p.strip_prefix(sign))
            .and_then(|p| p.split(',').next()?.parse().ok())
            .unwrap_or(0)
    };
    let old = start('-');
    let new = start('+');
    (old, new)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(kind: Kind, old: Option<u32>, new: Option<u32>, text: &str) -> Line {
        Line { kind, old, new, text: text.to_string() }
    }

    #[test]
    fn numbers_lines_of_a_modified_file() {
        let diff = "diff --git a/src/a.rs b/src/a.rs\nindex 1..2 100644\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -3,4 +3,5 @@ fn main() {\n ctx\n-old\n+new\n+more\n ctx2\n\\ No newline at end of file\n";
        let files = parse(diff);
        assert_eq!(files.len(), 1);
        let file = &files[0];
        assert_eq!(file.path, "src/a.rs");
        assert_eq!(file.status, Status::Modified);
        assert_eq!(file.hunks[0].header, "@@ -3,4 +3,5 @@ fn main() {");
        assert_eq!(
            file.hunks[0].lines,
            vec![
                line(Kind::Context, Some(3), Some(3), "ctx"),
                line(Kind::Del, Some(4), None, "old"),
                line(Kind::Add, None, Some(4), "new"),
                line(Kind::Add, None, Some(5), "more"),
                line(Kind::Context, Some(5), Some(6), "ctx2"),
            ]
        );
    }

    #[test]
    fn added_renamed_and_binary_files() {
        let diff = "diff --git a/new.txt b/new.txt\nnew file mode 100644\n--- /dev/null\n+++ b/new.txt\n@@ -0,0 +1,2 @@\n+one\n+two\ndiff --git a/old.rs b/moved.rs\nsimilarity index 90%\nrename from old.rs\nrename to moved.rs\n--- a/old.rs\n+++ b/moved.rs\n@@ -1 +1 @@\n-a\n+b\ndiff --git a/img.png b/img.png\nBinary files a/img.png and b/img.png differ\n";
        let files = parse(diff);
        assert_eq!(files.len(), 3);
        assert_eq!((files[0].status, files[0].path.as_str()), (Status::Added, "new.txt"));
        assert_eq!(files[0].hunks[0].lines[1], line(Kind::Add, None, Some(2), "two"));
        assert_eq!(files[1].status, Status::Renamed);
        assert_eq!(files[1].old_path.as_deref(), Some("old.rs"));
        assert_eq!(files[1].path, "moved.rs");
        assert!(files[2].binary);
        assert_eq!(files[2].path, "img.png");
        assert!(files[2].hunks.is_empty());
    }

    #[test]
    fn hunk_lines_that_look_like_headers_stay_lines() {
        let diff = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n--- not a header\n+++ not one either\n";
        let lines = &parse(diff)[0].hunks[0].lines;
        assert_eq!(lines[0], line(Kind::Del, Some(1), None, "-- not a header"));
        assert_eq!(lines[1], line(Kind::Add, None, Some(1), "++ not one either"));
    }
}
