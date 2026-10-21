// Wallet view (`#w/<addr>`), also rendered inside the side drawer.
import { esc, usd, price, num, pct, rel, date, frac, pnlClass, sigInfo, linkInfo, shortAddr, cmp, won, kindBadge, nowS } from "./util.js";
import {
  state, loadWallet, walletCell, scoreCell, sigChips, clusterBadge, issueLink, issueUrl, errorBox, loading,
  ext, kpi, profileUrl, scanAddrUrl, scanTxUrl, marketUrl,
} from "./data.js";
import { timeline } from "./charts.js";

const STATUS = { OPEN: "open", CLOSED: "closed", REDEEMABLE: "redeemable", REDEEMABLE_LOST: "lost" };

const POS_COLS = [
  { k: "status", l: "status", v: (p) => p.status },
  { k: "title", l: "market", v: (p) => (p.title || "").toLowerCase() },
  { k: "outcome", l: "outcome", v: (p) => p.outcome },
  { k: "avg", l: "avg → now", v: (p) => p.current_price - p.avg_price, n: 1 },
  { k: "size", l: "size", v: (p) => p.total_size, n: 1 },
  { k: "cost", l: "cost", v: (p) => p.cost, n: 1 },
  { k: "value", l: "value", v: (p) => p.current_value, n: 1 },
  { k: "pnl", l: "PnL", v: (p) => p.total_pnl, n: 1 },
  { k: "entry", l: "entry", v: (p) => p.first_entry_at, n: 1 },
  { k: "end", l: "end", v: (p) => p.end_date, n: 1 },
];

function arrow(avg, cur) {
  const d = cur - avg;
  if (!Number.isFinite(d)) return "";
  const s = d > 0.005 ? "▲" : d < -0.005 ? "▼" : "▶";
  return `<span class="${d > 0.005 ? "pos" : d < -0.005 ? "neg" : "muted"}" aria-label="${d > 0 ? "up" : d < 0 ? "down" : "flat"}">${s}</span>`;
}

function posRows(ps, sort) {
  const c = POS_COLS.find((x) => x.k === sort.k) || POS_COLS[7];
  const rows = [...ps].sort((a, b) => cmp(c.v(a), c.v(b), sort.dir));
  return rows
    .map((p) => {
      const r = won(p);
      const st = r === true ? "✓ won" : r === false ? "✗ lost" : STATUS[p.status] || p.status;
      const mk = state.marketById.has(p.condition_id) ? `#m/${esc(p.condition_id)}` : null;
      const title = esc(p.title || p.condition_id);
      return `<tr>
<td><span class="st st-${esc(String(p.status).toLowerCase())}${r === true ? " pos" : r === false ? " neg" : ""}">${esc(st)}</span></td>
<td class="ttl">${mk ? `<a href="${mk}" title="${title}">${title}</a>` : p.slug ? `<a href="${esc(marketUrl(p.slug))}" target="_blank" rel="noopener" title="${title}">${title}</a>` : `<span title="${title}">${title}</span>`}</td>
<td>${esc(p.outcome)}</td>
<td class="num">${price(p.avg_price)} ${arrow(p.avg_price, p.current_price)} ${price(p.current_price)}</td>
<td class="num">${num(p.total_size, 0)}</td>
<td class="num">${usd(p.cost)}</td>
<td class="num">${usd(p.current_value)}</td>
<td class="num ${pnlClass(p.total_pnl)}">${usd(p.total_pnl, { sign: true })}</td>
<td class="num" title="${esc(date(p.first_entry_at, true))}">${date(p.first_entry_at)}</td>
<td class="num">${date(p.end_date)}</td></tr>`;
    })
    .join("");
}

function tradeRows(ts, n) {
  return ts
    .slice(0, n)
    .map(
      (t) => `<tr class="${t.usdc >= 500 ? "big" : ""}">
<td class="num" title="${esc(date(t.ts, true))}">${esc(rel(t.ts))}</td>
<td>${kindBadge(t.kind)}</td>
<td class="ttl">${t.condition_id && state.marketById.has(t.condition_id) ? `<a href="#m/${esc(t.condition_id)}" title="${esc(t.title)}">${esc(t.title)}</a>` : `<span title="${esc(t.title)}">${esc(t.title || "–")}</span>`}</td>
<td>${esc(t.outcome || "")}</td>
<td class="num">${t.price ? price(t.price) : "–"}</td>
<td class="num">${t.size ? num(t.size, 0) : "–"}</td>
<td class="num">${usd(t.usdc)}</td>
<td>${t.tx ? `<a href="${esc(scanTxUrl(t.tx))}" target="_blank" rel="noopener" title="${esc(t.tx)}" aria-label="transaction on Polygonscan">${esc(t.tx.slice(0, 8))}↗</a>` : ""}</td></tr>`,
    )
    .join("");
}

