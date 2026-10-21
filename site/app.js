// polywatch dashboard — entry point: header, overview, tape, routing, drawer.
// No build step, no dependencies. Data contracts: crates/types/src/site.rs.
import { esc, usd, num, rel, relShort, date, frac, pnlClass, sigInfo, SIGNALS, store, cmp, debounce, nowS, kindBadge, price, KIND } from "./js/util.js";
import { cfg, state, loadCore, walletCell, scoreCell, sigChips, clusterBadge, issueLink, errorBox, loading, scanTxUrl } from "./js/data.js";
import { histogram, signalBars } from "./js/charts.js";
import { renderWallet } from "./js/wallet.js";
import { renderMarket, renderCluster } from "./js/views.js";

const $ = (s, r = document) => r.querySelector(s);
const view = $("#view");
const REFRESH_MS = 5 * 60 * 1000;

// ---- "new since last visit" ------------------------------------------------------
// The previous visit is pinned per browser session so a reload keeps the markers.
let prevVisit = null;
try {
  const pinned = sessionStorage.getItem("pw.prevVisit");
  if (pinned != null) prevVisit = JSON.parse(pinned);
  else {
    prevVisit = store.get("pw.lastVisit", null);
    sessionStorage.setItem("pw.prevVisit", JSON.stringify(prevVisit));
  }
} catch {
  prevVisit = store.get("pw.lastVisit", null);
}
store.set("pw.lastVisit", nowS());
const isNew = (r) => prevVisit != null && r.first_seen > prevVisit;

// ---- header ----------------------------------------------------------------------
function renderHeader() {
  const m = state.meta;
  const el = $("#status");
  if (!m) {
    el.innerHTML = state.err.meta ? `<span class="warn">⚠ meta.json: ${esc(state.err.meta)}</span>` : `<span class="muted">loading…</span>`;
    return;
  }
  const now = nowS();
  const stale = now - m.last_track > 1800;
  const c = m.counts || {};
  const api = m.api || {};
  const reqs = Object.entries(api.requests || {}).map(([k, v]) => `${k} ${num(v)}`).join(", ");
  const item = (label, val, cls = "", title = "") => `<span class="si ${cls}"${title ? ` title="${esc(title)}"` : ""}><span class="sl">${label}</span> ${val}</span>`;
  el.innerHTML = [
    item("discovery", esc(rel(m.last_discover, now)), "", date(m.last_discover, true)),
    item("tracker", `${esc(rel(m.last_track, now))}${stale ? " ⚠ stale" : ""}`, stale ? "warn" : "", `${date(m.last_track, true)}${stale ? " — more than 30 min old" : ""}`),
    item("API lag", `${num(m.feed_lag_s)} s`, m.feed_lag_s > 600 ? "warn" : ""),
    item("watchlist", num(c.watchlist)),
    item("tiers", `<span class="tier t-high">H</span>${num(c.high)} <span class="tier t-medium">M</span>${num(c.medium)} <span class="muted">· ${num(c.candidates)} cand. · ${num(c.clusters)} clusters</span>`),
    item("API", api.rate_limited || api.errors ? `<span class="warn">⚠ ${num(api.rate_limited)}×429 · ${num(api.errors)} err</span>` : `<span class="pos">✓ ok</span>`, "", `requests: ${reqs}`),
    m.feed_truncated ? item("", "⚠ feed truncated — some trades skipped this run", "warn") : "",
    cfg.sample ? item("", "sample data", "tag") : "",
    Object.keys(state.err).length ? item("", `⚠ ${Object.keys(state.err).length} file(s) failed`, "warn", Object.values(state.err).join("\n")) : "",
  ].join("");
  el.title = `generated ${date(m.generated_at, true)}${m.run_id ? ` · run ${m.run_id}` : ""}`;
}

