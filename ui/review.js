// Review view: the diff of what the commit will contain, with line comments
// that end up in the deny reason. Loaded before app.js, which defines the
// shared helpers it calls at click time.

let files = [];        // FileDiff[] from the binary, each with a flat line list
let comments = [];     // { file, start, end, text, box }
let drag = null;       // { file, start, end } while lines are being selected
let reviewOpen = false;
const rows = new Map(); // "file:index" -> line <tr>

const MARK = { context: " ", add: "+", del: "-" };
const SCOPE_LABEL = {
  staged: "Staged changes only: plain git commit",
  tracked: "Tracked files as they are: git commit -a",
  worktree: "Working tree, untracked files included: git add runs first",
};
const SUMMARY_SIZE = { width: 620, height: 680 };
const REVIEW_SIZE = { width: 1100, height: 820 };

const el = (tag, className, text) => {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text != null) e.textContent = text;
  return e;
};
const button = (label, onClick, className) => {
  const b = el("button", className, label);
  b.type = "button";
  b.onclick = onClick;
  return b;
};

async function toggleReview() {
  reviewOpen = !reviewOpen;
  $("summary").hidden = reviewOpen;
  $("review").hidden = !reviewOpen;
  $("toggle-review").textContent = reviewOpen ? "Summary" : "Review changes";
  if (reviewOpen && !files.length) await loadReview();
  await invoke("resize", reviewOpen ? REVIEW_SIZE : SUMMARY_SIZE);
}

async function loadReview() {
  $("files").textContent = "Reading the diff...";
  try {
    files = await invoke("changes");
  } catch (e) {
    $("files").textContent = "git error: " + e;
    return;
  }
  files.forEach((f) => { f.flat = f.hunks.flatMap((h) => h.lines); });
  $("review-scope").textContent = SCOPE_LABEL[scope] || "";
  $("review-count").textContent = files.length + (files.length === 1 ? " file" : " files");
  $("files").textContent = "";
  $("files").classList.toggle("empty", !files.length);
  if (!files.length) $("files").textContent = "(nothing to commit in this scope)";
  files.forEach((f, i) => $("files").append(renderFile(f, i)));
}

function renderFile(file, fi) {
  const box = el("div", "file");
  const head = el("div", "file-head");
  const added = file.flat.filter((l) => l.kind === "add").length;
  const removed = file.flat.filter((l) => l.kind === "del").length;
  const counts = el("span", "file-counts");
  counts.append(el("span", "add", "+" + added), " ", el("span", "del", "-" + removed));
  head.append(
    el("span", "file-status " + file.status, file.status),
    el("span", "file-path", file.old_path ? file.old_path + " -> " + file.path : file.path),
    counts,
  );
  box.append(head);
  if (file.binary) {
    box.append(el("div", "file-note", "Binary file, no diff."));
    return box;
  }
  const table = el("table", "diff");
  let idx = 0;
  file.hunks.forEach((hunk) => {
    const tr = el("tr", "hunk");
    tr.append(el("td", "no"), el("td", "no"), el("td", "code", hunk.header));
    table.append(tr);
    hunk.lines.forEach((line) => {
      table.append(renderLine(line, fi, idx));
      idx += 1;
    });
  });
  box.append(table);
  return box;
}

function renderLine(line, fi, idx) {
  const tr = el("tr", "line " + line.kind);
  tr.append(el("td", "no", line.old ?? ""), el("td", "no", line.new ?? ""));
  const code = el("td", "code");
  const plus = button("+", null, "plus");
  plus.title = "Add a comment; drag to select several lines";
  plus.onmousedown = (e) => {
    e.preventDefault();
    drag = { file: fi, start: idx, end: idx };
    highlight();
  };
  code.append(plus, el("span", "marker", MARK[line.kind]), el("span", "text", line.text));
  tr.append(code);
  tr.onmouseenter = () => {
    if (drag && drag.file === fi) {
      drag.end = idx;
      highlight();
    }
  };
  rows.set(fi + ":" + idx, tr);
  return tr;
}

document.addEventListener("mouseup", () => {
  if (!drag) return;
  const { file, start, end } = drag;
  drag = null;
  document.querySelectorAll("tr.selected").forEach((r) => r.classList.remove("selected"));
  openForm(file, Math.min(start, end), Math.max(start, end));
});