function clusterSection(w) {
  const c = w.cluster_id && state.clusterById.get(w.cluster_id);
  if (!w.cluster_id) return "";
  if (!c) return `<section><h3>Cluster ${clusterBadge(w.cluster_id)}</h3><p class="muted">cluster ${esc(w.cluster_id)} not found in clusters.json${state.err.clusters ? ` (${esc(state.err.clusters)})` : ""}</p></section>`;
  const links = (c.links || []).filter((l) => l.a === w.addr || l.b === w.addr);
  const others = c.members.filter((a) => a !== w.addr);
  return `<section><h3>Cluster ${clusterBadge(c.id)} <span class="muted">${c.members.length} members</span> <a href="#c/${esc(c.id)}">open cluster view →</a></h3>
<div class="tw"><table class="t"><thead><tr><th>member</th><th class="num">score</th><th>signals</th><th>links to this wallet</th></tr></thead><tbody>
${others
  .map((a) => {
    const r = state.byAddr.get(a);
    const ls = links.filter((l) => l.a === a || l.b === a);
    return `<tr><td>${walletCell(a)}</td><td class="num">${r ? scoreCell(r.score, r.tier) : "–"}</td><td class="chips">${r ? sigChips(r.signals) : ""}</td>
<td>${ls.length ? ls.map((l) => `<span class="lk l${linkInfo(l.kind).c}" title="${esc(l.evidence)}">${esc(linkInfo(l.kind).label)} <span class="num">${Number(l.confidence).toFixed(2)}</span></span>`).join(" ") : `<span class="muted">indirect</span>`}</td></tr>`;
  })
  .join("")}</tbody></table></div></section>`;
}