// ---- overview --------------------------------------------------------------------
const COLS = [
  { k: "new", l: "", t: "new since last visit", v: (r) => (isNew(r) ? 1 : 0), n: 1 },
  { k: "wallet", l: "wallet", v: (r) => (r.name || r.addr).toLowerCase() },
  { k: "score", l: "score", v: (r) => r.score, n: 1 },
  { k: "signals", l: "signals", v: (r) => (r.signals || []).reduce((s, x) => s + x[1], 0), n: 1 },
  { k: "age", l: "age d", v: (r) => r.age_d, n: 1 },
  { k: "pnl", l: "PnL", v: (r) => r.pnl, n: 1 },
  { k: "win", l: "win", t: "wins / resolved", v: (r) => r.win_rate, n: 1 },
  { k: "z", l: "z", t: "win-rate z-score vs. entry prices", v: (r) => r.z, n: 1 },
  { k: "value", l: "value", v: (r) => r.value, n: 1 },
  { k: "mkts", l: "mkts", v: (r) => r.n_markets, n: 1 },
  { k: "last", l: "last", t: "last trade", v: (r) => r.last_trade_ts, n: 1 },
  { k: "cluster", l: "cl", t: "cluster", v: (r) => r.cluster_id },
  { k: "issue", l: "issue", v: (r) => r.issue, n: 1 },
  { k: "watched", l: "w", t: "on tracker watchlist", v: (r) => (r.watched ? 1 : 0), n: 1 },
];
const ov = {
  sort: store.get("pw.sort", { k: "score", dir: -1 }),
  q: "",
  tier: "all",
  kinds: new Set(),
  watched: false,
};
if (!COLS.some((c) => c.k === ov.sort?.k)) ov.sort = { k: "score", dir: -1 };

function filtered() {
  const q = ov.q.trim().toLowerCase();
  return (state.index || []).filter(
    (r) =>
      (!q || r.addr.includes(q) || (r.name || "").toLowerCase().includes(q)) &&
      (ov.tier === "all" || r.tier === ov.tier) &&
      (!ov.watched || r.watched) &&
      (!ov.kinds.size || (r.signals || []).some(([k]) => ov.kinds.has(k))),
  );
}

function candRow(r, now) {
  const wins = r.win_rate != null ? r.win_rate * r.n_resolved : null;
  return `<tr data-addr="${esc(r.addr)}" class="${isNew(r) ? "isnew" : ""}">
<td>${isNew(r) ? `<span class="new" title="first seen ${esc(rel(r.first_seen, now))}">new</span>` : ""}</td>
<td class="wcell">${walletCell(r.addr, r.name)}</td>
<td class="num">${scoreCell(r.score, r.tier)}</td>
<td class="chips">${sigChips(r.signals)}</td>
<td class="num">${r.age_d != null ? num(r.age_d, 0) : "–"}</td>
<td class="num ${pnlClass(r.pnl)}">${usd(r.pnl, { sign: true })}</td>
<td class="num">${wins != null ? `${frac(Math.round(wins * 2) / 2)}/${frac(r.n_resolved)}` : "–"}</td>
<td class="num${r.z >= 2 ? " hi" : ""}">${Number.isFinite(r.z) ? r.z.toFixed(1) : "–"}</td>
<td class="num">${usd(r.value)}</td>
<td class="num">${num(r.n_markets)}</td>
<td class="num" title="${esc(date(r.last_trade_ts, true))}">${esc(relShort(r.last_trade_ts, now))}</td>
<td>${clusterBadge(r.cluster_id)}</td>
<td class="num">${issueLink(r.issue)}</td>
<td class="c">${r.watched ? `<span class="dot-w" title="watched by tracker" aria-label="watched">●</span>` : `<span class="sr">not watched</span>`}</td></tr>`;
}

