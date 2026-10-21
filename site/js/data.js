// Data source resolution, fetching, shared state and shared HTML fragments.
import { esc, shortAddr, sigInfo } from "./util.js";

const FALLBACK = { owner: "danielseisenbacher", repo: "poly_position_scraper" };

function resolveConfig() {
  let owner = FALLBACK.owner;
  let repo = FALLBACK.repo;
  const host = location.hostname;
  const gh = host.endsWith("github.io");
  if (gh) {
    owner = host.split(".")[0];
    repo = location.pathname.split("/").filter(Boolean)[0] || host;
  }
  let base;
  try {
    base = new URLSearchParams(location.search).get("data");
  } catch {
    base = null;
  }
  if (!base) base = gh ? `https://raw.githubusercontent.com/${owner}/${repo}/data/` : "./sample-data/";
  if (!base.endsWith("/")) base += "/";
  return { owner, repo, base, sample: base === "./sample-data/" };
}

export const cfg = resolveConfig();
export const DATA_BASE = cfg.base;

/** Shared, mutable app state. Views read from here; `loadCore` fills it. */
export const state = {
  meta: null,
  index: null,
  markets: null,
  clusters: null,
  tape: null,
  err: {},
  loadedAt: 0,
  byAddr: new Map(),
  marketById: new Map(),
  clusterById: new Map(),
  wallets: new Map(), // addr -> Promise<Wallet>
};

const FILES = { meta: "meta.json", index: "index.json", markets: "markets.json", clusters: "clusters.json", tape: "activity_recent.json" };

export class FetchError extends Error {
  constructor(msg, status) {
    super(msg);
    this.status = status;
  }
}

/** Fetch `<DATA_BASE><path>?t=<5-minute bucket>` as JSON. */
export async function fetchJson(path) {
  const t = Math.floor(Date.now() / 300000);
  const url = `${DATA_BASE}${path}${path.includes("?") ? "&" : "?"}t=${t}`;
  let res;
  try {
    res = await fetch(url);
  } catch (e) {
    throw new FetchError(`network error loading ${path}: ${e.message || e}`, 0);
  }
  if (!res.ok) throw new FetchError(`${path}: HTTP ${res.status}${res.status === 404 ? " (not published yet?)" : ""}`, res.status);
  try {
    return await res.json();
  } catch (e) {
    throw new FetchError(`${path}: invalid JSON (${e.message || e})`, res.status);
  }
}

/** Load the given core files (keys of FILES) in parallel; failures land in `state.err`. */
export async function loadCore(keys = Object.keys(FILES)) {
  const res = await Promise.allSettled(keys.map((k) => fetchJson(FILES[k])));
  res.forEach((r, i) => {
    const k = keys[i];
    if (r.status === "fulfilled" && r.value != null) {
      state[k] = r.value;
      delete state.err[k];
    } else if (r.status === "rejected") {
      state.err[k] = r.reason?.message || String(r.reason);
    }
  });
  if (keys.includes("index")) state.byAddr = new Map((state.index || []).map((r) => [r.addr, r]));
  if (keys.includes("markets")) state.marketById = new Map((state.markets || []).map((m) => [m.condition_id, m]));
  if (keys.includes("clusters")) state.clusterById = new Map((state.clusters || []).map((c) => [c.id, c]));
  state.loadedAt = Date.now();
}

/** Lazy, cached wallet file. A failed load is evicted so "retry" refetches. */
export function loadWallet(addr) {
  const a = String(addr).toLowerCase();
  if (!state.wallets.has(a)) {
    const p = fetchJson(`wallets/${a}.json`).catch((e) => {
      state.wallets.delete(a);
      throw e;
    });
    state.wallets.set(a, p);
  }
  return state.wallets.get(a);
}

// ---- links ---------------------------------------------------------------------
export const issueUrl = (n) => `https://github.com/${encodeURIComponent(cfg.owner)}/${encodeURIComponent(cfg.repo)}/issues/${encodeURIComponent(n)}`;
export const profileUrl = (a) => `https://polymarket.com/profile/${encodeURIComponent(a)}`;
export const scanAddrUrl = (a) => `https://polygonscan.com/address/${encodeURIComponent(a)}`;
export const scanTxUrl = (tx) => `https://polygonscan.com/tx/${encodeURIComponent(tx)}`;
export const marketUrl = (slug) => `https://polymarket.com/market/${encodeURIComponent(slug)}`;

// ---- fragments -----------------------------------------------------------------
export const nameOf = (addr, fallback = "") => state.byAddr.get(addr)?.name || fallback || "";

/** Wallet name + short address, linking to the wallet view. */
export function walletCell(addr, name) {
  const n = name ?? nameOf(addr);
  return `<a class="wl" href="#w/${esc(addr)}" title="${esc(addr)}">${n ? `<span class="wn">${esc(n)}</span> ` : ""}<span class="addr">${esc(shortAddr(addr))}</span></a>`;
}

export const tierBadge = (t) => `<span class="tier t-${esc(t)}">${esc(String(t || "low").slice(0, 1).toUpperCase())}<span class="sr"> tier ${esc(t)}</span></span>`;

export function scoreCell(score, tier) {
  const s = Math.max(0, Math.min(100, Number(score) || 0));
  return `<span class="score"><span class="sv">${s}</span><span class="sbar t-${esc(tier)}" aria-hidden="true"><i style="width:${s}%"></i></span>${tierBadge(tier)}</span>`;
}

/** Compact signal chips from `[[kind, strength], …]`. */
export function sigChips(sigs) {
  return (sigs || [])
    .map(([k, s]) => {
      const i = sigInfo(k);
      const st = Math.max(0, Math.min(1, Number(s) || 0));
      return `<span class="chip s${i.c}" style="--o:${(0.35 + 0.65 * st).toFixed(2)}" title="${esc(i.label)} (${esc(k)}) · strength ${st.toFixed(2)}">${esc(i.abbr)}</span>`;
    })
    .join("");
}

export const clusterBadge = (id) => (id ? `<a class="cbadge" href="#c/${esc(id)}" title="cluster ${esc(id)}">⛓&#8239;${esc(id.replace(/^c-/, "").slice(0, 4))}</a>` : "");

export const issueLink = (n) => (n != null ? `<a href="${esc(issueUrl(n))}" target="_blank" rel="noopener" title="GitHub issue #${esc(n)}">#${esc(n)}</a>` : "");

export function errorBox(msg, retry = true) {
  return `<div class="err" role="alert"><strong>⚠</strong> ${esc(msg)}${retry ? ` <button type="button" class="btn" data-action="retry">Retry</button>` : ""}</div>`;
}

export const loading = (what) => `<div class="loading" aria-live="polite">Loading ${esc(what)}…</div>`;

export const ext = (url, text, title = "") => `<a href="${esc(url)}" target="_blank" rel="noopener"${title ? ` title="${esc(title)}"` : ""}>${esc(text)}&#8239;↗</a>`;

export const kpi = (label, value, sub = "") => `<div class="kpi"><div class="kl">${esc(label)}</div><div class="kv">${value}</div>${sub ? `<div class="ks">${sub}</div>` : ""}</div>`;