/** Render wallet `addr` into `el`. `opts.drawer` = compact variant inside the drawer. */
export async function renderWallet(el, addr, opts = {}) {
  const a = String(addr || "").toLowerCase();
  const row = state.byAddr.get(a);
  if (!/^0x[0-9a-f]{40}$/.test(a)) {
    el.innerHTML = errorBox(`"${addr}" is not a wallet address`, false);
    return;
  }
  el.innerHTML = loading(`wallet ${shortAddr(a)}`);
  let w;
  try {
    w = await loadWallet(a);
  } catch (e) {
    el.innerHTML = `<h2>${row ? walletCell(a, row.name) : esc(a)}</h2>
${errorBox(e.status === 404 ? `No wallet file published for ${a} (wallets/${a}.json → 404).` : e.message)}
<p class="links">${ext(profileUrl(a), "Polymarket profile")} ${ext(scanAddrUrl(a), "Polygonscan")}${row?.issue != null ? " " + ext(issueUrl(row.issue), `issue #${row.issue}`) : ""}</p>
${row ? `<p>From index: score ${scoreCell(row.score, row.tier)} ${sigChips(row.signals)}</p>` : ""}`;
    return;
  }
  const now = nowS();
  const p = w.profile || {};
  const st = w.stats || {};
  const name = p.name || p.pseudonym || row?.name || "";
  const joined = st.join_date ?? p.created_at;
  const ageD = joined ? (now - joined) / 86400 : null;
  const R = w.resolved || {};
  const realized = st.realized_pnl ?? (w.positions || []).reduce((s, x) => s + (x.realized_pnl || 0), 0);
  const issue = w.issue ?? row?.issue;
  const sc = w.score || {};
  const trades = [...(w.trades || [])].sort((x, y) => y.ts - x.ts);
  const sort = { k: "pnl", dir: -1 };

  const sigs = (w.signals || [])
    .map((s) => {
      const i = sigInfo(s.kind);
      const refs = (s.refs || [])
        .map((cid) => {
          const pos = (w.positions || []).find((x) => x.condition_id === cid);
          const t = pos?.title || cid.slice(0, 10) + "…";
          return state.marketById.has(cid) ? `<a href="#m/${esc(cid)}">${esc(t)}</a>` : `<span>${esc(t)}</span>`;
        })
        .join(" · ");
      return `<li><span class="chip s${i.c}" style="--o:1">${esc(i.abbr)}</span><span class="sgn">${esc(i.label)}</span>
<span class="sbt" role="meter" aria-valuemin="0" aria-valuemax="1" aria-valuenow="${s.strength}" aria-label="strength"><i class="s${i.c}" style="width:${(Math.max(0, Math.min(1, s.strength)) * 100).toFixed(0)}%"></i></span><span class="num">${Number(s.strength).toFixed(2)}</span>
<div class="ev">${esc(s.evidence)}${refs ? `<div class="refs">${refs}</div>` : ""}</div></li>`;
    })
    .join("");

  el.innerHTML = `<div class="wallet">
<div class="ident"><h2>${name ? `<span class="wn">${esc(name)}</span> ` : ""}<code class="addr" title="${esc(a)}">${esc(opts.drawer ? shortAddr(a) : a)}</code>
<button type="button" class="ibtn" data-copy="${esc(a)}" aria-label="Copy address" title="Copy address">⧉</button></h2>
<div class="links">${ext(profileUrl(a), "Polymarket")} ${ext(scanAddrUrl(a), "Polygonscan")}${issue != null ? " " + ext(issueUrl(issue), `issue #${issue}`) : ""}${p.x_username ? " " + ext(`https://x.com/${encodeURIComponent(p.x_username)}`, "@" + p.x_username) : ""}
${w.cluster_id ? clusterBadge(w.cluster_id) : ""} ${(w.sources || []).map((s) => `<span class="tag">${esc(s)}</span>`).join(" ")}${p.pseudonym && p.name ? ` <span class="muted">aka ${esc(p.pseudonym)}</span>` : ""}${opts.drawer ? ` <a href="#w/${esc(a)}">full view →</a>` : ""}</div></div>
<div class="kpis">
${kpi("score", scoreCell(sc.total ?? row?.score, sc.tier ?? row?.tier), `mat. ×${Number(sc.materiality ?? 1).toFixed(2)}`)}
${kpi("value", usd(w.value))}
${kpi("realized PnL", `<span class="${pnlClass(realized)}">${usd(realized, { sign: true })}</span>`, st.biggest_win ? `best ${usd(st.biggest_win)}` : "")}
${kpi("win rate", R.n ? `${pct(R.wins / R.n)}` : "–", R.n ? `${frac(R.wins)}/${frac(R.n)} · z ${Number(R.z).toFixed(1)}` : "")}
${kpi("resolved", frac(R.n), `exp. wins ${Number(R.expected_wins || 0).toFixed(1)}`)}
${kpi("longshot wins", num(R.longshot_wins))}
${kpi("account age", ageD != null ? `${num(ageD, 0)} d` : "–", joined ? date(joined) : "")}
${kpi("first seen", esc(rel(w.first_seen, now)), `scored ${esc(rel(w.last_scored, now))}`)}
${st.volume_usdc ? kpi("volume", usd(st.volume_usdc), `${num(st.markets_traded)} mkts · ${num(st.trade_count)} trades`) : ""}
</div>
<section><h3>Signals</h3>${sigs ? `<ul class="sigs">${sigs}</ul>` : `<p class="muted">no signals</p>`}</section>
<section><h3>Trade timeline <span class="muted">▲ buy · ▼ sell · ◆ redeem · size ∝ √USDC · y = price · ┊ market end · lane tint = won/lost</span></h3>
<div class="tlw" data-tl></div></section>
<section><h3>Positions <span class="muted">${(w.positions || []).length}</span></h3>
<div class="tw"><table class="t" data-pos><thead><tr>${POS_COLS.map((c) => `<th${c.n ? ' class="num"' : ""} aria-sort="${c.k === sort.k ? (sort.dir > 0 ? "ascending" : "descending") : "none"}"><button type="button" data-k="${c.k}">${esc(c.l)}</button></th>`).join("")}</tr></thead>
<tbody>${posRows(w.positions || [], sort)}</tbody></table></div></section>
<section><h3>Recent trades <span class="muted">${trades.length} tracked, newest first</span></h3>
<div class="tw"><table class="t"><thead><tr><th class="num">when</th><th>kind</th><th>market</th><th>outcome</th><th class="num">price</th><th class="num">shares</th><th class="num">USDC</th><th>tx</th></tr></thead>
<tbody data-trades>${tradeRows(trades, 50)}</tbody></table></div>
${trades.length > 50 ? `<button type="button" class="btn" data-more>Show all ${trades.length}</button>` : ""}${trades.length ? "" : `<p class="muted">no tracked trades yet</p>`}</section>
${clusterSection(w)}
</div>`;

  // timeline: sized to its container, redrawn on resize
  const tl = el.querySelector("[data-tl]");
  const draw = () => {
    const width = Math.floor(tl.clientWidth);
    if (width <= 0 || width === tl._w) return;
    tl._w = width;
    const svg = timeline(w, width, now);
    tl.innerHTML = svg || `<p class="muted">no trades or position entries to plot</p>`;
  };
  draw();
  if ("ResizeObserver" in window) {
    const ro = new ResizeObserver(() => requestAnimationFrame(() => (tl.isConnected ? draw() : ro.disconnect())));
    ro.observe(tl);
  }

  const pt = el.querySelector("[data-pos]");
  pt.querySelector("thead").addEventListener("click", (ev) => {
    const b = ev.target.closest("button[data-k]");
    if (!b) return;
    const k = b.dataset.k;
    sort.dir = sort.k === k ? -sort.dir : POS_COLS.find((c) => c.k === k).n ? -1 : 1;
    sort.k = k;
    pt.querySelectorAll("th").forEach((th) => th.setAttribute("aria-sort", th.querySelector("button").dataset.k === k ? (sort.dir > 0 ? "ascending" : "descending") : "none"));
    pt.querySelector("tbody").innerHTML = posRows(w.positions || [], sort);
  });
  const more = el.querySelector("[data-more]");
  if (more)
    more.addEventListener("click", () => {
      el.querySelector("[data-trades]").innerHTML = tradeRows(trades, trades.length);
      more.remove();
    });
}
