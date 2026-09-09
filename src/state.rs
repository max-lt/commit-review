//! Review state kept between attempts in `.git/commit-review/state.json`:
//! which files were marked viewed and the pending comments, so the review
//! of a denied commit carries over to the next try.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;

use crate::diff::FileDiff;
use crate::git;

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct State {
    pub files: HashMap<String, FileState>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct FileState {
    /// Digest of the diff as reviewed; Viewed holds only while it matches.
    pub digest: u64,
    pub viewed: bool,
    pub comments: Vec<SavedComment>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct SavedComment {
    /// The commented lines as the diff showed them, marker first; empty
    /// for a comment on the whole file.
    pub quote: Vec<String>,
    pub text: String,
}

/// A saved comment brought back into the current diff.
#[derive(serde::Serialize, Debug, PartialEq, Eq)]
#[serde(tag = "anchor", rename_all = "lowercase")]
pub enum Restored {
    /// The quoted lines are still there, at these flat indices.
    Lines { start: usize, end: usize, text: String },
    /// A whole-file comment on a file that did not change.
    File { text: String },
    /// The commented lines changed: the agent acted on the comment.
    Outdated { quote: Vec<String>, text: String },
}

/// One file's review as the window sends it back.
#[derive(serde::Deserialize)]
pub struct FileReview {
    pub path: String,
    pub viewed: bool,
    pub comments: Vec<CommentAt>,
}

/// A comment at flat line indices, or on the whole file when absent.
#[derive(serde::Deserialize)]
pub struct CommentAt {
    pub start: Option<usize>,
    pub end: Option<usize>,
    pub text: String,
}

impl State {
    pub fn load() -> Result<State, String> {
        match std::fs::read(path()?) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| format!("unreadable review state: {e}")),
            Err(_) => Ok(State::default()),
        }
    }

    /// The state to keep from a review of these files. Files with nothing
    /// to keep are left out.
    pub fn build(files: &[FileDiff], reviews: Vec<FileReview>) -> State {
        let mut state = State::default();
        for review in reviews {
            if !review.viewed && review.comments.is_empty() {
                continue;
            }
            let Some(file) = files.iter().find(|f| f.path == review.path) else {
                continue;
            };
            let lines: Vec<String> = file.lines().map(|l| l.quoted()).collect();
            let comments = review
                .comments
                .into_iter()
                .map(|c| SavedComment {
                    quote: match (c.start, c.end) {
                        (Some(s), Some(e)) => lines.get(s..=e).map(<[String]>::to_vec).unwrap_or_default(),
                        _ => Vec::new(),
                    },
                    text: c.text,
                })
                .collect();
            state.files.insert(
                review.path,
                FileState { digest: digest(file), viewed: review.viewed, comments },
            );
        }
        state
    }

    pub fn save(&self) -> Result<(), String> {
        let path = path()?;
        let dir = path.parent().expect("state path has a directory");
        std::fs::create_dir_all(dir)
            .and_then(|_| std::fs::write(&path, serde_json::to_vec_pretty(self).unwrap()))
            .map_err(|e| format!("cannot save the review state: {e}"))
    }

    pub fn clear() -> Result<(), String> {
        match std::fs::remove_file(path()?) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(format!("cannot clear the review state: {e}"))
            }
            _ => Ok(()),
        }
    }

    /// Whether the file keeps its Viewed mark, and its comments placed in
    /// the diff as it is now.
    pub fn restore(&self, file: &FileDiff) -> (bool, Vec<Restored>) {
        let Some(saved) = self.files.get(&file.path) else {
            return (false, Vec::new());
        };
        let same = saved.digest == digest(file);
        let lines: Vec<String> = file.lines().map(|l| l.quoted()).collect();
        let restored = saved
            .comments
            .iter()
            .map(|c| {
                let text = c.text.clone();
                if c.quote.is_empty() {
                    return if same {
                        Restored::File { text }
                    } else {
                        Restored::Outdated { quote: Vec::new(), text }
                    };
                }
                match lines.windows(c.quote.len()).position(|w| w == c.quote.as_slice()) {
                    Some(start) => Restored::Lines { start, end: start + c.quote.len() - 1, text },
                    None => Restored::Outdated { quote: c.quote.clone(), text },
                }
            })
            .collect();
        (same && saved.viewed, restored)
    }
}

/// Where the state lives: with the repository's own metadata, never
/// versioned, gone with the repository.
fn path() -> Result<PathBuf, String> {
    let git_dir = git::run(&["rev-parse", "--git-dir"])?;
    Ok(PathBuf::from(git_dir).join("commit-review").join("state.json"))
}

/// Identity of a file's diff as reviewed: its lines, not their numbers.
/// Not stable across compiler versions; a stale digest only resets Viewed.
pub fn digest(file: &FileDiff) -> u64 {
    let mut hasher = DefaultHasher::new();
    file.status.hash(&mut hasher);
    file.binary.hash(&mut hasher);
    for line in file.lines() {
        line.kind.hash(&mut hasher);
        line.text.hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::parse;

    const BEFORE: &str = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,3 +1,3 @@\n a\n-b\n+c\n d\n";
    const AFTER: &str = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,4 +1,4 @@\n z\n a\n-b\n+c\n";

    fn state(quote: &[&str], viewed: bool, file: &FileDiff) -> State {
        let mut state = State::default();
        state.files.insert(
            "f".into(),
            FileState {
                digest: digest(file),
                viewed,
                comments: vec![SavedComment {
                    quote: quote.iter().map(|s| s.to_string()).collect(),
                    text: "why?".into(),
                }],
            },
        );
        state
    }

    #[test]
    fn unchanged_file_keeps_viewed_and_comments() {
        let file = &parse(BEFORE)[0];
        let (viewed, restored) = state(&["-b", "+c"], true, file).restore(file);
        assert!(viewed);
        assert_eq!(restored, vec![Restored::Lines { start: 1, end: 2, text: "why?".into() }]);
    }

    #[test]
    fn changed_file_loses_viewed_but_finds_its_lines() {
        let before = &parse(BEFORE)[0];
        let after = &parse(AFTER)[0];
        let (viewed, restored) = state(&["-b", "+c"], true, before).restore(after);
        assert!(!viewed);
        assert_eq!(restored, vec![Restored::Lines { start: 2, end: 3, text: "why?".into() }]);
    }

    #[test]
    fn comment_on_changed_lines_is_outdated() {
        let before = &parse(BEFORE)[0];
        let after = &parse(AFTER)[0];
        let (_, restored) = state(&[" d"], false, before).restore(after);
        assert_eq!(
            restored,
            vec![Restored::Outdated { quote: vec![" d".into()], text: "why?".into() }]
        );
        let (_, restored) = state(&[], false, before).restore(after);
        assert_eq!(restored, vec![Restored::Outdated { quote: Vec::new(), text: "why?".into() }]);
    }

    #[test]
    fn build_quotes_the_commented_lines() {
        let files = parse(BEFORE);
        let reviews = vec![FileReview {
            path: "f".into(),
            viewed: false,
            comments: vec![
                CommentAt { start: Some(1), end: Some(2), text: "a".into() },
                CommentAt { start: None, end: None, text: "b".into() },
            ],
        }];
        let state = State::build(&files, reviews);
        let saved = &state.files["f"];
        assert_eq!(saved.comments[0].quote, vec!["-b", "+c"]);
        assert!(saved.comments[1].quote.is_empty());
        assert_eq!(saved.digest, digest(&files[0]));
    }
}
