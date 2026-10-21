// Hand-drawn inline SVG charts. Every mark that has a tooltip carries `data-tip`
// (already-escaped HTML, escaped once more for the attribute); app.js shows it.
import { esc, usd, price, date, num, won, sigInfo, linkInfo, shortAddr, kindInfo } from "./util.js";

const tipAttr = (html) => ` data-tip="${esc(html)}" tabindex="-1"`;
const f1 = (n) => (Math.round(n * 10) / 10).toString();

// ---- small panels --------------------------------------------------------------

/** Score histogram (10 buckets of 10) on a log scale, with tier thresholds. */
export function histogram(h) {
  if (!Array.isArray(h) || !h.length) return `<p class="muted">no histogram</p>`;
  const W = 300, H = 112, L = 4, B = 16, T = 12;
  const bw = (W - L * 2) / h.length;
  const max = Math.log10(Math.max(...h, 1) + 1);
  const y = (v) => H - B - (Math.log10(v + 1) / max) * (H - B - T);
  const bars = h
    .map((v, i) => {
      const x = L + i * bw + 1;
      const yy = y(v);
      const cls = i >= 7 ? "t-high" : i >= 5 ? "t-medium" : i === 4 ? "t-mixed" : "t-low";
      return `<g${tipAttr(`score ${i * 10}–${i === 9 ? 100 : i * 10 + 9}: <b>${num(v)}</b> wallets`)}>
<rect class="hit" x="${f1(x - 1)}" y="${T}" width="${f1(bw)}" height="${H - B - T}"/>
<rect class="hb ${cls}" x="${f1(x)}" y="${f1(yy)}" width="${f1(bw - 2)}" height="${f1(Math.max(0, H - B - yy))}" rx="1.5"/>
<text class="vl" x="${f1(x + (bw - 2) / 2)}" y="${f1(yy - 2)}" text-anchor="middle">${v ? num(v) : ""}</text>
<text class="ax" x="${f1(x + (bw - 2) / 2)}" y="${H - 4}" text-anchor="middle">${i * 10}</text></g>`;
    })
    .join("");
  const thr = (s, lbl) => {
    const x = L + s * bw;
    return `<line class="thr" x1="${f1(x)}" x2="${f1(x)}" y1="${T - 4}" y2="${H - B}"/><text class="ax" x="${f1(x + 2)}" y="${T - 4}">${lbl}</text>`;
  };
  return `<svg class="chart" viewBox="0 0 ${W} ${H}" role="img" aria-label="Score histogram, log scale: ${h.map((v, i) => `${i * 10}s ${v}`).join(", ")}">
<line class="base" x1="${L}" x2="${W - L}" y1="${H - B}" y2="${H - B}"/>${bars}${thr(4.5, "M≥45")}${thr(7, "H≥70")}</svg>`;
}

/** Horizontal frequency bars for meta.signal_counts. */
export function signalBars(counts, activeKinds = new Set()) {
  const rows = Object.entries(counts || {}).sort((a, b) => b[1] - a[1]);
  if (!rows.length) return `<p class="muted">no signals</p>`;
  const max = Math.max(...rows.map((r) => r[1]), 1);
  return `<ul class="sbars">${rows
    .map(([k, v]) => {
      const i = sigInfo(k);
      return `<li><button type="button" class="sbrow${activeKinds.has(k) ? " on" : ""}" data-sig="${esc(k)}" aria-pressed="${activeKinds.has(k)}" title="${esc(i.label)}: ${v} candidates — click to filter">
<span class="chip s${i.c}">${esc(i.abbr)}</span><span class="sbl">${esc(i.label)}</span><span class="sbt"><i class="s${i.c}" style="width:${((v / max) * 100).toFixed(1)}%"></i></span><span class="num">${num(v)}</span></button></li>`;
    })
    .join("")}</ul>`;
}

