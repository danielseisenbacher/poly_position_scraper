// Market (`#m/<condition_id>`) and cluster (`#c/<id>`) views.
import { esc, usd, price, num, date, rel, shortAddr, linkInfo, LINKS, pnlClass } from "./util.js";
import { state, walletCell, scoreCell, sigChips, errorBox, ext, kpi, marketUrl } from "./data.js";
import { entryPlot, clusterGraph } from "./charts.js";

function sized(el, sel, draw) {
  const box = el.querySelector(sel);
  const go = () => {
    const w = Math.floor(box.clientWidth);
    if (w > 0 && w !== box._w) {
      box._w = w;
      box.innerHTML = draw(w) || `<p class="muted">nothing to plot</p>`;
    }
  };
  go();
  if ("ResizeObserver" in window) {
    const ro = new ResizeObserver(() => requestAnimationFrame(() => (box.isConnected ? go() : ro.disconnect())));
    ro.observe(box);
  }
}

export function renderMarket(el, id) {
  if (state.err.markets && !state.markets) {
    el.innerHTML = errorBox(`markets.json: ${state.err.markets}`);
    return;
  }
  const m = state.marketById.get(id);
  if (!m) {
    el.innerHTML = `<h2>Market</h2>${errorBox(`Market ${id} is not in markets.json — only markets with ≥ 2 flagged wallets are published.`, false)}<p><a href="#">← back to overview</a></p>`;
    return;
  }
  const ws = [...(m.wallets || [])].sort((a, b) => b.score - a.score);
  const outs = [...new Set(ws.map((w) => w.outcome))];
  const byOut = outs.map((o) => {
    const g = ws.filter((w) => w.outcome === o);
    const cost = g.reduce((s, w) => s + w.avg_price * w.size, 0);
    const sh = g.reduce((s, w) => s + w.size, 0);
    return { o, n: g.length, cost, avg: sh ? cost / sh : null };
  });
  const now = Date.now() / 1000;
  el.innerHTML = `<div class="market">
<h2>${esc(m.title)}</h2>
<div class="links">${m.slug ? ext(marketUrl(m.slug), "Polymarket") : ""} <code class="addr" title="${esc(m.condition_id)}">${esc(shortAddr(m.condition_id))}</code></div>
<div class="kpis">
${kpi("end date", date(m.end_date), m.end_date ? esc(rel(m.end_date, now)) : "")}
${kpi("volume", usd(m.volume))}
${kpi("resolved", m.resolved_outcome ? `<span class="pos">✓ ${esc(m.resolved_outcome)}</span>` : `<span class="muted">open</span>`)}
${kpi("flagged wallets", num(ws.length))}
${byOut.map((b) => kpi(`on ${b.o}`, `${b.n} · ${usd(b.cost)}`, b.avg != null ? `avg entry ${price(b.avg)}` : "")).join("")}
</div>
<section><h3>Entries <span class="muted">x = avg entry price · area ∝ cost basis · colour = tier (H/M/L in table) · ┊ current price</span></h3><div class="tlw" data-plot></div></section>
<section><h3>Flagged wallets in this market</h3>
<div class="tw"><table class="t"><thead><tr><th>wallet</th><th class="num">score</th><th>signals</th><th>outcome</th><th class="num">avg</th><th class="num">now</th><th class="num">size</th><th class="num">cost</th><th class="num">mark-to-market</th></tr></thead><tbody>
${ws
  .map((w) => {
    const r = state.byAddr.get(w.addr);
    const cost = w.avg_price * w.size;
    const mtm = (w.current_price - w.avg_price) * w.size;
    return `<tr><td>${walletCell(w.addr)}</td><td class="num">${scoreCell(w.score, r?.tier || (w.score >= 70 ? "high" : w.score >= 45 ? "medium" : "low"))}</td><td class="chips">${r ? sigChips(r.signals) : ""}</td>
<td>${esc(w.outcome)}${m.resolved_outcome ? (w.outcome === m.resolved_outcome ? ' <span class="pos">✓</span>' : ' <span class="neg">✗</span>') : ""}</td>
<td class="num">${price(w.avg_price)}</td><td class="num">${price(w.current_price)}</td><td class="num">${num(w.size, 0)}</td><td class="num">${usd(cost)}</td><td class="num ${pnlClass(mtm)}">${usd(mtm, { sign: true })}</td></tr>`;
  })
  .join("")}
</tbody></table></div></section></div>`;
  sized(el, "[data-plot]", (w) => entryPlot(m, w, (a) => state.byAddr.get(a)?.name));
}

