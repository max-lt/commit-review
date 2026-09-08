//! commit-review: human review window before a commit started by Claude Code.
//!
//! Usage:
//!   commit-review hook                Claude Code PreToolUse hook: reads the
//!                                     event JSON on stdin, answers on stdout
//!   commit-review [--command <cmd>]   manual launch: exit 0 = accept,
//!                                     exit 10 = deny with the reason on stdout

mod message;

use std::io::{Read, Write};
use std::process::Command;

use tauri::Manager;

/// Exit code of a manual launch when the reviewer denies the commit.
const EXIT_DENIED: i32 = 10;

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
}

#[derive(serde::Serialize)]
struct Context {
    repo: String,
    status: String,
    /// The command Claude is about to run, when known.
    command: Option<String>,
    message: Option<message::CommitMessage>,
    /// Characters outside printable ASCII in the message, per field.
    ascii_issues: Option<AsciiIssues>,
    /// The commit being rewritten by `--amend`, if any.
    amend: Option<Amend>,
}

#[derive(serde::Serialize)]
struct AsciiIssues {
    subject: Vec<message::NonAscii>,
    body: Vec<message::NonAscii>,
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
    let (line, code) = match output {
        Output::Hook => {
            let json = serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": reason,
                }
            });
            (json.to_string(), 0)
        }
        Output::Plain => (reason.to_string(), EXIT_DENIED),
    };
    println!("{line}");
    let _ = std::io::stdout().flush();
    std::process::exit(code)
}

fn git(args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .output()
        .map_err(|e| format!("git not found: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
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
            message = message::from_raw(&git(&["log", "-1", "--format=%B", &rev])?);
        }
        amend = Some(Amend {
            head: git(&["log", "-1", "--format=%h %s"])?,
            stat: git(&["show", "--stat", "--format=", "HEAD"])?,
            message_kept,
        });
    }
    let ascii_issues = message.as_ref().map(|m| AsciiIssues {
        subject: message::non_printable_ascii(&m.subject),
        body: message::non_printable_ascii(&m.body),
    });
    Ok(Context {
        repo: git(&["rev-parse", "--show-toplevel"])?,
        status: git(&["status", "--short"])?,
        command,
        message,
        ascii_issues,
        amend,
    })
}

/// The reviewer's decision.
#[tauri::command]
fn decide(review: tauri::State<Review>, accept: bool, reason: String) {
    if accept {
        std::process::exit(0)
    }
    deny(review.output, reason.trim())
}

fn review(command: Option<String>, output: Output) -> ! {
    let app = tauri::Builder::default()
        .manage(Review { command, output })
        .invoke_handler(tauri::generate_handler![context, decide])
        .setup(|app| {
            // Started by a hook, with no terminal: the window has to take
            // focus itself, or it opens behind the terminal.
            if let Some(window) = app.get_webview_window("main") {
                window.show()?;
                window.set_focus()?;
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