function renderTable() {
  const tb = $("#cand tbody");
  if (!tb) return;
  const c = COLS.find((x) => x.k === ov.sort.k);
  const rows = filtered().sort((a, b) => cmp(c.v(a), c.v(b), ov.sort.dir) || b.score - a.score);
  const now = nowS();
  tb.innerHTML = rows.length ? rows.map((r) => candRow(r, now)).join("") : `<tr><td colspan="${COLS.length}" class="muted">no candidates match the filters</td></tr>`;
  $("#cand-count").textContent = `${rows.length} of ${(state.index || []).length}`;
  document.querySelectorAll("#cand thead th").forEach((th) => {
    const k = th.dataset.k;
    th.setAttribute("aria-sort", k === ov.sort.k ? (ov.sort.dir > 0 ? "ascending" : "descending") : "none");
  });
  document.querySelectorAll("[data-sig]").forEach((b) => {
    const on = ov.kinds.has(b.dataset.sig);
    b.classList.toggle("on", on);
    b.setAttribute("aria-pressed", String(on));
  });
}

function convergeList() {
  const ms = (state.markets || []).filter((m) => (m.wallets || []).length >= 2);
  if (state.err.markets) return errorBox(`markets.json: ${state.err.markets}`);
  if (!ms.length) return `<p class="muted">none yet</p>`;
  return `<ul class="mlist">${ms
    .slice(0, 12)
    .map((m) => {
      const mx = Math.max(...m.wallets.map((w) => w.score));
      return `<li><a href="#m/${esc(m.condition_id)}" title="${esc(m.title)}">${esc(m.title)}</a><span class="num">${m.wallets.length}<span class="muted"> wallets · max ${mx}</span>${m.resolved_outcome ? ` · <span class="pos">✓${esc(m.resolved_outcome)}</span>` : ""}</span></li>`;
    })
    .join("")}</ul>`;
}

function tapeRows(rows, compact) {
  const now = nowS();
  return rows
    .map(
      (t) => `<tr class="${t.usdc >= 500 ? "big" : ""}">
<td class="num" title="${esc(date(t.ts, true))}">${esc(relShort(t.ts, now))}</td>
<td class="wcell">${walletCell(t.addr, t.name)}</td>
${compact ? "" : `<td class="num">${num(t.score)}</td>`}
<td>${kindBadge(t.kind)}</td>
<td class="ttl">${state.marketById.has(t.condition_id) ? `<a href="#m/${esc(t.condition_id)}" title="${esc(t.title)}">${esc(t.title)}</a>` : `<span title="${esc(t.title)}">${esc(t.title)}</span>`}</td>
${compact ? "" : `<td>${esc(t.outcome)}</td><td class="num">${price(t.price)}</td>`}
<td class="num">${t.usdc >= 500 ? "<b>" : ""}${usd(t.usdc)}${t.usdc >= 500 ? "</b>" : ""}</td>
${compact ? "" : `<td>${t.tx ? `<a href="${esc(scanTxUrl(t.tx))}" target="_blank" rel="noopener" title="${esc(t.tx)}" aria-label="transaction on Polygonscan">${esc(t.tx.slice(0, 8))}↗</a>` : ""}</td>`}</tr>`,
    )
    .join("");
}

const tapeSorted = () => [...(state.tape || [])].sort((a, b) => b.ts - a.ts);

function renderSide() {
  const side = $("#ov-side");
  if (!side) return;
  const m = state.meta;
  const latest = tapeSorted().slice(0, 15);
  side.innerHTML = `
<section class="panel"><h3>Score distribution <span class="muted">all scored wallets, log scale</span></h3>${m ? histogram(m.score_histogram) : errorBox(state.err.meta || "meta.json missing")}</section>
<section class="panel"><h3>Signal frequency <span class="muted">click to filter</span></h3>${m ? signalBars(m.signal_counts, ov.kinds) : ""}</section>
<section class="panel"><h3>Markets with ≥ 2 flagged wallets</h3>${convergeList()}</section>
<section class="panel"><h3>Latest trades <a href="#tape">tape →</a></h3>${state.err.tape && !state.tape ? errorBox(`activity_recent.json: ${state.err.tape}`) : latest.length ? `<div class="tw"><table class="t tape compact"><thead><tr><th class="num">ago</th><th>wallet</th><th>kind</th><th>market</th><th class="num">USDC</th></tr></thead><tbody>${tapeRows(latest, true)}</tbody></table></div>` : `<p class="muted">no trades in the last 48 h</p>`}</section>`;
}

