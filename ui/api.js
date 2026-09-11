// The one seam between the review UI and whatever serves the review:
// the Tauri window, with its commands, or the web, where a published
// review lives behind a small HTTP API. Loaded before review.js and app.js.
//
// Review format, version 1, as the web API serves it and Tauri returns it
// in two pieces:
//   { version: 1, context: <context command>, changes: <changes command> }
// A decision is { accept, notes, reviews } as the decide command takes it.

const api = (() => {
  if (window.__TAURI__) {
    const { invoke } = window.__TAURI__.core;
    return {
      remote: false,
      context: () => invoke("context"),
      changes: () => invoke("changes"),
      decide: (decision) => invoke("decide", decision),
      onDeadline: (cb) => window.__TAURI__.event.listen("deadline", cb),
    };
  }
  // Web: the page is /r/<id>, the review at /api/reviews/<id>. Both
  // pieces come from the same document, fetched once.
  const base = location.pathname.replace(/^\/r\//, "/api/reviews/").replace(/\/$/, "");
  const headers = () => {
    const token = localStorage.getItem("commit-review-token");
    return token ? { authorization: "Bearer " + token } : {};
  };
  let review = null;
  const load = () => (review ??= fetch(base, { headers: headers() }).then(async (r) => {
    if (r.status === 401) location.href = "/?next=" + encodeURIComponent(location.pathname);
    if (!r.ok) throw new Error(await r.text() || r.statusText);
    return r.json();
  }));
  return {
    remote: true,
    context: () => load().then((doc) => doc.context),
    changes: () => load().then((doc) => doc.changes),
    decide: (decision) => fetch(base + "/decide", {
      method: "POST",
      headers: { "content-type": "application/json", ...headers() },
      body: JSON.stringify(decision),
    }).then(async (r) => {
      if (!r.ok) throw new Error(await r.text() || r.statusText);
      document.body.innerHTML = '<p class="decided">' + (decision.accept ? "Accepted" : "Denied") + ". You can close this page.</p>";
    }),
    onDeadline: () => {},
  };
})();