// ---- time axis helpers -----------------------------------------------------------
const H1 = 3600, D1 = 86400;
const STEPS = [H1, 3 * H1, 6 * H1, 12 * H1, D1, 2 * D1, 7 * D1, 14 * D1, 30 * D1, 91 * D1, 182 * D1, 365 * D1];
const MON = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
function timeTicks(t0, t1, maxTicks) {
  const span = t1 - t0;
  const step = STEPS.find((s) => span / s <= maxTicks) || 365 * D1;
  const out = [];
  for (let t = Math.ceil(t0 / step) * step; t <= t1; t += step) out.push(t);
  const fmt = (t) => {
    const d = new Date(t * 1000);
    if (step < D1) return `${String(d.getUTCHours()).padStart(2, "0")}:00`;
    if (step >= 91 * D1) return `${MON[d.getUTCMonth()]} ${String(d.getUTCFullYear()).slice(2)}`;
    return `${MON[d.getUTCMonth()]} ${d.getUTCDate()}`;
  };
  return out.map((t) => [t, fmt(t)]);
}

function tri(x, y, r, up) {
  const a = r * 0.95, b = r * 0.8;
  return up
    ? `M${f1(x)},${f1(y - r)}L${f1(x + a)},${f1(y + b)}L${f1(x - a)},${f1(y + b)}Z`
    : `M${f1(x)},${f1(y + r)}L${f1(x + a)},${f1(y - b)}L${f1(x - a)},${f1(y - b)}Z`;
}
const diamond = (x, y, r) => `M${f1(x)},${f1(y - r)}L${f1(x + r)},${f1(y)}L${f1(x)},${f1(y + r)}L${f1(x - r)},${f1(y)}Z`;
const trunc = (s, n) => (s.length > n ? s.slice(0, n - 1) + "…" : s);

// ---- wallet trade timeline -----------------------------------------------------

/**
 * Trade timeline for one wallet: a cumulative USDC in/out sparkline over one lane
 * per market (top 8 by volume + "other"). Returns SVG markup sized to `width`.
 */
