#!/bin/bash
# Claude Code PreToolUse hook (matcher: Bash).
# Lets everything through except a command that runs `git commit`, for which
# it opens commit-review and denies the commit when the reviewer clicks Deny.
#
# Binary contract: exit 0 = accept, exit 10 = deny with the reason on stdout.
# Any other code = failure: deny as well. A gate that silently lets commits
# through when it is broken is worthless.
set -u

BIN="$HOME/Documents/projects/commit-review/target/release/commit-review"
LOG="$HOME/Library/Logs/commit-review.log"

input=$(cat)
cmd=$(printf '%s' "$input" | jq -r '.tool_input.command // empty')
[ -n "$cmd" ] || exit 0

deny() {
  jq -n --arg r "$1" \
    '{hookSpecificOutput:{hookEventName:"PreToolUse",permissionDecision:"deny",permissionDecisionReason:$r}}'
  exit 0
}

if [ ! -x "$BIN" ]; then
  # Without the binary, a substring match keeps blocking commits, and only
  # commits, until it is rebuilt.
  case "$cmd" in
    *"git commit"*) deny "commit-review: binary missing ($BIN). Run \`cargo build --release\` in ~/Documents/projects/commit-review." ;;
    *) exit 0 ;;
  esac
fi

"$BIN" --is-commit "$cmd" 2>>"$LOG"
case $? in
  0) ;;
  1) exit 0 ;;
  *) deny "commit-review --is-commit crashed on this command, see $LOG. Denied as a precaution." ;;
esac

cwd=$(printf '%s' "$input" | jq -r '.cwd // empty')
[ -n "$cwd" ] && cd "$cwd"

out=$("$BIN" --command "$cmd" 2>>"$LOG")
case $? in
  0)  exit 0 ;;
  10) deny "$out" ;;
  *)  deny "commit-review crashed, see $LOG. Commit denied as a precaution." ;;
esac