function renderOverview() {
  if (!state.index) {
    view.innerHTML = state.err.index ? errorBox(`index.json: ${state.err.index}`) : loading("candidates");
    return;
  }
  const counts = { all: state.index.length, high: 0, medium: 0, low: 0 };
  for (const r of state.index) counts[r.tier] = (counts[r.tier] || 0) + 1;
  view.innerHTML = `<div class="ov">
<div class="ov-main">
<div class="controls" role="search">
<input id="q" type="search" placeholder="filter address / name" aria-label="Filter by address or name" value="${esc(ov.q)}" autocomplete="off" spellcheck="false">
<div class="seg" role="group" aria-label="Tier">${["all", "high", "medium", "low"].map((t) => `<button type="button" data-tier="${t}" aria-pressed="${ov.tier === t}" class="${ov.tier === t ? "on" : ""}">${t} <span class="muted">${counts[t] || 0}</span></button>`).join("")}</div>
<label class="chk"><input type="checkbox" id="watched" ${ov.watched ? "checked" : ""}> watched only</label>
<span id="cand-count" class="muted num"></span>
</div>
<div class="sigf" role="group" aria-label="Filter by signal kind">${Object.keys(SIGNALS)
    .map((k) => `<button type="button" class="chip s${sigInfo(k).c} tog" data-sig="${k}" aria-pressed="false" title="${esc(sigInfo(k).label)}">${esc(sigInfo(k).abbr)}</button>`)
    .join("")}<button type="button" class="btn sm" data-clear>clear</button></div>
<div class="tw"><table class="t cand" id="cand"><thead><tr>${COLS.map((c) => `<th data-k="${c.k}" class="${c.n ? "num" : ""}"${c.t ? ` title="${esc(c.t)}"` : ""}><button type="button" aria-label="sort by ${esc(c.t || c.l || c.k)}">${esc(c.l)}</button></th>`).join("")}</tr></thead><tbody></tbody></table></div>
<p class="hint muted">Click a row to open the wallet drawer; click the name for the full view. Chips: ${Object.entries(SIGNALS).map(([k, i]) => `${i.abbr} = ${esc(i.label)}`).join(" · ")}.</p>
</div>
<aside class="ov-side" id="ov-side" aria-label="Summary panels"></aside></div>`;
  renderTable();
  renderSide();
}

// overview interactions (delegated; survive re-renders)
view.addEventListener("input", (e) => {
  if (e.target.id === "q") {
    ov.q = e.target.value;
    renderTableDebounced();
  }
  if (e.target.id === "tape-q" || e.target.id === "tape-kind" || e.target.id === "tape-big") renderTapeRows();
});
view.addEventListener("change", (e) => {
  if (e.target.id === "watched") {
    ov.watched = e.target.checked;
    renderTable();
  }
});
const renderTableDebounced = debounce(renderTable, 120);
view.addEventListener("click", (e) => {
  const th = e.target.closest("#cand thead th");
  if (th) {
    const k = th.dataset.k;
    const c = COLS.find((x) => x.k === k);
    ov.sort = { k, dir: ov.sort.k === k ? -ov.sort.dir : c.n ? -1 : 1 };
    store.set("pw.sort", ov.sort);
    renderTable();
    return;
  }
  const tier = e.target.closest("[data-tier]");
  if (tier) {
    ov.tier = tier.dataset.tier;
    view.querySelectorAll("[data-tier]").forEach((b) => {
      b.classList.toggle("on", b === tier);
      b.setAttribute("aria-pressed", String(b === tier));
    });
    renderTable();
    return;
  }
  const sig = e.target.closest("[data-sig]");
  if (sig) {
    const k = sig.dataset.sig;
    ov.kinds.has(k) ? ov.kinds.delete(k) : ov.kinds.add(k);
    renderTable();
    return;
  }
  if (e.target.closest("[data-clear]")) {
    ov.kinds.clear();
    renderTable();
    return;
  }
  const tr = e.target.closest("#cand tbody tr[data-addr]");
  if (tr && !e.target.closest("a,button")) openDrawer(tr.dataset.addr);
});

