//! Reads the intercepted shell command: tells whether it runs `git commit`
//! and extracts the commit message.
//!
//! Best effort, nothing is executed: `-m "..."`, `-m '...'`,
//! `--message=...`, `-am ...`, several `-m` (separate paragraphs, like
//! git), and the `$(cat <<'EOF' ... EOF)` heredoc Claude Code uses.
//! When no message is recognized, the UI shows the raw command instead.

use std::iter::Peekable;
use std::str::Chars;

/// Message split the way git does: subject = first paragraph,
/// body = everything after the first blank line.
#[derive(serde::Serialize, Debug, PartialEq, Eq)]
pub struct CommitMessage {
    pub subject: String,
    pub body: String,
}

/// A character outside printable ASCII (32-126), with its char index.
#[derive(serde::Serialize, Debug, PartialEq, Eq)]
pub struct NonAscii {
    pub index: usize,
    pub code: u32,
}

/// Characters outside printable ASCII (32-126). Newlines separate lines,
/// they are not content, so they are not reported.
pub fn non_printable_ascii(text: &str) -> Vec<NonAscii> {
    text.chars()
        .enumerate()
        .filter(|(_, c)| *c != '\n' && !(' '..='~').contains(c))
        .map(|(index, c)| NonAscii { index, code: c as u32 })
        .collect()
}

/// True when the command runs `git commit` itself, as opposed to merely
/// mentioning it inside a quoted string or a heredoc body.
pub fn is_git_commit(cmd: &str) -> bool {
    commit_index(&shell_words(&strip_heredocs(cmd))).is_some()
}

/// True when the command rewrites HEAD with `--amend`.
pub fn amends(cmd: &str) -> bool {
    commit_options(cmd).iter().any(|w| w == "--amend")
}

/// Revision whose message `-C`, `-c`, `--reuse-message=` or
/// `--reedit-message=` takes over, if any.
pub fn reused_message_rev(cmd: &str) -> Option<String> {
    let options = commit_options(cmd);
    let mut words = options.iter();
    while let Some(w) = words.next() {
        if w == "-C" || w == "-c" {
            return words.next().cloned();
        }
        let long = w
            .strip_prefix("--reuse-message=")
            .or_else(|| w.strip_prefix("--reedit-message="));
        if let Some(rev) = long {
            return Some(rev.to_string());
        }
    }
    None
}

/// Words after the `commit` subcommand, heredoc bodies stripped.
fn commit_options(cmd: &str) -> Vec<String> {
    let words = shell_words(&strip_heredocs(cmd));
    match commit_index(&words) {
        Some(i) => words[i + 1..].to_vec(),
        None => Vec::new(),
    }
}

/// Index of the `commit` word when the words run `git commit`.
fn commit_index(words: &[String]) -> Option<usize> {
    for (i, w) in words.iter().enumerate() {
        if w != "git" {
            continue;
        }
        let mut j = i + 1;
        while words.get(j).is_some_and(|w| w.starts_with('-')) {
            // `-C <path>` and `-c <key=value>` take a value.
            j += if words[j] == "-C" || words[j] == "-c" { 2 } else { 1 };
        }
        if words.get(j).is_some_and(|w| w == "commit") {
            return Some(j);
        }
    }
    None
}

/// The message given on the command line, if any.
pub fn extract(cmd: &str) -> Option<CommitMessage> {
    let mut paragraphs: Vec<String> = message_flags(cmd)
        .into_iter()
        .map(|v| heredoc_body(&v).unwrap_or(v))
        .collect();
    if paragraphs.is_empty() {
        // `git commit -F - <<EOF` or another form without -m: try a bare heredoc.
        paragraphs.extend(heredoc_body(cmd));
    }
    from_raw(&paragraphs.join("\n\n"))
}

/// A message as git stores it (`%B`), split into subject and body.
pub fn from_raw(raw: &str) -> Option<CommitMessage> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let (subject, body) = match raw.split_once("\n\n") {
        Some((s, b)) => (s, b.trim()),
        None => (raw, ""),
    };
    Some(CommitMessage {
        subject: subject.lines().map(str::trim).collect::<Vec<_>>().join(" "),
        body: body.to_string(),
    })
}

