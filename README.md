# commit-review

Human review window before a commit started by Claude Code: the commit
message with its subject and body, what deserves a look in it (characters
outside printable ASCII, emails, links, Co-authored-by trailers), the
changed files, the exact command, a review view of the diff with line
comments, notes for the agent, and two buttons: Accept or Deny.

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
with the notes as the reason; Accept with notes prints an allow with the
notes as `additionalContext`, so the agent reads them either way. A crash
denies as well.

The second form is a manual launch inside a git repository. With
`--command`, it extracts the commit message (`-m`, `--message`, `-am`,
`$(cat <<'EOF' ... EOF)` heredoc) and shows it; without, only the file
list is shown. Parser: `src/message.rs`.

| Decision              | stdout            | exit  |
| --------------------- | ----------------- | ----- |
| Accept                | the notes, if any | 0     |
| Deny                  | the notes         | 10    |
| Window closed, Cmd+Q  | the reason        | 10    |
| Failure               | stderr            | other |

The exit code is the binary's own. In a shell wrapper such as
`commit-review; echo $?`, the echo returns 0, not the binary.

## Review view

"Review changes" swaps the summary for the diff of what the commit will
contain: the index for a plain `git commit`, tracked files for `-a`, the
whole working tree when a `git add` runs first; against HEAD~1 for an
amend. A file tree with a filter sits on the left; each file collapses,
takes a file-level comment, and can be marked Viewed, which collapses it
and counts it. Lines are numbered on both sides. The "+" on a line opens
a comment; dragging it selects a range. Comments stay pending and
editable until the decision, which sends them to the agent after the notes,
each as:

    src/main.rs:L42-L45
    > -old line
    > +new line
    the comment

The review carries over between attempts. Its state lives in
`.git/commit-review/state.json`, saved on Deny and cleared on Accept. A
file whose diff did not change keeps its Viewed mark. A comment comes
back pending, and goes to the agent again, as long as the lines it quotes
are still in the diff; once they changed, the agent acted on it, and the
comment shows as outdated, not sent unless reopened. Closing the window
without a decision saves nothing.

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