// ---- tape ------------------------------------------------------------------------
let tapeLimit = 500;
function renderTape() {
  if (!state.tape) {
    view.innerHTML = state.err.tape ? errorBox(`activity_recent.json: ${state.err.tape}`) : loading("tape");
    return;
  }
  const kinds = [...new Set(state.tape.map((t) => t.kind))].sort();
  view.innerHTML = `<h2>Live tape <span class="muted">watchlist trades, last 48 h, newest first · rows ≥ $500 bold</span></h2>
<div class="controls" role="search">
<input id="tape-q" type="search" placeholder="filter wallet / name / market" aria-label="Filter tape by wallet, name or market" autocomplete="off" spellcheck="false">
<select id="tape-kind" aria-label="Trade kind"><option value="">all kinds</option>${kinds.map((k) => `<option value="${esc(k)}">${esc((KIND[k]?.sym || "") + " " + k)}</option>`).join("")}</select>
<label class="chk"><input type="checkbox" id="tape-big"> ≥ $500 only</label>
<span id="tape-count" class="muted num"></span></div>
<div class="tw"><table class="t tape"><thead><tr><th class="num">ago</th><th>wallet</th><th class="num">score</th><th>kind</th><th>market</th><th>outcome</th><th class="num">price</th><th class="num">USDC</th><th>tx</th></tr></thead><tbody id="tape-body"></tbody></table></div>
<div id="tape-more"></div>`;
  renderTapeRows();
}
function renderTapeRows() {
  const body = $("#tape-body");
  if (!body) return;
  const q = ($("#tape-q")?.value || "").trim().toLowerCase();
  const kind = $("#tape-kind")?.value || "";
  const big = $("#tape-big")?.checked;
  const rows = tapeSorted().filter((t) => (!kind || t.kind === kind) && (!big || t.usdc >= 500) && (!q || t.addr.includes(q) || (t.name || "").toLowerCase().includes(q) || (t.title || "").toLowerCase().includes(q)));
  body.innerHTML = rows.length ? tapeRows(rows.slice(0, tapeLimit), false) : `<tr><td colspan="9" class="muted">no trades match</td></tr>`;
  $("#tape-count").textContent = `${rows.length} trades · ${usd(rows.reduce((s, t) => s + (t.usdc || 0), 0))}`;
  $("#tape-more").innerHTML = rows.length > tapeLimit ? `<button type="button" class="btn" data-tape-more>Show all ${rows.length}</button>` : "";
}
view.addEventListener("click", (e) => {
  if (e.target.closest("[data-tape-more]")) {
    tapeLimit = Infinity;
    renderTapeRows();
  }
});

// ---- drawer ----------------------------------------------------------------------
const drawer = $("#drawer");
let drawerReturn = null;
let drawerAddr = null;
function openDrawer(addr) {
  drawerReturn = document.activeElement;
  drawerAddr = addr;
  drawer.hidden = false;
  document.body.classList.add("has-drawer");
  renderWallet($("#drawer-body"), addr, { drawer: true });
  $("#drawer-close").focus();
}
function closeDrawer() {
  if (drawer.hidden) return;
  drawer.hidden = true;
  document.body.classList.remove("has-drawer");
  $("#drawer-body").innerHTML = "";
  if (drawerReturn?.isConnected) drawerReturn.focus();
}
$("#drawer-close").addEventListener("click", closeDrawer);
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    hideTip();
    closeDrawer();
  }
});