/// The command with heredoc bodies removed, so their text is not read as
/// commands. The closing delimiter line stays, so the heredoc still ends.
fn strip_heredocs(cmd: &str) -> String {
    let mut out = String::new();
    let mut lines = cmd.lines();
    while let Some(line) = lines.next() {
        out.push_str(line);
        out.push('\n');
        let Some(delim) = heredoc_delimiter(line) else {
            continue;
        };
        for body_line in lines.by_ref() {
            if body_line.trim() == delim {
                out.push_str(body_line);
                out.push('\n');
                break;
            }
        }
    }
    out
}

/// Delimiter word of a `<<EOF`, `<<'EOF'`, `<<"EOF"` or `<<-EOF` on the line.
fn heredoc_delimiter(line: &str) -> Option<String> {
    let rest = &line[line.find("<<")? + 2..];
    let rest = rest.strip_prefix('-').unwrap_or(rest).trim_start_matches(' ');
    let rest = rest.strip_prefix(['\'', '"']).unwrap_or(rest);
    let delim: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!delim.is_empty()).then_some(delim)
}

/// Content of the first heredoc in the text.
fn heredoc_body(text: &str) -> Option<String> {
    let mut lines = text.lines();
    let delim = loop {
        if let Some(delim) = heredoc_delimiter(lines.next()?) {
            break delim;
        }
    };
    let mut body = Vec::new();
    for line in lines {
        if line.trim() == delim {
            return Some(body.join("\n"));
        }
        body.push(line);
    }
    None
}

/// Values of the -m / --message options, in order.
fn message_flags(cmd: &str) -> Vec<String> {
    let words = shell_words(cmd);
    let mut out = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let w = &words[i];
        if w == "--message" {
            if let Some(v) = words.get(i + 1) {
                out.push(v.clone());
                i += 1;
            }
        } else if let Some(v) = w.strip_prefix("--message=") {
            out.push(v.to_string());
        } else if w.starts_with('-') && !w.starts_with("--") {
            // Short option cluster: `-am`, `-sm`, `-mtext`. Like git, whatever
            // follows the `m` inside the word is the value, else the next word.
            if let Some(pos) = w[1..].find('m') {
                let inline = &w[pos + 2..];
                if !inline.is_empty() {
                    out.push(inline.to_string());
                } else if let Some(v) = words.get(i + 1) {
                    out.push(v.clone());
                    i += 1;
                }
            }
        }
        i += 1;
    }
    out
}

/// Splits into words, honoring single quotes, double quotes, backslash and
/// `$(...)` substitutions. Enough to read options, not a real shell parser.
fn shell_words(cmd: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut chars = cmd.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                for c in chars.by_ref() {
                    if c == '\'' {
                        break;
                    }
                    cur.push(c);
                }
            }
            '"' => {
                in_word = true;
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some(n @ ('"' | '\\' | '$' | '`')) => cur.push(n),
                            Some('\n') | None => {}
                            Some(n) => {
                                cur.push('\\');
                                cur.push(n);
                            }
                        },
                        '$' if chars.peek() == Some(&'(') => {
                            cur.push('$');
                            substitution(&mut chars, &mut cur);
                        }
                        _ => cur.push(c),
                    }
                }
            }
            '\\' => {
                in_word = true;
                match chars.next() {
                    Some('\n') | None => {}
                    Some(n) => cur.push(n),
                }
            }
            '$' if chars.peek() == Some(&'(') => {
                in_word = true;
                cur.push('$');
                substitution(&mut chars, &mut cur);
            }
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            _ => {
                in_word = true;
                cur.push(c);
            }
        }
    }
    if in_word {
        words.push(cur);
    }
    words
}

/// Appends a `(...)` substitution body up to its closing paren. Quoted spans
/// and heredocs are copied as they are, so their quotes and parens do not
/// count.
fn substitution(chars: &mut Peekable<Chars>, cur: &mut String) {
    let mut depth = 0;
    while let Some(c) = chars.next() {
        cur.push(c);
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return;
                }
            }
            '\'' | '"' => {
                for n in chars.by_ref() {
                    cur.push(n);
                    if n == c {
                        break;
                    }
                }
            }
            '<' if chars.peek() == Some(&'<') => {
                cur.push(chars.next().unwrap());
                heredoc(chars, cur);
            }
            _ => {}
        }
    }
}

/// Appends a heredoc, from its delimiter word to the line that closes it.
fn heredoc(chars: &mut Peekable<Chars>, cur: &mut String) {
    let first = line(chars, cur);
    let Some(delim) = heredoc_delimiter(&format!("<<{first}")) else {
        return;
    };
    while chars.peek().is_some() {
        if line(chars, cur).trim() == delim {
            return;
        }
    }
}