function highlight() {
  const [a, b] = [Math.min(drag.start, drag.end), Math.max(drag.start, drag.end)];
  document.querySelectorAll("tr.selected").forEach((r) => r.classList.remove("selected"));
  for (let i = a; i <= b; i++) rows.get(drag.file + ":" + i)?.classList.add("selected");
}

// The cell under a line where its comments and forms live.
function commentCell(fi, idx) {
  const line = rows.get(fi + ":" + idx);
  let row = line.nextElementSibling;
  if (!row || !row.classList.contains("comment-row")) {
    row = el("tr", "comment-row");
    const td = el("td");
    td.colSpan = 3;
    row.append(td);
    line.after(row);
  }
  return row.firstElementChild;
}

function pruneRow(fi, idx) {
  const row = rows.get(fi + ":" + idx).nextElementSibling;
  if (row && row.classList.contains("comment-row") && !row.firstElementChild.childElementCount) row.remove();
}

function markCommented() {
  document.querySelectorAll("tr.commented").forEach((r) => r.classList.remove("commented"));
  comments.forEach((c) => {
    for (let i = c.start; i <= c.end; i++) rows.get(c.file + ":" + i)?.classList.add("commented");
  });
  const n = comments.length;
  $("pending-count").textContent = n ? n + (n === 1 ? " pending comment" : " pending comments") : "";
}

function where(fi, start, end) {
  return files[fi].path + ":" + lineRef(files[fi].flat.slice(start, end + 1));
}

function openForm(fi, start, end, existing) {
  const form = el("div", "comment-form");
  const ta = el("textarea");
  ta.rows = 3;
  ta.placeholder = "Leave a comment";
  ta.value = existing ? existing.text : "";
  const cancel = () => {
    form.remove();
    if (existing) renderComment(existing);
    pruneRow(fi, end);
  };
  const submit = () => {
    const text = ta.value.trim();
    if (!text) return;
    form.remove();
    if (existing) {
      existing.text = text;
      renderComment(existing);
    } else {
      const c = { file: fi, start, end, text };
      comments.push(c);
      renderComment(c);
    }
    markCommented();
  };
  ta.onkeydown = (e) => {
    if (e.key === "Escape") { e.stopPropagation(); cancel(); }
    if (e.key === "Enter" && e.metaKey) { e.stopPropagation(); submit(); }
  };
  const actions = el("div", "form-actions");
  actions.append(button("Cancel", cancel), button(existing ? "Update comment" : "Add review comment", submit, "primary"));
  form.append(el("div", "where", where(fi, start, end)), ta, actions);
  commentCell(fi, end).append(form);
  ta.focus();
}

function renderComment(c) {
  const box = el("div", "comment");
  const head = el("div", "comment-head");
  head.append(el("span", "author", user), el("span", "pending", "Pending"), el("span", "where", where(c.file, c.start, c.end)));
  const actions = el("div", "comment-actions");
  actions.append(
    button("Edit", () => { box.remove(); openForm(c.file, c.start, c.end, c); }),
    button("Delete", () => {
      box.remove();
      comments.splice(comments.indexOf(c), 1);
      pruneRow(c.file, c.end);
      markCommented();
    }),
  );
  box.append(head, el("div", "comment-body", c.text), actions);
  c.box = box;
  commentCell(c.file, c.end).append(box);
}

// "L12", "L12-L15", or old-file numbers for deleted lines only.
function lineRef(lines) {
  const news = lines.map((l) => l.new).filter((n) => n != null);
  if (news.length) return news[0] === news.at(-1) ? "L" + news[0] : "L" + news[0] + "-L" + news.at(-1);
  const olds = lines.map((l) => l.old);
  return "old L" + olds[0] + (olds.length > 1 ? "-L" + olds.at(-1) : "");
}

// The pending comments as text for Claude: where, the quoted lines, the note.
function reviewText() {
  return comments.map((c) => {
    const lines = files[c.file].flat.slice(c.start, c.end + 1);
    const quoted = lines.map((l) => "> " + MARK[l.kind] + l.text).join("\n");
    return where(c.file, c.start, c.end) + "\n" + quoted + "\n" + c.text;
  }).join("\n\n");
}
