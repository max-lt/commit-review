# commit-review

Human review window before a commit started by Claude Code.
Spike: the commit message (subject and body), the list of changed files,
the exact command, an optional deny reason, two buttons: Accept or Deny.

## Build

    cargo build --release
    cargo test

The binary is `target/release/commit-review`. That directory is on the PATH
through `~/.zshrc`, so there is nothing to install.

## Binary contract

    commit-review [--command "<intercepted shell command>"]
    commit-review --is-commit "<shell command>"

The first form runs inside a git repository, opens the window and exits on
the decision. With `--command`, it extracts the commit message (`-m`,
`--message`, `-am`, `$(cat <<'EOF' ... EOF)` heredoc) and shows it;
without, it is a manual launch and only the file list is shown.

The second form opens no window: it exits 0 when the command runs
`git commit` itself, 1 when it only mentions it inside a quoted string or
a heredoc body, or not at all. Parser: `src/message.rs`.

| Decision              | stdout     | exit  |
| --------------------- | ---------- | ----- |
| Accept                | nothing    | 0     |
| Deny                  | the reason | 10    |
| Window closed, Cmd+Q  | the reason | 10    |
| Failure               | stderr     | other |

The exit code is the binary's own. In a shell wrapper such as
`commit-review; echo $?`, the echo returns 0, not the binary.

## Claude Code hook

`hooks/review-before-commit.sh` is a PreToolUse hook on the Bash tool.
It asks the binary whether the command runs `git commit`; if so, it opens
the window and denies the commit with the reason when the exit code is 10.

It is wired in this project's `.claude/settings.json`. To enable it
everywhere, copy the `hooks` block into `~/.claude/settings.json`.
Claude Code snapshots hooks at startup: restart the session after editing
settings.

Rebuild the binary before changing the hook script: the script is read on
every Bash call of every session, and an old binary given a new flag opens
the window instead of answering. The gate fails closed: without the binary,
every command mentioning `git commit` is denied with a message saying to
rebuild.