// ---- tooltip (all charts) ----------------------------------------------------------
const tip = $("#tip");
let tipPinned = null;
function showTip(target, x, y) {
  tip.innerHTML = target.getAttribute("data-tip");
  tip.hidden = false;
  const r = tip.getBoundingClientRect();
  const vw = document.documentElement.clientWidth;
  let left = x + 12;
  if (left + r.width > vw - 8) left = Math.max(8, x - r.width - 12);
  let top = y + 14;
  if (top + r.height > innerHeight - 8) top = Math.max(8, y - r.height - 10);
  tip.style.left = `${left}px`;
  tip.style.top = `${top}px`;
}
function hideTip() {
  tip.hidden = true;
  tipPinned = null;
}
document.addEventListener("pointermove", (e) => {
  if (e.pointerType !== "mouse") return;
  const t = e.target.closest?.("[data-tip]");
  if (t) showTip(t, e.clientX, e.clientY);
  else if (!tipPinned) tip.hidden = true;
});
document.addEventListener("click", (e) => {
  const t = e.target.closest?.("[data-tip]");
  if (t && !e.target.closest("a")) {
    tipPinned = t;
    showTip(t, e.clientX, e.clientY);
  } else if (!e.target.closest("#tip")) hideTip();
});
addEventListener("scroll", () => !tipPinned && (tip.hidden = true), { passive: true });

// ---- global actions ---------------------------------------------------------------
document.addEventListener("click", async (e) => {
  const cp = e.target.closest("[data-copy]");
  if (cp) {
    try {
      await navigator.clipboard.writeText(cp.dataset.copy);
      cp.textContent = "✓";
      setTimeout(() => (cp.textContent = "⧉"), 1200);
    } catch {
      prompt("Copy address", cp.dataset.copy);
    }
  }
  if (e.target.closest("[data-action=retry]")) {
    const inDrawer = e.target.closest("#drawer");
    await loadCore(Object.keys(state.err).length ? undefined : ["meta", "index"]);
    renderHeader();
    if (inDrawer) {
      if (drawerAddr) renderWallet($("#drawer-body"), drawerAddr, { drawer: true });
    } else route();
  }
});

// ---- routing -----------------------------------------------------------------------
function parseHash() {
  const h = decodeURIComponent(location.hash.replace(/^#\/?/, ""));
  const [v, ...rest] = h.split("/");
  return { v: v || "", id: rest.join("/") };
}
function setNav(v) {
  document.querySelectorAll("#nav a").forEach((a) => a.setAttribute("aria-current", a.dataset.v === v ? "page" : "false"));
}
function route() {
  const { v, id } = parseHash();
  hideTip();
  setNav(v === "tape" ? "tape" : v === "" ? "" : "x");
  if (v !== "") closeDrawer();
  document.title = "polywatch";
  if (v === "w") {
    document.title = `polywatch · ${id.slice(0, 10)}`;
    renderWallet(view, id);
  } else if (v === "m") renderMarket(view, id);
  else if (v === "c") renderCluster(view, id);
  else if (v === "tape") renderTape();
  else renderOverview();
  if (v !== "") scrollTo(0, 0);
}
addEventListener("hashchange", route);

// ---- refresh ----------------------------------------------------------------------
async function refresh() {
  await loadCore(["meta", "index", "tape"]);
  renderHeader();
  const { v } = parseHash();
  if (v === "") {
    if (!$("#cand")) renderOverview();
    else {
      renderTable();
      renderSide();
    }
  } else if (v === "tape") renderTapeRows();
}
setInterval(() => !document.hidden && refresh(), REFRESH_MS);
document.addEventListener("visibilitychange", () => {
  if (!document.hidden && Date.now() - state.loadedAt > REFRESH_MS) refresh();
});
setInterval(renderHeader, 30000);

document.querySelector(".skip")?.addEventListener("click", (e) => {
  e.preventDefault();
  view.focus();
});

// ---- boot ---------------------------------------------------------------------------
(async function boot() {
  view.innerHTML = loading("data");
  await loadCore();
  renderHeader();
  route();
  $("#src").innerHTML = `data: <a href="${esc(cfg.base)}">${esc(cfg.sample ? "sample-data" : cfg.base.replace(/^https?:\/\//, ""))}</a> · <a href="https://github.com/${esc(cfg.owner)}/${esc(cfg.repo)}">${esc(cfg.owner)}/${esc(cfg.repo)}</a>`;
})().catch((e) => {
  view.innerHTML = errorBox(`Unexpected error: ${e.message || e}`);
});
