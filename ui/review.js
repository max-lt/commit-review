// Review view: the diff of what the commit will contain, with line comments
// that end up in the deny reason. Loaded before app.js, which defines the
// shared helpers it calls at click time.

let files = [];        // FileDiff[] from the binary, each with a flat line list
let comments = [];     // { file, start, end, text, box }; start = FILE for a file comment
let drag = null;       // { file, start, end } while lines are being selected
let reviewOpen = false;
const rows = new Map(); // "file:index" -> line <tr>
const boxes = [];       // file index -> .file element
const checks = [];      // file index -> Viewed checkbox
const treeItems = [];   // file index -> <li> in the tree
const viewed = new Set();

const FILE = -1;
const MARK = { context: " ", add: "+", del: "-" };
const SCOPE_LABEL = {
  staged: "Staged changes only: plain git commit",
  tracked: "Tracked files as they are: git commit -a",
  worktree: "Working tree, untracked files included: git add runs first",
};
const SVG = "http://www.w3.org/2000/svg";
const CHEVRON = "M12.78 5.22a.749.749 0 0 1 0 1.06l-4.25 4.25a.749.749 0 0 1-1.06 0L3.22 6.28a.749.749 0 1 1 1.06-1.06L8 8.939l3.72-3.719a.749.749 0 0 1 1.06 0Z";
const BUBBLE = "M1 2.75C1 1.784 1.784 1 2.75 1h10.5c.966 0 1.75.784 1.75 1.75v7.5A1.75 1.75 0 0 1 13.25 12H9.06l-2.573 2.573A1.458 1.458 0 0 1 4 13.543V12H2.75A1.75 1.75 0 0 1 1 10.25Zm1.75-.25a.25.25 0 0 0-.25.25v7.5c0 .138.112.25.25.25h2a.75.75 0 0 1 .75.75v2.19l2.72-2.72a.749.749 0 0 1 .53-.22h4.5a.25.25 0 0 0 .25-.25v-7.5a.25.25 0 0 0-.25-.25Z";

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
const icon = (path) => {
  const svg = document.createElementNS(SVG, "svg");
  svg.setAttribute("viewBox", "0 0 16 16");
  svg.setAttribute("width", "16");
  svg.setAttribute("height", "16");
  const p = document.createElementNS(SVG, "path");
  p.setAttribute("d", path);
  p.setAttribute("fill", "currentColor");
  svg.append(p);
  return svg;
};

async function toggleReview() {
  reviewOpen = !reviewOpen;
  $("summary").hidden = reviewOpen;
  $("review").hidden = !reviewOpen;
  $("review-head").hidden = !reviewOpen;
  $("toggle-review").textContent = reviewOpen ? "Summary" : "Review changes";
  if (reviewOpen && !files.length) await loadReview();
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
  $("files").textContent = "";
  $("files").classList.toggle("empty", !files.length);
  if (!files.length) $("files").textContent = "(nothing to commit in this scope)";
  files.forEach((f, i) => $("files").append(renderFile(f, i)));
  renderTree();
  files.forEach((f, i) => {
    if (f.viewed) setViewed(i, true);
    f.restored.forEach((r) => restore(i, r));
  });
  updateViewed();
  markCommented();
}

function setViewed(fi, on) {
  checks[fi].checked = on;
  if (on) viewed.add(fi); else viewed.delete(fi);
  boxes[fi].classList.toggle("collapsed", on);
  treeItems[fi].classList.toggle("viewed", on);
  updateViewed();
}

// A comment from an earlier attempt: back in place while its lines are
// unchanged, otherwise shown as outdated and not sent unless reopened.
function restore(fi, r) {
  if (r.anchor === "outdated") {
    renderOutdated(fi, r);
    return;
  }
  const c = r.anchor === "lines"
    ? { file: fi, start: r.start, end: r.end, text: r.text, earlier: true }
    : { file: fi, start: FILE, end: FILE, text: r.text, earlier: true };
  comments.push(c);
  renderComment(c);
}

function renderOutdated(fi, r) {
  const box = el("div", "comment outdated");
  const head = el("div", "comment-head");
  head.append(el("span", "author", user), el("span", "tag outdated", "Outdated"), el("span", "where", files[fi].path));
  box.append(head);
  if (r.quote.length) box.append(el("pre", "quote", r.quote.join("\n")));
  const actions = el("div", "comment-actions");
  actions.append(
    button("Dismiss", () => box.remove()),
    button("Reopen", () => {
      box.remove();
      const c = { file: fi, start: FILE, end: FILE, text: r.text };
      comments.push(c);
      renderComment(c);
      markCommented();
    }),
  );
  box.append(el("div", "comment-body", r.text), actions);
  commentCell(fi, FILE).append(box);
}

