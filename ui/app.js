// Summary view and the decision. Loaded after review.js.

const { invoke } = window.__TAURI__.core;
const DEFAULT_DENY = "Commit denied by human review, no reason given. Do not retry the commit: ask the reviewer what should change.";
const $ = (id) => document.getElementById(id);
let scope = "worktree";
let user = "You";

// The notes and comments go to the agent with either decision.
const decide = (accept) => {
  const notes = [$("reason").value.trim(), reviewText()].filter(Boolean).join("\n\n");
  let text = "";
  if (!accept) text = notes ? "Commit denied by human review. Reason:\n" + notes : DEFAULT_DENY;
  else if (notes) text = "Commit accepted by human review, with notes:\n" + notes;
  return invoke("decide", { accept, notes: text });
};
$("accept").onclick = () => decide(true);
$("deny").onclick = () => decide(false);
$("toggle-review").onclick = () => toggleReview();
window.addEventListener("keydown", (e) => {
  if (e.key === "Escape") decide(false);
  if (e.key === "Enter" && e.metaKey) decide(true);
});

// Renders text with each reported character wrapped in a <mark>; characters
// that would be invisible (controls, exotic spaces) show their code point.
const render = (el, text, issues) => {
  el.textContent = "";
  const bad = new Map(issues.map((i) => [i.index, i.code]));
  let run = "";
  const flush = () => { if (run) { el.appendChild(document.createTextNode(run)); run = ""; } };
  [...text].forEach((ch, i) => {
    if (!bad.has(i)) { run += ch; return; }
    flush();
    const code = bad.get(i);
    const hex = "U+" + code.toString(16).toUpperCase().padStart(4, "0");
    const m = document.createElement("mark");
    m.className = "bad";
    m.title = hex;
    m.textContent = (code < 33 || code === 127 || /\s/.test(ch)) ? "<" + hex + ">" : ch;
    el.appendChild(m);
  });
  flush();
};

// Appends a line to the notes, on its own line.
const appendReason = (line) => {
  const r = $("reason");
  if (r.value && !r.value.endsWith("\n")) r.value += "\n";
  r.value += line + "\n";
  r.focus();
};
const badge = (name, count) => {
  if (!count) return;
  const b = document.createElement("button");
  b.type = "button";
  b.className = "badge";
  b.textContent = name + ": " + count + " non-ASCII";
  b.title = "Add to the notes";
  b.onclick = () => appendReason(b.textContent);
  $("message-label").appendChild(b);
};

$("reason").focus();
invoke("context").then((ctx) => {
  const { repo, status, command, message, ascii_issues, amend } = ctx;
  scope = ctx.scope;
  user = ctx.user;
  $("repo").textContent = repo;
  let noChanges = "(no changes)";
  if (amend) {
    $("title").textContent = "The agent wants to amend " + amend.head.split(" ")[0];
    $("amend-head").textContent = amend.head;
    $("amend-stat").textContent = amend.stat;
    $("amend").hidden = false;
    $("message-label").textContent = amend.message_kept ? "Commit message (kept as is)" : "Commit message (replaces the current one)";
    $("files-label").textContent = "Working tree";
    noChanges = "(none: message or metadata only)";
  }
  $("status").textContent = status || noChanges;
  if (command) {
    $("command").textContent = command;
    $("raw").hidden = false;
  }
  if (message) {
    render($("subject"), message.subject, ascii_issues.subject);
    badge("subject", ascii_issues.subject.length);
    if (message.body) {
      render($("body"), message.body, ascii_issues.body);
      badge("body", ascii_issues.body.length);
      $("body").hidden = false;
    }
  } else {
    $("subject").textContent = command ? "(message not recognized, see the exact command)" : "(manual launch, no command given)";
    $("subject").classList.add("muted");
  }
}).catch((e) => {
  $("status").textContent = "git error: " + e;
  $("status").classList.add("err");
});
