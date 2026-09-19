// commit-review, remote side. Identity is a GitHub account, obtained on
// both the machine and the phone through the device flow, so the worker
// never holds a client secret. Every request under /api carries a token
// "<github id>.<secret>": the id routes to the user's Durable Object, the
// secret is checked there.

const json = (data, status = 200, headers = {}) =>
  new Response(JSON.stringify(data), { status, headers: { "content-type": "application/json", ...headers } });
const text = (body, status) => new Response(body, { status });

/// Keeps the review documents under the storage value limit.
const CHUNK = 100 * 1024;
/// A review nobody picked up is forgotten after a day.
const TTL_MS = 24 * 60 * 60 * 1000;
/// How long a wait request parks before the client asks again.
const WAIT_MS = 25 * 1000;

const GITHUB = "https://github.com";
const GITHUB_API = "https://api.github.com";
const UA = "commit-review";

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    const path = url.pathname;
    if (path === "/auth/start" && request.method === "POST") return authStart(env);
    if (path === "/auth/poll" && request.method === "POST") return authPoll(request, env);
    if (path.startsWith("/api/")) return withUser(request, env);
    // Pages: the list at /, a review at /r/<id>; the rest are the UI files.
    if (path === "/") return env.ASSETS.fetch(new Request(url.origin + "/remote.html", request));
    if (path.startsWith("/r/")) return env.ASSETS.fetch(new Request(url.origin + "/index.html", request));
    return env.ASSETS.fetch(request);
  },
};

async function authStart(env) {
  const res = await fetch(GITHUB + "/login/device/code", {
    method: "POST",
    headers: { accept: "application/json", "content-type": "application/json", "user-agent": UA },
    body: JSON.stringify({ client_id: env.GITHUB_CLIENT_ID, scope: "read:user" }),
  });
  if (!res.ok) return text("GitHub refused the device flow: " + (await res.text()), 502);
  const { device_code, user_code, verification_uri, expires_in, interval } = await res.json();
  return json({ device_code, user_code, verification_uri, expires_in, interval });
}

/// One poll of the device flow. 202 while the user has not authorized;
/// 200 with our own token once GitHub says who they are.
async function authPoll(request, env) {
  const { device_code, kind, name } = await request.json();
  const res = await fetch(GITHUB + "/login/oauth/access_token", {
    method: "POST",
    headers: { accept: "application/json", "content-type": "application/json", "user-agent": UA },
    body: JSON.stringify({
      client_id: env.GITHUB_CLIENT_ID,
      device_code,
      grant_type: "urn:ietf:params:oauth:grant-type:device_code",
    }),
  });
  const body = await res.json();
  if (body.error === "authorization_pending" || body.error === "slow_down") return json({ pending: body.error }, 202);
  if (body.error || !body.access_token) return json({ error: body.error_description || body.error || "no token" }, 400);
  const who = await fetch(GITHUB_API + "/user", {
    headers: { authorization: "Bearer " + body.access_token, accept: "application/vnd.github+json", "user-agent": UA },
  });
  if (!who.ok) return text("GitHub did not tell who you are", 502);
  const { id, login } = await who.json();
  const box = env.USER.get(env.USER.idFromName("gh:" + id));
  const registered = await box.fetch("https://box/register", {
    method: "POST",
    body: JSON.stringify({ kind: kind === "machine" ? "machine" : "browser", name: String(name || ""), login }),
  });
  const { secret } = await registered.json();
  return json({ token: id + "." + secret, login });
}

/// Routes an /api request to the Durable Object of the token's user.
function withUser(request, env) {
  const auth = request.headers.get("authorization") || "";
  const token = auth.startsWith("Bearer ") ? auth.slice(7) : "";
  const dot = token.indexOf(".");
  if (dot <= 0) return text("missing token", 401);
  const box = env.USER.get(env.USER.idFromName("gh:" + token.slice(0, dot)));
  const headers = new Headers(request.headers);
  headers.set("x-token-secret", token.slice(dot + 1));
  return box.fetch(new Request(request, { headers }));
}

const randomId = (bytes) => {
  const buf = crypto.getRandomValues(new Uint8Array(bytes));
  return btoa(String.fromCharCode(...buf)).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
};

/// One GitHub user: their tokens, their pending reviews, and whoever is
/// waiting for a decision right now.
export class UserBox {
  constructor(state) {
    this.storage = state.storage;
    /// review id -> resolvers of parked wait requests.
    this.waiters = new Map();
  }