/// Appends one line, newline included, and returns it without the newline.
fn line(chars: &mut Peekable<Chars>, cur: &mut String) -> String {
    let mut text = String::new();
    for c in chars.by_ref() {
        cur.push(c);
        if c == '\n' {
            break;
        }
        text.push(c);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(subject: &str, body: &str) -> Option<CommitMessage> {
        Some(CommitMessage { subject: subject.into(), body: body.into() })
    }

    #[test]
    fn claude_code_heredoc() {
        let cmd = "git add -A && git commit -m \"$(cat <<'EOF'\nAdd review window\n\nShow subject and body.\n\nTrailer: value\nEOF\n)\"";
        assert_eq!(
            extract(cmd),
            msg("Add review window", "Show subject and body.\n\nTrailer: value")
        );
    }

    #[test]
    fn heredoc_body_may_contain_quotes_and_parens() {
        let cmd = "git commit -m \"$(cat <<'EOF'\nui: subject\n\nShowed \"no changes\" (none) for --amend.\nEOF\n)\" --amend && git log -1";
        assert_eq!(extract(cmd), msg("ui: subject", "Showed \"no changes\" (none) for --amend."));
        assert!(amends(cmd));
        assert!(is_git_commit(cmd));
    }

    #[test]
    fn simple_dash_m() {
        assert_eq!(extract("git commit -m 'fix typo'"), msg("fix typo", ""));
        assert_eq!(extract("git commit -m \"fix \\\"quoted\\\" typo\""), msg("fix \"quoted\" typo", ""));
    }

    #[test]
    fn two_dash_m_become_paragraphs() {
        assert_eq!(extract("git commit -m subject -m 'body text'"), msg("subject", "body text"));
    }

    #[test]
    fn combined_short_flags_and_long_form() {
        assert_eq!(extract("git commit -am 'all in'"), msg("all in", ""));
        assert_eq!(extract("git commit --message='long form'"), msg("long form", ""));
        assert_eq!(extract("git commit -mglued"), msg("glued", ""));
    }

    #[test]
    fn no_message_is_none() {
        assert_eq!(extract("git commit --amend --no-edit"), None);
        assert_eq!(extract("git commit -m ''"), None);
    }

    #[test]
    fn heredoc_without_dash_m() {
        assert_eq!(extract("git commit -F - <<EOF\nsubject only\nEOF"), msg("subject only", ""));
    }

    #[test]
    fn printable_ascii_passes() {
        assert!(non_printable_ascii("Add thing (v2): ok!\n\nbody ~ 100%").is_empty());
    }

    #[test]
    fn non_ascii_and_control_chars_are_reported() {
        assert_eq!(
            non_printable_ascii("c\u{2019}est"),
            vec![NonAscii { index: 1, code: 0x2019 }]
        );
        assert_eq!(
            non_printable_ascii("a\tb\u{e9}"),
            vec![NonAscii { index: 1, code: 9 }, NonAscii { index: 3, code: 0xe9 }]
        );
    }

    #[test]
    fn multiline_subject_is_folded() {
        assert_eq!(extract("git commit -m 'line one\nline two\n\nbody'"), msg("line one line two", "body"));
    }

    #[test]
    fn detects_a_real_git_commit() {
        assert!(is_git_commit("git commit -m x"));
        assert!(is_git_commit("git add -A && git commit -m \"$(cat <<'EOF'\nsubject\nEOF\n)\""));
        assert!(is_git_commit("git -C /tmp/repo commit -am x"));
        assert!(is_git_commit("git -c user.name=me commit"));
    }

    #[test]
    fn amend_and_reused_message() {
        assert!(amends("git commit --amend --no-edit"));
        assert!(amends("git add -A && git commit --amend -m 'new subject'"));
        assert!(!amends("git commit -m 'says --amend in the message'"));
        assert!(!amends("git rebase --amend"));
        assert_eq!(reused_message_rev("git commit --amend -C HEAD~1"), Some("HEAD~1".into()));
        assert_eq!(reused_message_rev("git commit --reuse-message=abc123"), Some("abc123".into()));
        assert_eq!(reused_message_rev("git -c user.name=me commit --amend"), None);
    }

    #[test]
    fn ignores_git_commit_mentioned_in_text() {
        assert!(!is_git_commit("cat > README.md <<'EOF'\nrun git commit -m x\nEOF"));
        assert!(!is_git_commit("echo \"git commit\""));
        assert!(!is_git_commit("git status"));
        assert!(!is_git_commit("cargo build"));
    }
}
