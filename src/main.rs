//! commit-review: human review window before a commit started by a coding agent.
//!
//! `commit-review --help` lists the commands; without one, a manual launch
//! opens the window on the working tree (exit 0 = accept, 10 = deny, the
//! notes on stdout).

mod diff;
mod git;
mod message;
mod prefs;
mod remote;
mod state;

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use clap::{Parser, Subcommand};
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
    /// The same review on the worker, when published.
    remote: Mutex<Option<remote::Published>>,
}

/// The review as published: version 1 of the format.
#[derive(serde::Serialize)]
struct Document {
    version: u32,
    context: Context,
    changes: Vec<Change>,
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

/// Human review window before a commit started by a coding agent.
#[derive(Parser)]
#[command(name = "commit-review", version, about, long_about = None)]
struct Cli {
    /// The shell command about to run, as an agent would; without it the
    /// window shows the whole working tree. Exit 0 = accept, 10 = deny.
    #[arg(long, value_name = "SHELL COMMAND")]
    command: Option<String>,
    #[command(subcommand)]
    action: Option<Action>,
}

#[derive(Subcommand)]
enum Action {
    /// PreToolUse hook: reads the event JSON on stdin, answers on stdout
    Hook,
    /// Turn the gate on
    Enable,
    /// Park the gate: the hook lets commits through until `enable`
    Disable,
    /// The gate and the login
    Status,
    /// The phone side: a GitHub login through the worker
    Auth {
        #[command(subcommand)]
        action: Auth,
    },
}

#[derive(Subcommand)]
enum Auth {
    /// Log this machine in with GitHub's device flow
    Login {
        /// Origin of the worker, e.g. https://commit-review.example.dev;
        /// the saved one when omitted
        #[arg(long)]
        url: Option<String>,
    },
    /// Who is logged in, and where
    Status,
    /// Forget the login
    Logout,
}

fn main() {
    let cli = Cli::parse();
    match cli.action {
        Some(Action::Hook) => hook(),
        Some(Action::Enable) => toggle(true),
        Some(Action::Disable) => toggle(false),
        Some(Action::Status) => status(),
        Some(Action::Auth { action }) => auth(action),
        None => review(cli.command, Output::Plain),
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
    if !message::is_git_commit(command) || !prefs::enabled() {
        std::process::exit(0);
    }
    if let Some(cwd) = event["cwd"].as_str() {
        std::env::set_current_dir(cwd).expect("hook cwd exists");
    }
    review(Some(command.to_string()), Output::Hook)
}

/// `commit-review enable` / `disable`.
fn toggle(enabled: bool) -> ! {
    match prefs::set(enabled) {
        Ok(()) => {
            println!("Review {}", if enabled { "enabled" } else { "disabled: commits go through until `commit-review enable`" });
            std::process::exit(0)
        }
        Err(e) => {
            eprintln!("commit-review: {e}");
            std::process::exit(1)
        }
    }
}

/// `commit-review status`: the gate and the login.
fn status() -> ! {
    println!("Review {}", if prefs::enabled() { "enabled" } else { "disabled" });
    match remote::load() {
        Some(c) => println!("Logged in as {} on {}", c.login, c.url),
        None => println!("Not logged in: reviews stay on this machine"),
    }
    std::process::exit(0)
}

fn auth(action: Auth) -> ! {
    let result = match action {
        Auth::Login { url } => match url.or_else(|| remote::load().map(|c| c.url)) {
            Some(url) => remote::login(&url),
            None => Err("no worker known yet: pass --url https://<worker>".to_string()),
        },
        Auth::Status => {
            match remote::load() {
                Some(c) => println!("Logged in as {} on {}", c.login, c.url),
                None => println!("Not logged in: reviews stay on this machine"),
            }
            Ok(())
        }
        Auth::Logout => remote::logout(),
    };
    match result {
        Ok(()) => std::process::exit(0),
        Err(e) => {
            eprintln!("commit-review: {e}");
            std::process::exit(1)
        }
    }
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

/// A manual launch has no command: show the whole working tree.
fn scope_of(command: Option<&str>) -> message::Scope {
    command.map_or(message::Scope::Worktree, message::scope)
}

/// What the window shows.
fn build_context(review: &Review) -> Result<Context, String> {
    let command = review.command.clone();
    let mut message = command.as_deref().and_then(message::extract);
    if message.is_none() {
        // `-F <file>`: the message is on disk, relative to where git runs.
        message = command
            .as_deref()
            .and_then(message::message_file)
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|raw| message::from_raw(&raw));
    }
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
fn build_changes(review: &Review) -> Result<Vec<Change>, String> {
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

/// The reviewer's decision, from the window or the phone, with the notes
/// and comments left with it. `reviews` is absent when the diff was never
/// opened: the saved state then stands as it is.
fn conclude(review: &Review, accept: bool, notes: &str, reviews: Option<Vec<state::FileReview>>) -> ! {
    // Whoever did not decide has nothing left to decide.
    if let (Some(config), Some(published)) = (remote::load(), review.remote.lock().unwrap().as_ref()) {
        remote::withdraw(&config, &published.id);
    }
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

#[tauri::command]
fn context(review: tauri::State<Arc<Review>>) -> Result<Context, String> {
    build_context(&review)
}

#[tauri::command]
fn changes(review: tauri::State<Arc<Review>>) -> Result<Vec<Change>, String> {
    build_changes(&review)
}

#[tauri::command]
fn decide(review: tauri::State<Arc<Review>>, accept: bool, notes: String, reviews: Option<Vec<state::FileReview>>) {
    conclude(&review, accept, &notes, reviews)
}

/// Publishes the review for the phone and waits for its decision, while
/// the window waits for the reviewer here. The first decision wins.
fn publish(review: Arc<Review>, handle: tauri::AppHandle) {
    let Some(config) = remote::load() else { return };
    std::thread::spawn(move || {
        let document = build_context(&review).and_then(|context| {
            Ok(Document { version: 1, context, changes: build_changes(&review)? })
        });
        let published = document
            .and_then(|doc| serde_json::to_value(doc).map_err(|e| e.to_string()))
            .and_then(|doc| remote::publish(&config, &doc));
        let published = match published {
            Ok(p) => p,
            Err(e) => {
                eprintln!("commit-review: {e}");
                let _ = handle.emit("remote", serde_json::json!({ "error": e }));
                return;
            }
        };
        let _ = handle.emit("remote", serde_json::json!({ "url": published.url }));
        let id = published.id.clone();
        *review.remote.lock().unwrap() = Some(published);
        match remote::wait(&config, &id) {
            Ok(decision) => conclude(&review, decision.accept, &decision.notes, decision.reviews),
            // Withdrawn: the window decided, the process is on its way out.
            Err(e) => eprintln!("commit-review: {e}"),
        }
    });
}

fn review(command: Option<String>, output: Output) -> ! {
    // The command may move first: `cd repo && git commit`, `git -C repo`.
    if let Some(dir) = command.as_deref().and_then(message::working_dir) {
        if let Err(e) = std::env::set_current_dir(&dir) {
            eprintln!("commit-review: cannot enter {dir}: {e}");
        }
    }
    // Git paths are shown relative to the root; the cwd may be deeper.
    if let Ok(root) = git::run(&["rev-parse", "--show-toplevel"]) {
        std::env::set_current_dir(root).expect("repository root exists");
    }
    let review = Arc::new(Review { command, output, files: Mutex::new(Vec::new()), remote: Mutex::new(None) });
    let app = tauri::Builder::default()
        .manage(review.clone())
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
    publish(review, app.handle().clone());

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