  async fetch(request) {
    const url = new URL(request.url);
    const parts = url.pathname.split("/").filter(Boolean);
    if (url.pathname === "/register" && request.method === "POST") {
      const { kind, name, login } = await request.json();
      const secret = randomId(32);
      await this.storage.put("token:" + secret, { kind, name, created: Date.now() });
      await this.storage.put("profile", { login });
      return json({ secret });
    }
    const secret = request.headers.get("x-token-secret");
    const token = secret && (await this.storage.get("token:" + secret));
    if (!token) return text("unknown token", 401);
    // parts: ["api", "reviews", id?, action?]
    if (parts[1] === "me") return json({ ...(await this.storage.get("profile")), kind: token.kind, name: token.name });
    if (parts[1] !== "reviews") return text("not found", 404);
    const id = parts[2];
    const action = parts[3];
    if (!id && request.method === "GET") return this.list();
    if (!id && request.method === "POST") return this.create(request, token);
    if (!id) return text("not found", 404);
    const meta = await this.storage.get("review:" + id);
    if (!meta) return text("gone", 410);
    if (!action && request.method === "GET") return this.read(id, meta);
    if (!action && request.method === "DELETE") return this.remove(id, meta, "elsewhere");
    if (action === "wait" && request.method === "GET") return this.wait(id);
    if (action === "decide" && request.method === "POST") return this.decide(id, meta, await request.json());
    return text("not found", 404);
  }

  async list() {
    const all = await this.storage.list({ prefix: "review:" });
    const reviews = [];
    for (const [key, meta] of all) {
      if (key.split(":").length !== 2) continue;
      if (Date.now() - meta.created > TTL_MS) {
        await this.remove(key.slice("review:".length), meta, "expired");
        continue;
      }
      reviews.push({ id: key.slice("review:".length), ...meta });
    }
    reviews.sort((a, b) => b.created - a.created);
    return json(reviews);
  }

  /// Stores a review document in chunks and answers with its id.
  async create(request, token) {
    const body = await request.text();
    let doc;
    try {
      doc = JSON.parse(body);
    } catch {
      return text("review is not JSON", 400);
    }
    if (doc.version !== 1 || !doc.context) return text("unknown review format", 400);
    const id = randomId(12);
    const chunks = Math.ceil(body.length / CHUNK);
    for (let i = 0; i < chunks; i++) {
      await this.storage.put(`review:${id}:chunk:${i}`, body.slice(i * CHUNK, (i + 1) * CHUNK));
    }
    const ctx = doc.context;
    await this.storage.put("review:" + id, {
      created: Date.now(),
      machine: token.name,
      repo: ctx.repo,
      branch: ctx.branch || null,
      subject: ctx.message ? ctx.message.subject : null,
      amend: Boolean(ctx.amend),
      files: Array.isArray(doc.changes) ? doc.changes.length : 0,
      chunks,
      decision: null,
    });
    return json({ id });
  }

  async read(id, meta) {
    if (meta.decision) return text("decided", 410);
    const pieces = [];
    for (let i = 0; i < meta.chunks; i++) pieces.push(await this.storage.get(`review:${id}:chunk:${i}`));
    return new Response(pieces.join(""), { headers: { "content-type": "application/json" } });
  }

  /// The machine asks for the decision; parked until one comes or the
  /// wait runs out (204, ask again). A delivered decision ends the review.
  async wait(id) {
    let meta = await this.storage.get("review:" + id);
    if (!meta.decision) {
      await new Promise((resolve) => {
        const set = this.waiters.get(id) || new Set();
        set.add(resolve);
        this.waiters.set(id, set);
        setTimeout(resolve, WAIT_MS);
      });
      meta = await this.storage.get("review:" + id);
    }
    if (!meta || !meta.decision) return new Response(null, { status: 204 });
    await this.purge(id, meta);
    return json(meta.decision);
  }

  async decide(id, meta, decision) {
    if (meta.decision) return text("already decided", 409);
    if (typeof decision.accept !== "boolean") return text("decision needs accept", 400);
    meta.decision = { accept: decision.accept, notes: String(decision.notes || ""), reviews: decision.reviews || null, at: Date.now() };
    await this.storage.put("review:" + id, meta);
    this.wake(id);
    return json({ ok: true });
  }

  /// The machine decided elsewhere, or the review is stale: gone for all.
  async remove(id, meta, why) {
    await this.purge(id, meta);
    this.wake(id);
    return json({ removed: why });
  }

  async purge(id, meta) {
    const keys = ["review:" + id];
    for (let i = 0; i < meta.chunks; i++) keys.push(`review:${id}:chunk:${i}`);
    await this.storage.delete(keys);
  }

  wake(id) {
    const set = this.waiters.get(id);
    if (!set) return;
    this.waiters.delete(id);
    set.forEach((resolve) => resolve());
  }
}