export function timeline(w, width, now) {
  const TK = new Set(["buy", "sell", "redeem"]);
  const trades = (w.trades || []).filter((t) => t.condition_id && TK.has(t.kind) && Number.isFinite(t.ts));
  const positions = w.positions || [];
  const posBy = new Map();
  for (const p of positions) {
    const cur = posBy.get(p.condition_id);
    if (!cur || (p.cost || 0) > (cur.cost || 0)) posBy.set(p.condition_id, p);
  }
  const vol = new Map();
  const titles = new Map();
  for (const t of trades) {
    vol.set(t.condition_id, (vol.get(t.condition_id) || 0) + (t.usdc || 0));
    titles.set(t.condition_id, t.title);
  }
  for (const p of positions) {
    if (!vol.has(p.condition_id) && p.first_entry_at) vol.set(p.condition_id, p.cost || 0);
    if (!titles.has(p.condition_id)) titles.set(p.condition_id, p.title);
  }
  if (!vol.size) return null;
  const ranked = [...vol.entries()].sort((a, b) => b[1] - a[1]).map((e) => e[0]);
  const top = ranked.slice(0, 8);
  const rest = new Set(ranked.slice(8));
  const lanes = top.map((cid) => ({ cid, title: titles.get(cid) || cid, pos: posBy.get(cid) }));
  if (rest.size) lanes.push({ cid: "__other", title: `other (${rest.size} markets)`, pos: null });
  const laneOf = (cid) => (rest.has(cid) ? lanes.length - 1 : top.indexOf(cid));

  // time span: trades + position entries, plus market ends that already happened
  const ts = trades.map((t) => t.ts);
  for (const cid of ranked) {
    const p = posBy.get(cid);
    if (p?.first_entry_at) ts.push(p.first_entry_at);
  }
  let t0 = Math.min(...ts), t1 = Math.max(...ts);
  for (const l of lanes) if (l.pos?.end_date && l.pos.end_date <= now && l.pos.end_date > t0) t1 = Math.max(t1, l.pos.end_date);
  if (t1 - t0 < H1) { t0 -= H1; t1 += H1; }
  const padT = (t1 - t0) * 0.02;
  t0 -= padT; t1 += padT;

  const narrow = width < 560;
  const LW = narrow ? 92 : 190, R = 30, SPK = 46, GAP = 10, LH = narrow ? 30 : 34, AX = 18;
  const top0 = SPK + GAP;
  const H = top0 + lanes.length * LH + AX;
  const pw = Math.max(80, width - LW - R);
  const x = (t) => LW + ((t - t0) / (t1 - t0)) * pw;
  const yIn = (i, p) => top0 + i * LH + 4 + (1 - Math.max(0, Math.min(1, p))) * (LH - 8);
  // r ∝ sqrt(usdc), scaled to the 90th percentile so one huge redeem doesn't shrink the rest
  const us = trades.map((t) => t.usdc || 0).sort((a, b) => a - b);
  const refU = Math.max(us[Math.floor(us.length * 0.9)] || 0, 1);
  const rMax = narrow ? 8 : 11;
  const rad = (u) => Math.min(rMax, 2.5 + Math.sqrt(Math.max(0, u) / refU) * (rMax - 4));

  let s = "";
  // lanes
  lanes.forEach((l, i) => {
    const y0 = top0 + i * LH;
    const r = l.pos ? won(l.pos) : null;
    const cls = r === true ? "lane-won" : r === false ? "lane-lost" : "lane";
    const res = r === true ? "✓ won" : r === false ? "✗ lost" : l.pos ? (l.pos.status === "OPEN" ? "open" : l.pos.status.toLowerCase()) : "";
    const label = trunc(l.title, narrow ? 13 : 28);
    const lt = l.pos
      ? `<b>${esc(l.title)}</b><br>${esc(l.pos.outcome)} · ${esc(res)} · avg ${price(l.pos.avg_price)} → ${price(l.pos.current_price)}<br>cost ${usd(l.pos.cost)} · PnL ${usd(l.pos.total_pnl, { sign: true })}`
      : `<b>${esc(l.title)}</b>`;
    s += `<rect class="${cls}" x="${LW}" y="${y0 + 1}" width="${f1(pw)}" height="${LH - 2}"/>`;
    s += `<line class="mid" x1="${LW}" x2="${f1(LW + pw)}" y1="${f1(y0 + LH / 2)}" y2="${f1(y0 + LH / 2)}"/>`;
    const href = l.cid !== "__other" ? `#m/${esc(l.cid)}` : null;
    s += `<g class="ll"${tipAttr(lt)}>${href ? `<a href="${href}">` : ""}<text class="lbl" x="${LW - 6}" y="${f1(y0 + LH / 2 - (narrow ? 0 : 2))}" text-anchor="end">${esc(label)}</text>${href ? "</a>" : ""}`;
    if (!narrow && res) s += `<text class="lbl2 ${r === true ? "pos" : r === false ? "neg" : ""}" x="${LW - 6}" y="${f1(y0 + LH / 2 + 10)}" text-anchor="end">${esc(l.pos.outcome)} · ${esc(res)}</text>`;
    s += `</g>`;
    if (l.pos?.end_date && l.pos.end_date >= t0 && l.pos.end_date <= t1) {
      const xe = x(l.pos.end_date);
      s += `<g${tipAttr(`market end ${date(l.pos.end_date)}`)}><rect class="hit" x="${f1(xe - 3)}" y="${y0}" width="6" height="${LH}"/><line class="endt" x1="${f1(xe)}" x2="${f1(xe)}" y1="${y0 + 1}" y2="${y0 + LH - 1}"/></g>`;
    }
  });
  // price labels on the right of the first lane
  s += `<text class="ax" x="${f1(LW + pw + 4)}" y="${top0 + 8}">p=1</text><text class="ax" x="${f1(LW + pw + 4)}" y="${top0 + LH - 3}">p=0</text>`;

  // entries without recorded trades: hollow ring at avg price
  for (const [cid, p] of posBy) {
    if (!p.first_entry_at || trades.some((t) => t.condition_id === cid)) continue;
    const i = laneOf(cid);
    if (i < 0) continue;
    s += `<g${tipAttr(`<b>first entry</b> ${date(p.first_entry_at, true)}<br>${esc(p.title)}<br>${esc(p.outcome)} avg ${price(p.avg_price)} · cost ${usd(p.cost)}<br><span class="muted">no tracked trades</span>`)}><circle class="entry" cx="${f1(x(p.first_entry_at))}" cy="${f1(yIn(i, p.avg_price))}" r="4"/></g>`;
  }
  // trades, oldest first so newer ones draw on top
  const asc = [...trades].sort((a, b) => a.ts - b.ts);
  for (const t of asc) {
    const i = laneOf(t.condition_id);
    if (i < 0) continue;
    const cx = x(t.ts), cy = yIn(i, t.price), r = rad(t.usdc);
    const k = kindInfo(t.kind);
    const shape = t.kind === "redeem" ? diamond(cx, cy, r) : tri(cx, cy, r, t.kind === "buy");
    const tip = `<b>${k.sym} ${esc(t.kind)}</b> ${date(t.ts, true)}<br>${esc(t.title)}<br>${esc(t.outcome)} @ ${price(t.price)} · ${num(t.size, 0)} sh · <b>${usd(t.usdc)}</b>`;
    s += `<g${tipAttr(tip)}><circle class="hit" cx="${f1(cx)}" cy="${f1(cy)}" r="${f1(r + 4)}"/><path class="mk ${k.cls}" d="${shape}"/></g>`;
  }

  // cumulative USDC in/out sparkline (one y scale: both are USDC)
  let cin = 0, cout = 0;
  const pin = [[t0, 0]], pout = [[t0, 0]];
  for (const t of asc) {
    if (t.kind === "buy") { cin += t.usdc || 0; pin.push([t.ts, cin]); }
    else { cout += t.usdc || 0; pout.push([t.ts, cout]); }
  }
  pin.push([t1, cin]); pout.push([t1, cout]);
  const ym = Math.max(cin, cout, 1);
  const ys = (v) => SPK - 3 - (v / ym) * (SPK - 8);
  const step = (pts) => pts.map(([t, v], j) => (j ? `H${f1(x(t))}V${f1(ys(v))}` : `M${f1(x(t))},${f1(ys(v))}`)).join("");
  s += `<line class="base" x1="${LW}" x2="${f1(LW + pw)}" y1="${SPK - 3}" y2="${SPK - 3}"/>`;
  s += `<path class="spk in" d="${step(pin)}"/><path class="spk out" d="${step(pout)}"/>`;
  s += `<text class="ax" x="${LW - 6}" y="10" text-anchor="end">cumulative USDC</text>`;
  s += `<text class="dl" x="${LW - 6}" y="24" text-anchor="end">in ${usd(cin)} <tspan class="sw in">━━</tspan></text>`;
  s += `<text class="dl" x="${LW - 6}" y="37" text-anchor="end">out ${usd(cout)} <tspan class="sw out">╍╍</tspan></text>`;

  // time axis
  const yA = top0 + lanes.length * LH;
  s += `<line class="base" x1="${LW}" x2="${f1(LW + pw)}" y1="${yA}" y2="${yA}"/>`;
  for (const [t, lbl] of timeTicks(t0, t1, Math.max(3, Math.floor(pw / 64)))) {
    s += `<line class="grid" x1="${f1(x(t))}" x2="${f1(x(t))}" y1="${top0}" y2="${yA}"/><text class="ax" x="${f1(x(t))}" y="${yA + 13}" text-anchor="middle">${lbl}</text>`;
  }
  if (now >= t0 && now <= t1) s += `<line class="nowl" x1="${f1(x(now))}" x2="${f1(x(now))}" y1="0" y2="${yA}"/>`;

  const aria = `Trade timeline: ${trades.length} trades across ${vol.size} markets from ${date(t0)} to ${date(t1)}. Buys ▲, sells ▼, redeems ◆; y within each lane is price 0–1.`;
  return `<svg class="chart tl" width="${width}" height="${H}" viewBox="0 0 ${width} ${H}" role="img" aria-label="${esc(aria)}">${s}</svg>`;
}

