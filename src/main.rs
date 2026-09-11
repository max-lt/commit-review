//! commit-review: human review window before a commit started by Claude Code.
//!
//! Usage:
//!   commit-review hook                Claude Code PreToolUse hook: reads the
//!                                     event JSON on stdin, answers on stdout
//!   commit-review [--command <cmd>]   manual launch: exit 0 = accept,
//!                                     exit 10 = deny; the notes on stdout

mod diff;
mod git;
mod message;
mod state;

use std::io::{Read, Write};
use std::sync::Mutex;

use tauri::{Emitter, Manager};

/// Exit code of a manual launch when the reviewer denies the commit.
const EXIT_DENIED: i32 = 10;

/// How long the window waits for a decision under the hook. Claude Code
/// kills a hook at its timeout (3600 s as registered) and lets the command
/// through, so the gate has to give up first, by denying.
const DEADLINE: std::time::Duration = std::time::Duration::from_secs(55 * 60);
/// Time left to the window to deny with its notes and comments saved
/// before the process denies on its own.
const GRACE: std::time::Duration = std::time::Duration::from_secs(15);

/// How a decision leaves the process.
#[derive(Clone, Copy)]
enum Output {
    /// Claude Code hook protocol: a JSON deny on stdout, exit 0 either way.
    Hook,
    /// Manual launch: the reason on stdout, exit 10 on deny.
    Plain,
}

/// What the window is reviewing, shared with the webview commands.
struct Review {
    command: Option<String>,
    output: Output,
    /// The diff as sent to the window, to quote commented lines on save.
    files: Mutex<Vec<diff::FileDiff>>,
}

/// A file of the diff with what an earlier attempt left on it.
#[derive(serde::Serialize)]
struct Change {
    #[serde(flatten)]
    diff: diff::FileDiff,
    /// Marked viewed earlier and unchanged since.
    viewed: bool,
    restored: Vec<state::Restored>,
}

#[derive(serde::Serialize)]
struct Context {
    repo: String,
    status: String,
    /// The reviewer, from git config, for the comment boxes.
    user: String,
    /// The command the agent is about to run, when known.
    command: Option<String>,
    message: Option<message::CommitMessage>,
    /// What the message contains that deserves a look, per field.
    findings: Option<Findings>,
    /// The commit being rewritten by `--amend`, if any.
    amend: Option<Amend>,
    scope: message::Scope,
}

#[derive(serde::Serialize)]
struct Findings {
    subject: Vec<message::Finding>,
    body: Vec<message::Finding>,
}

#[derive(serde::Serialize)]
struct Amend {
    /// Short hash and subject of HEAD.
    head: String,
    /// Files HEAD already touches, from `git show --stat`.
    stat: String,
    /// The message is taken over from a commit, not given on the command line.
    message_kept: bool,
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("hook") => hook(),
        _ => review(flag_value("command"), Output::Plain),
    }
}

fn hook() -> ! {
    // A panic must still deny: Claude Code treats an unexpected exit code as
    // a non-blocking hook error and lets the commit through.
    std::panic::set_hook(Box::new(|info| {
        deny(Output::Hook, &format!("commit-review crashed: {info}"))
    }));
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .expect("hook event on stdin");
    let event: serde_json::Value = serde_json::from_str(&input).expect("hook event is JSON");
    let command = event["tool_input"]["command"].as_str().unwrap_or("");
    if !message::is_git_commit(command) {
        std::process::exit(0);
    }
    if let Some(cwd) = event["cwd"].as_str() {
        std::env::set_current_dir(cwd).expect("hook cwd exists");
    }
    review(Some(command.to_string()), Output::Hook)
}

fn deny(output: Output, reason: &str) -> ! {
    finish(output, false, reason)
}

/// Leaves the process with the decision. The text reaches the agent either
/// way: as the deny reason, or as extra context on an accepted commit.
fn finish(output: Output, accept: bool, text: &str) -> ! {
    let line = match output {
        Output::Plain => text.to_string(),
        Output::Hook if accept && text.is_empty() => String::new(),
        Output::Hook => {
            let mut fields = serde_json::json!({ "hookEventName": "PreToolUse" });
            if accept {
                fields["permissionDecision"] = "allow".into();
                fields["additionalContext"] = text.into();
            } else {
                fields["permissionDecision"] = "deny".into();
                fields["permissionDecisionReason"] = text.into();
            }
            serde_json::json!({ "hookSpecificOutput": fields }).to_string()
        }
    };
    if !line.is_empty() {
        println!("{line}");
    }
    let _ = std::io::stdout().flush();
    let code = match (output, accept) {
        (Output::Plain, false) => EXIT_DENIED,
        _ => 0,
    };
    std::process::exit(code)
}

