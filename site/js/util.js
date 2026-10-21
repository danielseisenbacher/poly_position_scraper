// Formatting, escaping and small shared helpers. No DOM state lives here.

const ESC = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };
/** Escape any value for insertion into HTML text or a quoted attribute. */
export const esc = (v) => String(v ?? "").replace(/[&<>"']/g, (c) => ESC[c]);

export const nowS = () => Math.floor(Date.now() / 1000);

export const SIGNALS = {
  NewAccountLongshot: { abbr: "NAL", label: "New account longshot", c: 1 },
  OldAccountWinRate: { abbr: "OWR", label: "Old account win rate", c: 2 },
  LeaderboardEntrant: { abbr: "LBE", label: "Leaderboard entrant", c: 3 },
  LongshotWinner: { abbr: "LSW", label: "Longshot winner", c: 4 },
  Concentration: { abbr: "CON", label: "Concentration", c: 5 },
  ResolutionTiming: { abbr: "RTM", label: "Resolution timing", c: 6 },
  PreMove: { abbr: "PRE", label: "Pre-move", c: 7 },
  NicheMarket: { abbr: "NCH", label: "Niche market", c: 8 },
  LinkedAccounts: { abbr: "LNK", label: "Linked accounts", c: 9 },
};
export const sigInfo = (k) => SIGNALS[k] || { abbr: String(k).slice(0, 3).toUpperCase(), label: String(k), c: 9 };

export const LINKS = {
  SameOwnerEoa: { abbr: "EOA", label: "Same owner EOA", c: 1 },
  SameXUsername: { abbr: "X", label: "Same X username", c: 2 },
  SameProfileImageId: { abbr: "IMG", label: "Same profile image", c: 3 },
  SameNamePseudonym: { abbr: "NAME", label: "Same name/pseudonym", c: 4 },
  SequentialUserId: { abbr: "SEQ", label: "Sequential user id", c: 5 },
  FactoryCoCreation: { abbr: "FAC", label: "Factory co-creation", c: 6 },
  CoTrade: { abbr: "CO", label: "Co-trading", c: 7 },
};
export const linkInfo = (k) => LINKS[k] || { abbr: String(k).slice(0, 4), label: String(k), c: 8 };

export const shortAddr = (a) => (a && a.length > 12 ? `${a.slice(0, 6)}…${a.slice(-4)}` : a || "");

const isNum = (n) => typeof n === "number" && Number.isFinite(n);

/** $1,234 / $12.3k / $4.56M. */
export function usd(n, { sign = false } = {}) {
  if (!isNum(n)) return "–";
  const s = n < 0 ? "−" : sign && n > 0 ? "+" : "";
  const a = Math.abs(n);
  let t;
  if (a >= 1e6) t = (a / 1e6).toFixed(a >= 1e7 ? 1 : 2) + "M";
  else if (a >= 1e4) t = (a / 1e3).toFixed(1) + "k";
  else if (a >= 1000) t = Math.round(a).toLocaleString("en-US");
  else t = a >= 100 ? a.toFixed(0) : a.toFixed(a >= 10 ? 1 : 2);
  return `${s}$${t}`;
}
export const num = (n, d = 0) => (isNum(n) ? n.toLocaleString("en-US", { maximumFractionDigits: d, minimumFractionDigits: d }) : "–");
export const price = (p) => (isNum(p) ? p.toFixed(p < 0.1 && p > 0 ? 3 : 2) : "–");
export const pct = (p, d = 0) => (isNum(p) ? (p * 100).toFixed(d) + "%" : "–");
export const signed = (n, d = 1) => (isNum(n) ? (n > 0 ? "+" : n < 0 ? "−" : "") + Math.abs(n).toFixed(d) : "–");
/** Trim trailing ".0" for fractional counts (CLOSED positions count half). */
export const frac = (n) => (isNum(n) ? (Number.isInteger(n) ? String(n) : n.toFixed(1)) : "–");
export const pnlClass = (n) => (!isNum(n) || n === 0 ? "" : n > 0 ? "pos" : "neg");

export function rel(ts, now = nowS()) {
  if (!isNum(ts) || ts <= 0) return "–";
  const d = now - ts;
  const a = Math.abs(d);
  const f = a < 60 ? `${Math.round(a)} s` : a < 3600 ? `${Math.round(a / 60)} min` : a < 172800 ? `${Math.round(a / 3600)} h` : `${Math.round(a / 86400)} d`;
  return d >= 0 ? `${f} ago` : `in ${f}`;
}
export const relShort = (ts, now = nowS()) => rel(ts, now).replace(" ago", "").replace(" min", "m").replace(" s", "s").replace(" h", "h").replace(" d", "d");

export function date(ts, withTime = false) {
  if (!isNum(ts)) return "–";
  const iso = new Date(ts * 1000).toISOString();
  return withTime ? iso.slice(0, 16).replace("T", " ") + "Z" : iso.slice(0, 10);
}

export function debounce(fn, ms) {
  let t;
  return (...a) => {
    clearTimeout(t);
    t = setTimeout(() => fn(...a), ms);
  };
}

/** localStorage access that never throws (private mode, blocked storage). */
export const store = {
  get(k, d = null) {
    try {
      const v = localStorage.getItem(k);
      return v == null ? d : JSON.parse(v);
    } catch {
      return d;
    }
  },
  set(k, v) {
    try {
      localStorage.setItem(k, JSON.stringify(v));
    } catch {
      /* ignore */
    }
  },
};

/** Stable comparator: nulls last regardless of direction. */
export function cmp(a, b, dir) {
  const na = a == null || (typeof a === "number" && Number.isNaN(a));
  const nb = b == null || (typeof b === "number" && Number.isNaN(b));
  if (na || nb) return na === nb ? 0 : na ? 1 : -1;
  if (typeof a === "string") return dir * a.localeCompare(b);
  return dir * (a - b);
}

/** Polymarket position "won": mirrors `Position::won` in crates/types/src/domain.rs. */
export function won(p) {
  if (p.status === "REDEEMABLE_LOST") return false;
  if (p.status === "REDEEMABLE") return p.current_price >= 0.999;
  if (p.status === "CLOSED") return p.current_price >= 0.999 ? true : p.current_price <= 0.001 ? false : null;
  return null;
}

export const KIND = {
  buy: { sym: "▲", short: "B", cls: "buy" },
  sell: { sym: "▼", short: "S", cls: "sell" },
  redeem: { sym: "◆", short: "R", cls: "redeem" },
  split: { sym: "⧉", short: "SP", cls: "other" },
  merge: { sym: "⧓", short: "M", cls: "other" },
  deposit: { sym: "↓", short: "D", cls: "other" },
  withdraw: { sym: "↑", short: "W", cls: "other" },
  other: { sym: "·", short: "?", cls: "other" },
};
export const kindInfo = (k) => KIND[k] || KIND.other;
export const kindBadge = (k) => {
  const i = kindInfo(k);
  return `<span class="kind ${i.cls}" title="${esc(k)}">${i.sym}&#8239;${i.short}</span>`;
};