// ---- market entry dot plot -----------------------------------------------------

/** x = avg entry price, one lane per outcome, r ∝ sqrt(cost basis). */
export function entryPlot(m, width, nameOf) {
  const ws = m.wallets || [];
  if (!ws.length) return null;
  const outs = [...new Set(ws.map((w) => w.outcome))];
  const L = 56, R = 16, T = 8, LH = 64, AX = 20;
  const pw = Math.max(120, width - L - R);
  const H = T + outs.length * LH + AX;
  const x = (p) => L + Math.max(0, Math.min(1, p)) * pw;
  const maxC = Math.max(...ws.map((w) => w.avg_price * w.size), 1);
  let s = "";
  outs.forEach((o, i) => {
    const y0 = T + i * LH;
    const win = m.resolved_outcome ? (o === m.resolved_outcome ? "won" : "lost") : "";
    s += `<rect class="${win === "won" ? "lane-won" : win === "lost" ? "lane-lost" : "lane"}" x="${L}" y="${y0 + 1}" width="${f1(pw)}" height="${LH - 2}"/>`;
    s += `<text class="lbl" x="${L - 6}" y="${y0 + LH / 2}" text-anchor="end">${esc(trunc(o, 8))}</text>`;
    if (win) s += `<text class="lbl2 ${win === "won" ? "pos" : "neg"}" x="${L - 6}" y="${y0 + LH / 2 + 11}" text-anchor="end">${win === "won" ? "✓ won" : "✗ lost"}</text>`;
    const cur = ws.find((w) => w.outcome === o)?.current_price;
    if (Number.isFinite(cur)) {
      s += `<g${tipAttr(`current price of ${esc(o)}: ${price(cur)}`)}><rect class="hit" x="${f1(x(cur) - 3)}" y="${y0}" width="6" height="${LH}"/><line class="endt" x1="${f1(x(cur))}" x2="${f1(x(cur))}" y1="${y0 + 1}" y2="${y0 + LH - 1}"/></g>`;
    }
  });
  ws.forEach((w, j) => {
    const i = outs.indexOf(w.outcome);
    const cost = w.avg_price * w.size;
    const r = 3 + Math.sqrt(cost / maxC) * 11;
    const jit = ((j * 0.618) % 1) - 0.5;
    const cy = T + i * LH + LH / 2 + jit * (LH - 2 * Math.min(r, 14) - 6);
    const tier = w.score >= 70 ? "high" : w.score >= 45 ? "medium" : "low";
    const nm = nameOf(w.addr) || shortAddr(w.addr);
    const tip = `<b>${esc(nm)}</b> · score ${w.score}<br>${esc(w.outcome)} avg ${price(w.avg_price)} → ${price(w.current_price)}<br>${num(w.size, 0)} sh · cost ${usd(cost)}`;
    s += `<g${tipAttr(tip)}><a href="#w/${esc(w.addr)}"><circle class="hit" cx="${f1(x(w.avg_price))}" cy="${f1(cy)}" r="${f1(r + 3)}"/><circle class="dot t-${tier}" cx="${f1(x(w.avg_price))}" cy="${f1(cy)}" r="${f1(r)}"/></a></g>`;
  });
  const yA = T + outs.length * LH;
  s += `<line class="base" x1="${L}" x2="${L + pw}" y1="${yA}" y2="${yA}"/>`;
  for (let p = 0; p <= 1.0001; p += 0.1) {
    s += `<line class="grid" x1="${f1(x(p))}" x2="${f1(x(p))}" y1="${T}" y2="${yA}"/><text class="ax" x="${f1(x(p))}" y="${yA + 13}" text-anchor="middle">${p.toFixed(1)}</text>`;
  }
  return `<svg class="chart" width="${width}" height="${H}" viewBox="0 0 ${width} ${H}" role="img" aria-label="${esc(`Entry prices of ${ws.length} flagged wallets in ${m.title}; x is average entry price, circle area is cost basis.`)}">${s}</svg>`;
}