/// Value of `--<name> <value>` or `--<name>=<value>` on the command line.
fn flag_value(name: &str) -> Option<String> {
    let flag = format!("--{name}");
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == flag {
            return args.next();
        }
        if let Some(v) = a.strip_prefix(&flag).and_then(|v| v.strip_prefix('=')) {
            return Some(v.to_string());
        }
    }
    None
}

/// A manual launch has no command: show the whole working tree.
fn scope_of(command: Option<&str>) -> message::Scope {
    command.map_or(message::Scope::Worktree, message::scope)
}

/// What the window shows.
#[tauri::command]
fn context(review: tauri::State<Review>) -> Result<Context, String> {
    let command = review.command.clone();
    let mut message = command.as_deref().and_then(message::extract);
    let mut amend = None;
    if command.as_deref().is_some_and(message::amends) {
        let message_kept = message.is_none();
        if message_kept {
            // Without -m, git keeps the message of HEAD or of the -C revision.
            let rev = command
                .as_deref()
                .and_then(message::reused_message_rev)
                .unwrap_or_else(|| "HEAD".to_string());
            message = message::from_raw(&git::run(&["log", "-1", "--format=%B", &rev])?);
        }
        amend = Some(Amend {
            head: git::run(&["log", "-1", "--format=%h %s"])?,
            stat: git::run(&["show", "--stat", "--format=", "HEAD"])?,
            message_kept,
        });
    }
    let findings = message.as_ref().map(|m| Findings {
        subject: message::findings(&m.subject),
        body: message::findings(&m.body),
    });
    Ok(Context {
        repo: git::run(&["rev-parse", "--show-toplevel"])?,
        status: git::run(&["status", "--short"])?,
        user: git::run(&["config", "user.name"]).unwrap_or_else(|_| "You".to_string()),
        scope: scope_of(command.as_deref()),
        command,
        message,
        findings,
        amend,
    })
}

/// The changes the commit will contain, with the earlier review of each.
#[tauri::command]
fn changes(review: tauri::State<Review>) -> Result<Vec<Change>, String> {
    let command = review.command.as_deref();
    let files = diff::changes(scope_of(command), command.is_some_and(message::amends))?;
    let saved = state::State::load()?;
    let changes = files
        .iter()
        .map(|f| {
            let (viewed, restored) = saved.restore(f);
            Change { diff: f.clone(), viewed, restored }
        })
        .collect();
    *review.files.lock().unwrap() = files;
    Ok(changes)
}

/// The reviewer's decision, with the notes and comments left in the window.
/// `reviews` is absent when the diff was never opened: the saved state
/// then stands as it is.
#[tauri::command]
fn decide(
    review: tauri::State<Review>,
    accept: bool,
    notes: String,
    reviews: Option<Vec<state::FileReview>>,
) {
    let kept = match (accept, reviews) {
        (true, _) => state::State::clear(),
        (false, Some(reviews)) => state::State::build(&review.files.lock().unwrap(), reviews).save(),
        (false, None) => Ok(()),
    };
    // Losing the review state is not worth losing the decision.
    if let Err(e) = kept {
        eprintln!("commit-review: {e}");
    }
    finish(review.output, accept, notes.trim())
}

fn review(command: Option<String>, output: Output) -> ! {
    // Git paths are shown relative to the root; the cwd may be deeper.
    if let Ok(root) = git::run(&["rev-parse", "--show-toplevel"]) {
        std::env::set_current_dir(root).expect("repository root exists");
    }
    let app = tauri::Builder::default()
        .manage(Review { command, output, files: Mutex::new(Vec::new()) })
        .invoke_handler(tauri::generate_handler![context, changes, decide])
        .setup(move |app| {
            // Started by a hook, with no terminal: the window has to take
            // focus itself, or it opens behind the terminal.
            if let Some(window) = app.get_webview_window("main") {
                window.show()?;
                window.set_focus()?;
            }
            if matches!(output, Output::Hook) {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(DEADLINE);
                    // The window denies through the usual path, saving the review.
                    let _ = handle.emit("deadline", ());
                    std::thread::sleep(GRACE);
                    deny(output, "No reviewer answered within an hour: commit denied. Do not retry until the reviewer is back.")
                });
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("commit-review: could not start the window");

    app.run(move |_app, event| {
        // Window closed or Cmd+Q without a click: no decision, so deny.
        // A gate that accepts by accident is worthless.
        if let tauri::RunEvent::ExitRequested { code: None, .. } = event {
            deny(output, "Review window closed without a decision: commit denied.")
        }
    });
    // `run` only returns on platforms where the event loop can end.
    deny(output, "Review window ended without a decision: commit denied.")
}
