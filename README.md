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

    commit-review hook
    commit-review [--command "<shell command>"]

`hook` is the Claude Code PreToolUse entry point. It reads the event JSON
on stdin and exits 0 at once unless the command runs `git commit` itself;
a mention inside a quoted string or a heredoc body does not count. Then it
opens the window in the event's cwd. Deny prints the hook's JSON answer
with the reason. A crash denies as well.

The second form is a manual launch inside a git repository. With
`--command`, it extracts the commit message (`-m`, `--message`, `-am`,
`$(cat <<'EOF' ... EOF)` heredoc) and shows it; without, only the file
list is shown. Parser: `src/message.rs`.

| Decision              | stdout     | exit  |
| --------------------- | ---------- | ----- |
| Accept                | nothing    | 0     |
| Deny                  | the reason | 10    |
| Window closed, Cmd+Q  | the reason | 10    |
| Failure               | stderr     | other |

The exit code is the binary's own. In a shell wrapper such as
`commit-review; echo $?`, the echo returns 0, not the binary.

## Review view

"Review changes" swaps the summary for the diff of what the commit will
contain: the index for a plain `git commit`, tracked files for `-a`, the
whole working tree when a `git add` runs first; against HEAD~1 for an
amend. Lines are numbered on both sides. The "+" on a line opens a
comment; dragging it selects a range. Comments stay pending and editable
until Deny, which sends them to Claude after the deny reason, each as:

    src/main.rs:L42-L45
    > -old line
    > +new line
    the comment

`--view review` opens the window on that view.

## Claude Code hook

Register the binary as a PreToolUse hook on the Bash tool, in
`~/.claude/settings.json` for every session or in a project's
`.claude/settings.json` for that project only:

    "hooks": {
      "PreToolUse": [{ "matcher": "Bash", "hooks": [{ "type": "command",
        "command": "/Users/max/Documents/projects/commit-review/target/release/commit-review hook",
        "timeout": 3600 }] }]
    }

Settings changes reach running sessions. If the binary is missing, which
only `cargo clean` does, Claude Code reports a hook error and lets commits
through: rebuild.
