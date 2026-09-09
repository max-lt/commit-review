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

// Finding kinds as the binary names them, with singular and plural labels.
const KIND_LABEL = {
  "non-ascii": ["non-ASCII", "non-ASCII"],
  email: ["email", "emails"],
  link: ["link", "links"],
  "co-authored-by": ["Co-authored-by", "Co-authored-by"],
};

// A character that would be invisible (controls, exotic spaces) shows its
// code point instead.
const visible = (ch) => {
  const code = ch.codePointAt(0);
  if (code < 33 || code === 127 || /\s/.test(ch)) return "<U+" + code.toString(16).toUpperCase().padStart(4, "0") + ">";
  return ch;
};

// Renders text with each finding wrapped in a <mark>. Spans go first, then
// single characters, which are the more precise mark.
const render = (el, text, findings) => {
  el.textContent = "";
  const chars = [...text];
  const kinds = new Array(chars.length).fill(null);
  findings.filter((f) => f.kind !== "non-ascii").forEach((f) => { for (let i = f.start; i < f.end; i++) kinds[i] = f.kind; });
  findings.filter((f) => f.kind === "non-ascii").forEach((f) => { kinds[f.start] = f.kind; });
  let i = 0;
  while (i < chars.length) {
    const kind = kinds[i];
    let j = i;
    while (j < chars.length && kinds[j] === kind) j++;
    const run = chars.slice(i, j);
    if (!kind) {
      el.append(run.join(""));
    } else {
      const m = document.createElement("mark");
      m.className = "bad " + kind;
      m.title = KIND_LABEL[kind][0];
      m.textContent = kind === "non-ascii" ? run.map(visible).join("") : run.join("");
      el.append(m);
    }
    i = j;
  }
};

// Appends a line to the notes, on its own line.
const appendReason = (line) => {
  const r = $("reason");
  if (r.value && !r.value.endsWith("\n")) r.value += "\n";
  r.value += line + "\n";
  r.focus();
};

// One badge per kind of finding in a field, e.g. "body: 2 links".
const badges = (name, findings) => {
  const counts = {};
  findings.forEach((f) => { counts[f.kind] = (counts[f.kind] || 0) + 1; });
  Object.keys(KIND_LABEL).filter((kind) => counts[kind]).forEach((kind) => {
    const n = counts[kind];
    const b = document.createElement("button");
    b.type = "button";
    b.className = "badge " + kind;
    b.textContent = name + ": " + n + " " + KIND_LABEL[kind][n === 1 ? 0 : 1];
    b.title = "Add to the notes";
    b.onclick = () => appendReason(b.textContent);
    $("message-label").appendChild(b);
  });
};

$("reason").focus();
invoke("context").then((ctx) => {
  const { repo, status, command, message, findings, amend } = ctx;
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
    render($("subject"), message.subject, findings.subject);
    badges("subject", findings.subject);
    if (message.body) {
      render($("body"), message.body, findings.body);
      badges("body", findings.body);
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
