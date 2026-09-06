#!/bin/bash
# Claude Code PreToolUse hook (matcher: Bash).
# Lets everything through except `git commit`, for which it opens
# commit-review and denies the commit when the reviewer clicks Deny.
#
# Binary contract: exit 0 = accept, exit 10 = deny with the reason on stdout.
# Any other code = failure: deny as well. A gate that silently lets commits
# through when it is broken is worthless.
set -u

BIN="$HOME/Documents/projects/commit-review/target/release/commit-review"
LOG="$HOME/Library/Logs/commit-review.log"

input=$(cat)
cmd=$(printf '%s' "$input" | jq -r '.tool_input.command // empty')
case "$cmd" in
  *"git commit"*) ;;
  *) exit 0 ;;
esac

cwd=$(printf '%s' "$input" | jq -r '.cwd // empty')
[ -n "$cwd" ] && cd "$cwd"

deny() {
  jq -n --arg r "$1" \
    '{hookSpecificOutput:{hookEventName:"PreToolUse",permissionDecision:"deny",permissionDecisionReason:$r}}'
  exit 0
}

[ -x "$BIN" ] || deny "commit-review: binary missing ($BIN). Run \`cargo build --release\` in ~/Documents/projects/commit-review."

out=$("$BIN" --command "$cmd" 2>>"$LOG")
code=$?
case "$code" in
  0)  exit 0 ;;
  10) deny "$out" ;;
  *)  deny "commit-review crashed (exit $code), see $LOG. Commit denied as a precaution." ;;
esac