export function renderCluster(el, id) {
  if (state.err.clusters && !state.clusters) {
    el.innerHTML = errorBox(`clusters.json: ${state.err.clusters}`);
    return;
  }
  const c = state.clusterById.get(id);
  if (!c) {
    el.innerHTML = `<h2>Cluster</h2>${errorBox(`Cluster ${id} not found in clusters.json.`, false)}<p><a href="#">← back to overview</a></p>`;
    return;
  }
  const rows = c.members.map((a) => ({ a, r: state.byAddr.get(a) }));
  rows.sort((x, y) => (y.r?.score ?? -1) - (x.r?.score ?? -1));
  const kinds = [...new Set((c.links || []).map((l) => l.kind))];
  const total = rows.reduce((s, x) => s + (x.r?.value || 0), 0);
  el.innerHTML = `<div class="cluster">
<h2>Cluster <code>${esc(c.id)}</code> <span class="muted">${c.members.length} members · ${(c.links || []).length} links</span></h2>
<div class="kpis">${kpi("members", num(c.members.length))}${kpi("flagged", num(rows.filter((x) => x.r).length))}${kpi("max score", num(Math.max(0, ...rows.map((x) => x.r?.score ?? 0))))}${kpi("combined value", usd(total))}</div>
<div class="cgrid">
<section><h3>Graph <span class="muted">edge width = confidence</span></h3><div class="tlw" data-graph></div>
<ul class="legend">${kinds.map((k) => `<li><span class="lk l${linkInfo(k).c}">${esc(linkInfo(k).abbr)}</span> ${esc(linkInfo(k).label)}</li>`).join("")}</ul></section>
<section><h3>Members</h3><div class="tw"><table class="t"><thead><tr><th>wallet</th><th class="num">score</th><th>signals</th><th class="num">value</th><th class="num">PnL</th><th class="num">age d</th></tr></thead><tbody>
${rows
  .map(({ a, r }) => `<tr><td>${walletCell(a)}</td><td class="num">${r ? scoreCell(r.score, r.tier) : '<span class="muted">not flagged</span>'}</td><td class="chips">${r ? sigChips(r.signals) : ""}</td>
<td class="num">${r ? usd(r.value) : "–"}</td><td class="num ${pnlClass(r?.pnl)}">${r ? usd(r.pnl, { sign: true }) : "–"}</td><td class="num">${r?.age_d != null ? num(r.age_d, 0) : "–"}</td></tr>`)
  .join("")}</tbody></table></div></section></div>
<section><h3>Links</h3><div class="tw"><table class="t"><thead><tr><th>kind</th><th class="num">conf.</th><th>a</th><th>b</th><th>evidence</th></tr></thead><tbody>
${(c.links || [])
  .map((l) => `<tr><td><span class="lk l${linkInfo(l.kind).c}">${esc(linkInfo(l.kind).abbr)}</span> ${esc(LINKS[l.kind]?.label || l.kind)}</td><td class="num">${Number(l.confidence).toFixed(2)}</td><td>${walletCell(l.a)}</td><td>${walletCell(l.b)}</td><td class="ev">${esc(l.evidence)}</td></tr>`)
  .join("")}</tbody></table></div></section></div>`;
  sized(el, "[data-graph]", (w) => clusterGraph(c, w, (a) => state.byAddr.get(a)));
}
