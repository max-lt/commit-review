//! commit-review: human review window before a commit started by Claude Code.
//!
//! Usage: `commit-review [--command <intercepted shell command>]`
//!
//! Exit contract, read by hooks/review-before-commit.sh:
//!   - accept:  nothing on stdout, exit 0
//!   - deny:    the reason on stdout, exit 10
//!   - failure: message on stderr, any other code

mod message;

use std::io::Write;
use std::process::Command;

use tauri::Manager;

/// Exit code when the reviewer denies the commit.
const EXIT_DENIED: i32 = 10;

#[derive(serde::Serialize)]
struct Context {
    repo: String,
    status: String,
    /// The command Claude is about to run, when the hook passed it along.
    command: Option<String>,
    message: Option<message::CommitMessage>,
    /// Characters outside printable ASCII in the message, per field.
    ascii_issues: Option<AsciiIssues>,
}

#[derive(serde::Serialize)]
struct AsciiIssues {
    subject: Vec<message::NonAscii>,
    body: Vec<message::NonAscii>,
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

/// Value of `--command` on the command line, if present.
fn command_arg() -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--command" {
            return args.next();
        }
        if let Some(v) = a.strip_prefix("--command=") {
            return Some(v.to_string());
        }
    }
    None
}

/// Ends the process with the code the hook reads, after writing the
/// optional reason to stdout.
fn exit_with(code: i32, stdout_line: Option<&str>) -> ! {
    if let Some(line) = stdout_line {
        println!("{line}");
    }
    let _ = std::io::stdout().flush();
    std::process::exit(code)
}

/// What the window shows.
#[tauri::command]
fn context() -> Result<Context, String> {
    let command = command_arg();
    let message = command.as_deref().and_then(message::extract);
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
    })
}

/// The reviewer's decision.
#[tauri::command]
fn decide(accept: bool, reason: String) {
    if accept {
        exit_with(0, None)
    } else {
        exit_with(EXIT_DENIED, Some(reason.trim()))
    }
}

fn main() {
    let app = tauri::Builder::default()
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

    app.run(|_app, event| {
        // Window closed or Cmd+Q without a click: no decision, so deny.
        // A gate that accepts by accident is worthless.
        if let tauri::RunEvent::ExitRequested { code: None, .. } = event {
            exit_with(
                EXIT_DENIED,
                Some("Review window closed without a decision: commit denied."),
            )
        }
    });
}