// ---- cluster node-link -----------------------------------------------------------

/** Circle-layout node-link diagram; edges coloured/labelled by LinkKind, width by confidence. */
export function clusterGraph(c, width, rowOf) {
  const mem = c.members || [];
  const n = mem.length;
  if (!n) return null;
  const W = Math.min(width, 640), H = Math.min(360, Math.max(220, W * 0.6));
  const cx = W / 2, cy = H / 2, rad = n <= 1 ? 0 : Math.min(W, H) / 2 - 46;
  const pos = new Map(mem.map((a, i) => {
    const ang = -Math.PI / 2 + (i / n) * 2 * Math.PI;
    return [a, [cx + rad * Math.cos(ang), cy + rad * Math.sin(ang)]];
  }));
  const pairCount = new Map();
  let s = "";
  for (const l of c.links || []) {
    const pa = pos.get(l.a), pb = pos.get(l.b);
    if (!pa || !pb) continue;
    const key = [l.a, l.b].sort().join("|");
    const k = pairCount.get(key) || 0;
    pairCount.set(key, k + 1);
    const off = (k % 2 ? -1 : 1) * Math.ceil(k / 2) * 26;
    const mx = (pa[0] + pb[0]) / 2, my = (pa[1] + pb[1]) / 2;
    const dx = pb[0] - pa[0], dy = pb[1] - pa[1], len = Math.hypot(dx, dy) || 1;
    const qx = mx - (dy / len) * off * 2, qy = my + (dx / len) * off * 2;
    const lx = mx - (dy / len) * off, ly = my + (dx / len) * off;
    const li = linkInfo(l.kind);
    const tip = `<b>${esc(li.label)}</b> · confidence ${Number(l.confidence).toFixed(2)}<br>${esc(shortAddr(l.a))} ↔ ${esc(shortAddr(l.b))}<br>${esc(l.evidence)}`;
    s += `<g${tipAttr(tip)}><path class="hitl" d="M${f1(pa[0])},${f1(pa[1])}Q${f1(qx)},${f1(qy)} ${f1(pb[0])},${f1(pb[1])}"/>
<path class="edge l${li.c}" style="stroke-width:${f1(1 + 4 * (Number(l.confidence) || 0))}" d="M${f1(pa[0])},${f1(pa[1])}Q${f1(qx)},${f1(qy)} ${f1(pb[0])},${f1(pb[1])}"/>
<rect class="elbg" x="${f1(lx - 15)}" y="${f1(ly - 7)}" width="30" height="13" rx="3"/><text class="elt" x="${f1(lx)}" y="${f1(ly + 3)}" text-anchor="middle">${esc(li.abbr)}</text></g>`;
  }
  for (const a of mem) {
    const [px, py] = pos.get(a);
    const r = rowOf(a);
    const sc = r?.score ?? 0;
    const nr = 7 + sc / 12;
    const tier = r?.tier || "low";
    const nm = r?.name || shortAddr(a);
    const below = py >= cy;
    s += `<g${tipAttr(`<b>${esc(nm)}</b><br>${esc(a)}<br>score ${r ? sc : "– (not flagged)"}`)}><a href="#w/${esc(a)}"><circle class="node t-${esc(tier)}" cx="${f1(px)}" cy="${f1(py)}" r="${f1(nr)}"/>
<text class="nl" x="${f1(px)}" y="${f1(py + (below ? nr + 12 : -nr - 5))}" text-anchor="middle">${esc(trunc(nm, 18))}${r ? ` · ${sc}` : ""}</text></a></g>`;
  }
  return `<svg class="chart" width="${W}" height="${H}" viewBox="0 0 ${W} ${H}" role="img" aria-label="${esc(`Cluster ${c.id}: ${n} members, ${(c.links || []).length} links`)}">${s}</svg>`;
}