// What to keep for the next attempt; null when the diff was never opened.
function reviewState() {
  if (!files.length) return null;
  return files.map((f, i) => ({
    path: f.path,
    viewed: viewed.has(i),
    comments: comments.filter((c) => c.file === i).map((c) => ({
      start: c.start === FILE ? null : c.start,
      end: c.end === FILE ? null : c.end,
      text: c.text,
    })),
  }));
}

function renderFile(file, fi) {
  const box = el("div", "file");
  boxes[fi] = box;
  const head = el("div", "file-head");
  const chevron = button(null, () => box.classList.toggle("collapsed"), "chevron");
  chevron.append(icon(CHEVRON));
  chevron.title = "Collapse or expand";
  const added = file.flat.filter((l) => l.kind === "add").length;
  const removed = file.flat.filter((l) => l.kind === "del").length;
  const counts = el("span", "file-counts");
  counts.append(el("span", "add", "+" + added), " ", el("span", "del", "-" + removed));
  const viewedLabel = el("label", "viewed-label");
  const check = el("input");
  check.type = "checkbox";
  check.onchange = () => setViewed(fi, check.checked);
  checks[fi] = check;
  viewedLabel.append(check, "Viewed");
  const comment = button(null, () => openForm(fi, FILE, FILE), "file-comment");
  comment.append(icon(BUBBLE));
  comment.title = "Comment on this file";
  head.append(
    chevron,
    el("span", "file-status " + file.status, file.status),
    el("span", "file-path", file.old_path ? file.old_path + " -> " + file.path : file.path),
    counts,
    viewedLabel,
    comment,
  );
  box.append(head, el("div", "file-comments"));
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
  const scroller = el("div", "diff-scroll");
  scroller.append(table);
  box.append(scroller);
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

// Sidebar: directories as collapsible groups, files that scroll to their diff.
function renderTree() {
  const root = { dirs: new Map(), files: [] };
  files.forEach((f, i) => {
    const parts = f.path.split("/");
    let node = root;
    parts.slice(0, -1).forEach((dir) => {
      if (!node.dirs.has(dir)) node.dirs.set(dir, { dirs: new Map(), files: [] });
      node = node.dirs.get(dir);
    });
    node.files.push({ name: parts.at(-1), index: i });
  });
  $("tree").textContent = "";
  $("tree").append(renderTreeNode(root));
}

function renderTreeNode(node) {
  const ul = el("ul", "tree-level");
  [...node.dirs.keys()].sort().forEach((name) => {
    const details = el("details");
    details.open = true;
    details.append(el("summary", "tree-dir", name), renderTreeNode(node.dirs.get(name)));
    const li = el("li");
    li.append(details);
    ul.append(li);
  });
  node.files.sort((a, b) => a.name.localeCompare(b.name)).forEach(({ name, index }) => {
    const li = el("li", "tree-file");
    li.append(button(name, () => reveal(index)));
    treeItems[index] = li;
    ul.append(li);
  });
  return ul;
}

function reveal(fi) {
  boxes[fi].classList.remove("collapsed");
  boxes[fi].scrollIntoView({ block: "start", behavior: "smooth" });
}

function filterFiles(query) {
  const q = query.trim().toLowerCase();
  files.forEach((f, i) => {
    const hidden = Boolean(q) && !f.path.toLowerCase().includes(q);
    boxes[i].hidden = hidden;
    treeItems[i].hidden = hidden;
  });
  document.querySelectorAll("#tree details").forEach((d) => {
    d.parentElement.hidden = !d.querySelector("li.tree-file:not([hidden])");
  });
}

function updateViewed() {
  $("review-count").textContent = viewed.size + " / " + files.length + " viewed";
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

// Where the comments and forms of a line, or of the whole file, live.
function commentCell(fi, idx) {
  if (idx === FILE) return boxes[fi].querySelector(".file-comments");
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
  if (idx === FILE) return;
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
  if (start === FILE) return files[fi].path + " (file comment)";
  return files[fi].path + ":" + lineRef(files[fi].flat.slice(start, end + 1));
}

function openForm(fi, start, end, existing) {
  boxes[fi].classList.remove("collapsed");
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
  head.append(el("span", "author", user), el("span", "tag pending", "Pending"));
  if (c.earlier) head.append(el("span", "tag earlier", "Earlier round"));
  head.append(el("span", "where", where(c.file, c.start, c.end)));
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

// The pending comments as text for the agent: where, the quoted lines, the note.
function reviewText() {
  return comments.map((c) => {
    if (c.start === FILE) return where(c.file, FILE, FILE) + "\n" + c.text;
    const lines = files[c.file].flat.slice(c.start, c.end + 1);
    const quoted = lines.map((l) => "> " + MARK[l.kind] + l.text).join("\n");
    return where(c.file, c.start, c.end) + "\n" + quoted + "\n" + c.text;
  }).join("\n\n");
}
