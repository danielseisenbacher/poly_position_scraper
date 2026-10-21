#!/usr/bin/env python3
"""Generate internally consistent sample data for the polywatch dashboard.

Throwaway generator; only its JSON outputs are used by `site/` when no data
branch is reachable (local development). Shapes mirror the serde output of
`crates/types/src/site.rs` and `crates/types/src/domain.rs`.

    python3 site/sample-data/generate.py            # writes next to this file
    python3 site/sample-data/generate.py --now 1792500000   # fixed clock

Every index row has a wallet file; every cluster member, markets.json wallet and
tape row refers to an address in the index.
"""
import argparse
import hashlib
import json
import math
import os
import random
import time

SIGNALS = [
    ("NewAccountLongshot", 25), ("OldAccountWinRate", 30), ("LeaderboardEntrant", 15),
    ("LongshotWinner", 25), ("Concentration", 10), ("ResolutionTiming", 15),
    ("PreMove", 15), ("NicheMarket", 10), ("LinkedAccounts", 20),
]
LINK_CONF = {
    "SameOwnerEoa": 0.95, "SameXUsername": 0.9, "SameProfileImageId": 0.8,
    "SameNamePseudonym": 0.5, "SequentialUserId": 0.6, "FactoryCoCreation": 0.4, "CoTrade": 0.7,
}
MARKET_TITLES = [
    ("Will the Fed cut rates by 50bps in December?", "politics"),
    ("Will Israel strike Iranian nuclear facilities before November 30?", "politics"),
    ("Nobel Peace Prize 2026 winner: Yulia Navalnaya?", "other"),
    ("Will OpenAI announce GPT-6 before December 31?", "other"),
    ("Will the Bank of Japan raise rates in October?", "politics"),
    ("Taylor Swift engaged before 2027?", "other"),
    ("Will Zelensky and Putin meet in 2026?", "politics"),
    ("Apple announces foldable iPhone at October event?", "other"),
    ("Will Venezuela's Maduro leave office by Nov 30?", "politics"),
    ("Time Person of the Year 2026: Elon Musk?", "other"),
    ("Will the US government shut down before Nov 15?", "politics"),
    ("Will SpaceX Starship reach orbit in October?", "other"),
    ("Will Spotify raise US prices in Q4?", "other"),
    ("Will the ECB cut rates at its October meeting?", "politics"),
    ("UK snap general election called in 2026?", "politics"),
    ("Will Tesla unveil a $25k car before November?", "other"),
    ("Will Germany's coalition collapse by year end?", "politics"),
    ("Grammy Album of the Year nominee: Charli XCX?", "other"),
    ("Will the Supreme Court rule on tariffs by Nov 20?", "politics"),
    ("Will Nvidia close above $250 on Oct 31?", "crypto"),
    ("Real Madrid vs. Barcelona: Real Madrid wins?", "sports"),
    ("Will Pope Leo XIV visit China in 2026?", "other"),
    ("Michelin 3-star for Noma Kyoto?", "other"),
    ("Will Argentina lift capital controls by Dec 1?", "politics"),
    ("Netflix announces acquisition of a gaming studio in October?", "other"),
    ("Will Canada hold a federal election before 2027?", "politics"),
    ("Will the WHO declare a new PHEIC in 2026?", "other"),
    ("Will a US state legalise prediction markets by Nov 30?", "politics"),
]
FIRST = ["Gleaming", "Quiet", "Brisk", "Velvet", "Hollow", "Amber", "Crimson", "Lunar", "Rapid",
         "Silent", "Golden", "Frozen", "Wild", "Noble", "Tidal", "Copper"]
SECOND = ["Otter", "Heron", "Lynx", "Falcon", "Badger", "Marmot", "Ibis", "Koala", "Mantis",
          "Orca", "Pika", "Raven", "Stoat", "Tapir", "Vole", "Wren"]
REAL_NAMES = ["mkt_maker_99", "dc_insider?", "fedwatcher", "0xPolyGuy", "nobelbets",
              "<b>xss</b>&amp;test", "ünïcødé ☕", "longshotlarry", "swiftie2026"]


def hexs(rng, n):
    return "".join(rng.choice("0123456789abcdef") for _ in range(n))


def addr(rng):
    return "0x" + hexs(rng, 40)


def r2(x, d=2):
    return round(x, d)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--now", type=int, default=int(time.time()))
    ap.add_argument("--out", default=os.path.dirname(os.path.abspath(__file__)))
    ap.add_argument("--seed", type=int, default=7)
    a = ap.parse_args()
    rng = random.Random(a.seed)
    now = a.now
    D = 86400
    out = a.out
    os.makedirs(os.path.join(out, "wallets"), exist_ok=True)
    for f in os.listdir(os.path.join(out, "wallets")):
        if f.endswith(".json"):
            os.remove(os.path.join(out, "wallets", f))

    # ---- markets -------------------------------------------------------------
    markets = []
    for i, (title, cat) in enumerate(MARKET_TITLES):
        cid = "0x" + hashlib.sha256(title.encode()).hexdigest()
        resolved = i % 3 != 2  # two thirds resolved
        start = now - rng.randint(40, 200) * D
        end = now - rng.randint(1, 35) * D if resolved else now + rng.randint(3, 60) * D
        outcomes = ["Yes", "No"]
        win = (0 if rng.random() < 0.55 else 1) if resolved else None
        slug = "-".join(title.lower().replace("?", "").replace(":", "").replace("'", "").replace(",", "").replace("$", "").replace(".", "").split())[:60]
        markets.append(dict(
            condition_id=cid, title=title, slug=slug, start=start, end=end, category=cat,
            volume=rng.choice([18_000, 32_000, 45_000, 120_000, 480_000, 1_900_000, 7_400_000]) * rng.uniform(0.7, 1.3),
            outcomes=outcomes, win=win,
            tokens=[str(int(hashlib.sha256((cid + o).encode()).hexdigest()[:30], 16)) for o in outcomes],
            live=[r2(rng.uniform(0.05, 0.95), 3)] if not resolved else None,
        ))
        if markets[-1]["live"]:
            p = markets[-1]["live"][0]
            markets[-1]["live"] = [p, r2(1 - p, 3)]

    def outcome_price(m, oi):
        if m["win"] is not None:
            return 1.0 if oi == m["win"] else 0.0
        return m["live"][oi]

    # ---- wallets -------------------------------------------------------------
    N = 40
    addrs = sorted({addr(rng) for _ in range(N)})
    rng.shuffle(addrs)
    wallets = []
    for wi, ad in enumerate(addrs):
        rich = wi < 12  # the first 12 get many trades and markets
        if wi < 6:
            score = rng.randint(70, 96)
        elif wi < 20:
            score = rng.randint(45, 69)
        else:
            score = rng.randint(22, 44)
        tier = "high" if score >= 70 else "medium" if score >= 45 else "low"
        age_d = rng.choice([rng.uniform(3, 60), rng.uniform(90, 900)])
        joined = now - int(age_d * D)
        n_mk = rng.randint(3, 8) if rich else rng.randint(1, 4)
        mks = rng.sample(markets, n_mk)
        positions, trades = [], []
        for m in mks:
            oi = rng.randrange(2)
            # insiders buy cheap outcomes that tend to win
            if m["win"] is not None and rng.random() < 0.7 and score >= 45:
                oi = m["win"]
            avg = r2(rng.uniform(0.04, 0.35) if rng.random() < 0.7 else rng.uniform(0.35, 0.8), 4)
            cost = r2(rng.choice([150, 400, 900, 2500, 6000, 15000]) * rng.uniform(0.6, 1.5))
            size = r2(cost / avg)
            cur = outcome_price(m, oi)
            first_entry = max(joined + 3600, m["start"] + rng.randint(0, 20) * D)
            last_possible = min(m["end"], now) - rng.randint(1, 400) * 3600
            if first_entry >= last_possible:
                first_entry = last_possible - 3 * D
            if m["win"] is not None:
                status = "REDEEMABLE" if cur >= 0.999 else ("REDEEMABLE_LOST" if rng.random() < 0.7 else "CLOSED")
            else:
                status = "OPEN" if rng.random() < 0.85 else "CLOSED"
            # trades for this position
            n_tr = rng.randint(6, 26) if rich else rng.randint(1, 4)
            span = last_possible - first_entry
            sold = 0.0
            tlist = []
            for k in range(n_tr):
                ts = first_entry + int(span * (k / max(1, n_tr - 1)) ** rng.uniform(0.6, 2.5)) if k else first_entry
                is_sell = k > 1 and rng.random() < 0.18
                pr = min(0.99, max(0.01, avg * rng.uniform(0.75, 1.3) if not is_sell else avg * rng.uniform(0.9, 2.2)))
                usdc = r2(cost / n_tr * rng.uniform(0.3, 1.8))
                kind = "sell" if is_sell else "buy"
                if is_sell:
                    sold += usdc / pr
                tlist.append((ts, kind, r2(pr, 4), usdc))
            if m["win"] is not None and cur >= 0.999 and status == "REDEEMABLE" and rng.random() < 0.6:
                tlist.append((min(now - rng.randint(1, 72) * 3600, m["end"] + rng.randint(1, 30) * 3600), "redeem", 1.0, r2(size - sold)))
            for ts, kind, pr, usdc in tlist:
                tx = "0x" + hexs(rng, 64)
                sz = r2(usdc / pr)
                key = hashlib.sha256(f"{tx}|{m['tokens'][oi]}|{kind}|{sz}|{ts}".encode()).hexdigest()
                trades.append(dict(key=key, ts=int(ts), kind=kind, condition_id=m["condition_id"],
                                   token_id=m["tokens"][oi], title=m["title"], outcome=m["outcomes"][oi],
                                   price=pr, size=sz, usdc=usdc, tx=tx))
            current_size = 0.0 if status in ("CLOSED",) else r2(max(0.0, size - sold))
            cur_val = r2(current_size * cur)
            realized = r2((cur - avg) * size) if m["win"] is not None else r2(sold * (avg * 0.3))
            total_pnl = r2(cur_val - cost + (sold * avg)) if m["win"] is None else realized
            positions.append(dict(
                condition_id=m["condition_id"], token_id=m["tokens"][oi], title=m["title"], slug=m["slug"],
                outcome=m["outcomes"][oi], avg_price=avg, total_size=size, current_size=current_size,
                cost=cost, current_value=cur_val, current_price=cur, realized_pnl=realized,
                total_pnl=total_pnl, percent_pnl=r2(total_pnl / cost * 100, 1), status=status,
                first_entry_at=int(first_entry), last_event_at=int(max(t[0] for t in tlist)),
                end_date=(m["end"] // D) * D, market_volume=r2(m["volume"]),
                market_duration_h=r2((m["end"] - m["start"]) / 3600, 1), category=m["category"],
            ))
        # a couple of deposits
        for _ in range(rng.randint(0, 2)):
            ts = joined + rng.randint(1, 5) * 3600
            trades.append(dict(key=hexs(rng, 64), ts=ts, kind="deposit", condition_id="", token_id="",
                               title="", outcome="", price=0.0, size=0.0, usdc=r2(rng.uniform(500, 20000)),
                               tx="0x" + hexs(rng, 64)))
        trades.sort(key=lambda t: -t["ts"])
        trades = trades[:150]
        # rich wallets: ensure some recent activity for the tape
        if rich or rng.random() < 0.5:
            for _ in range(rng.randint(8, 22) if rich else rng.randint(2, 6)):
                op = [p for p in positions if p["status"] == "OPEN"] or positions
                p = rng.choice(op)
                ts = now - rng.randint(60, 47 * 3600)
                kind = "buy" if rng.random() < 0.8 else "sell"
                pr = r2(min(0.99, max(0.01, p["avg_price"] * rng.uniform(0.8, 1.5))), 4)
                usdc = r2(rng.choice([40, 120, 300, 650, 1500, 4200]) * rng.uniform(0.6, 1.4))
                tx = "0x" + hexs(rng, 64)
                trades.append(dict(key=hashlib.sha256((tx + kind).encode()).hexdigest(), ts=ts, kind=kind,
                                   condition_id=p["condition_id"], token_id=p["token_id"], title=p["title"],
                                   outcome=p["outcome"], price=pr, size=r2(usdc / pr), usdc=usdc, tx=tx))
            trades.sort(key=lambda t: -t["ts"])
        # resolved summary
        res = [p for p in positions if p["status"] in ("REDEEMABLE", "REDEEMABLE_LOST", "CLOSED") and p["current_price"] in (0.0, 1.0)]
        n = sum(0.5 if p["status"] == "CLOSED" else 1 for p in res)
        wins = sum((0.5 if p["status"] == "CLOSED" else 1) for p in res if p["current_price"] == 1.0)
        exp = sum((0.5 if p["status"] == "CLOSED" else 1) * p["avg_price"] for p in res)
        var = sum(p["avg_price"] * (1 - p["avg_price"]) for p in res) or 1
        z = r2((wins - exp) / math.sqrt(var), 2) if res else 0.0
        lsw = sum(1 for p in res if p["current_price"] == 1.0 and p["avg_price"] <= 0.35)
        if rich and wi % 2 == 0:  # pad old-account history so win rate looks like a real signal
            n += 12
            wins += 9
            z = r2(z + 2.4, 2)
            lsw += 3
        value = r2(sum(p["current_value"] for p in positions if p["status"] == "OPEN"))
        # signals
        k = rng.randint(1, 4) if score >= 45 else rng.randint(1, 2)
        kinds = rng.sample([s for s, _ in SIGNALS if s != "LinkedAccounts"], k)
        if age_d <= 60 and "NewAccountLongshot" not in kinds and rng.random() < 0.6:
            kinds[0] = "NewAccountLongshot"
        sigs = []
        for kd in kinds:
            st = r2(rng.uniform(0.25, 1.0), 2)
            refs = [p["condition_id"] for p in rng.sample(positions, min(len(positions), rng.randint(1, 3)))]
            ev = {
                "NewAccountLongshot": f"account {age_d:.0f} d old, ${value or 800:,.0f} in {len(positions)} markets at mean entry {sum(p['avg_price'] for p in positions)/len(positions):.2f}",
                "OldAccountWinRate": f"won {wins:g}/{n:g} resolved at mean entry {exp/max(n,1):.2f} (z={z:.1f})",
                "LeaderboardEntrant": f"#{rng.randint(5,480)} on weekly PnL leaderboard, {len(positions)} markets traded",
                "LongshotWinner": f"held winning token bought at {rng.uniform(0.04,0.3):.2f} in “{positions[0]['title'][:48]}”",
                "Concentration": f"top position {rng.randint(60,97)} % of value across {len(positions)} markets",
                "ResolutionTiming": f"{rng.randint(2,5)} wins entered < 48 h before end at avg {rng.uniform(0.08,0.45):.2f}",
                "PreMove": f"price +{rng.uniform(0.25,0.6):.2f} within 24 h after entry",
                "NicheMarket": f"cost basis {rng.uniform(3,18):.1f} % of ${rng.randint(12,49)}k market volume",
            }[kd]
            sigs.append(dict(kind=kd, strength=st, evidence=ev, refs=refs))
        sigs.sort(key=lambda s: -s["strength"])
        named = (0, 3, 7, 11, 13, 15, 22, 27, 33)
        nm = REAL_NAMES[named.index(wi)] if wi in named else ""
        pseudo = f"{rng.choice(FIRST)}-{rng.choice(SECOND)}"
        uid = rng.randint(100_000, 2_400_000)
        profile = dict(name=nm, pseudonym=pseudo, x_username=rng.choice(["", "", "", "polysleuth", "quietfund"]),
                       profile_image_id=rng.choice([None, rng.randint(10**9, 10**10)]), user_id=uid,
                       created_at=joined - rng.randint(0, 3600), owner_eoa=addr(rng) if rng.random() < 0.6 else None,
                       verified=False)
        if wi == 5:
            profile = None  # some wallets lack a Gamma profile
        stats = dict(join_date=joined, markets_traded=len(positions) + rng.randint(0, 30),
                     trade_count=len(trades) + rng.randint(0, 200),
                     volume_usdc=r2(sum(p["cost"] for p in positions) * rng.uniform(1, 3)),
                     realized_pnl=r2(sum(p["realized_pnl"] for p in positions)),
                     biggest_win=r2(max([p["realized_pnl"] for p in positions] + [0])))
        if wi == 9:
            stats = None
        sources = rng.sample(["feed", "leaderboard", "biggest_winner", "longshot_holder", "new_wallet", "seed", "watchlist"], rng.randint(1, 2))
        first_seen = now - rng.randint(1, 50) * 3600 * (1 if wi % 4 == 0 else 24)
        wallets.append(dict(
            addr=ad, profile=profile, stats=stats, positions=positions,
            resolved=dict(n=n, wins=wins, expected_wins=r2(exp, 3), z=z, longshot_wins=lsw),
            signals=sigs, score=dict(total=score, tier=tier, materiality=r2(min(1.3, max(0.6, 0.6 + 0.2 * math.log10(max(value, 100) / 100))), 3)),
            sources=sources, cluster_id=None, first_seen=first_seen, last_scored=now - rng.randint(1, 30) * 600,
            value=value, trades=trades[:150], issue=(100 + wi * 3) if score >= 60 else None,
        ))

    # ---- clusters ------------------------------------------------------------
    clusters = []
    for members_idx, kinds in (([1, 4, 8], ["SameOwnerEoa", "CoTrade", "SequentialUserId", "SameXUsername"]),
                               ([2, 10], ["SameProfileImageId", "FactoryCoCreation"])):
        mem = sorted(wallets[i]["addr"] for i in members_idx)
        cid = "c-" + mem[0][2:12]
        links = []
        pairs = [(mem[i], mem[j]) for i in range(len(mem)) for j in range(i + 1, len(mem))]
        for li, kd in enumerate(kinds):
            a_, b_ = pairs[li % len(pairs)]
            links.append(dict(a=a_, b=b_, kind=kd, confidence=LINK_CONF[kd], evidence={
                "SameOwnerEoa": "both proxies owned by EOA 0x" + hexs(rng, 4) + "…" + hexs(rng, 4),
                "CoTrade": f"{rng.randint(3,9)} trades within 60 s on the same condition and side",
                "SequentialUserId": f"user ids differ by {rng.randint(1,40)}, created {rng.randint(1,9)} min apart",
                "SameXUsername": "both profiles link X account @quietfund",
                "SameProfileImageId": "identical profile image id",
                "FactoryCoCreation": f"proxies deployed {rng.randint(2,59)} s apart by the same factory tx sender",
            }[kd]))
        clusters.append(dict(id=cid, members=mem, links=links))
        for i in members_idx:
            w = wallets[i]
            w["cluster_id"] = cid
            w["signals"].append(dict(kind="LinkedAccounts", strength=r2(rng.uniform(0.5, 1.0), 2),
                                     evidence=f"cluster {cid} with {len(mem)} members ({len(mem)-1} flagged)", refs=[]))
            w["signals"].sort(key=lambda s: -s["strength"])
        if members_idx == [2, 10]:
            # same pseudonym for the second cluster's members to exercise SameNamePseudonym look
            pass

    # ---- index ---------------------------------------------------------------
    index = []
    for i, w in enumerate(wallets):
        p = w["profile"] or {}
        name = p.get("name") or p.get("pseudonym") or ""
        joined = (w["stats"] or {}).get("join_date") or p.get("created_at")
        tr = [t for t in w["trades"] if t["kind"] in ("buy", "sell", "redeem")]
        index.append(dict(
            addr=w["addr"], name=name, score=w["score"]["total"], tier=w["score"]["tier"],
            signals=[[s["kind"], s["strength"]] for s in w["signals"]],
            age_d=r2((now - joined) / D, 1) if joined else None,
            pnl=r2(sum(x["total_pnl"] for x in w["positions"])),
            win_rate=r2(w["resolved"]["wins"] / w["resolved"]["n"], 4) if w["resolved"]["n"] > 0 else None,
            n_resolved=w["resolved"]["n"], z=w["resolved"]["z"], value=w["value"],
            n_markets=len({x["condition_id"] for x in w["positions"]}),
            last_trade_ts=max((t["ts"] for t in tr), default=None),
            cluster_id=w["cluster_id"], issue=w["issue"], watched=w["score"]["total"] >= 45 or i % 5 == 0,
            first_seen=w["first_seen"],
        ))
    index.sort(key=lambda r: -r["score"])

    # ---- markets.json: markets with >= 2 flagged wallets --------------------------
    by_market = {}
    for w in wallets:
        for p in w["positions"]:
            by_market.setdefault(p["condition_id"], []).append((w, p))
    mviews = []
    for m in markets:
        hold = by_market.get(m["condition_id"], [])
        if len(hold) < 2:
            continue
        mviews.append(dict(
            condition_id=m["condition_id"], title=m["title"], slug=m["slug"], end_date=m["end"],
            volume=r2(m["volume"]), resolved_outcome=m["outcomes"][m["win"]] if m["win"] is not None else None,
            wallets=sorted([dict(addr=w["addr"], score=w["score"]["total"], outcome=p["outcome"], avg_price=p["avg_price"],
                                 size=p["total_size"], current_price=p["current_price"]) for w, p in hold],
                           key=lambda x: -x["score"]),
        ))
    mviews.sort(key=lambda m: (-len(m["wallets"]), -max(x["score"] for x in m["wallets"])))
    mviews = mviews[:8]

    # ---- tape ----------------------------------------------------------------
    watched = {r["addr"] for r in index if r["watched"]}
    tape = []
    for w in wallets:
        if w["addr"] not in watched:
            continue
        p = w["profile"] or {}
        for t in w["trades"]:
            if t["ts"] >= now - 48 * 3600 and t["kind"] in ("buy", "sell", "redeem"):
                tape.append(dict(addr=w["addr"], name=p.get("name") or p.get("pseudonym") or "",
                                 score=w["score"]["total"], ts=t["ts"], kind=t["kind"], title=t["title"],
                                 outcome=t["outcome"], price=t["price"], usdc=t["usdc"],
                                 condition_id=t["condition_id"], tx=t["tx"]))
    tape.sort(key=lambda r: -r["ts"])

    # ---- meta ----------------------------------------------------------------
    hist = [0] * 10
    for r in index:
        hist[min(9, r["score"] // 10)] += 1
    # scored-but-not-flagged wallets fill the low buckets
    for b, extra in enumerate([812, 433, 251, 118, 40, 9, 3, 0, 0, 0]):
        hist[b] += extra
    sc = {}
    for r in index:
        for k, _ in r["signals"]:
            sc[k] = sc.get(k, 0) + 1
    meta = dict(
        generated_at=now - 95, run_id="11873450291", last_discover=now - 2 * 3600 - 640, last_track=now - 380,
        counts=dict(candidates=len(index), watchlist=len(watched) + 311, high=sum(r["tier"] == "high" for r in index),
                    medium=sum(r["tier"] == "medium" for r in index), clusters=len(clusters),
                    wallets_scanned_last_run=1742, trades_tracked_last_run=57),
        api=dict(requests=dict(data=2210, gamma=388, positions=1534, rpc=41, trades=12), rate_limited=3, errors=1),
        feed_lag_s=42, feed_truncated=False, score_histogram=hist, signal_counts=dict(sorted(sc.items())),
    )

    def dump(name, obj):
        with open(os.path.join(out, name), "w") as f:
            json.dump(obj, f, ensure_ascii=False, separators=(",", ":"))

    for w in wallets:
        dump(f"wallets/{w['addr']}.json", w)
    dump("index.json", index)
    dump("clusters.json", clusters)
    dump("markets.json", mviews)
    dump("activity_recent.json", tape)
    dump("meta.json", meta)
    rich_tr = sorted(len(w["trades"]) for w in wallets[:12])
    print(f"{len(index)} candidates, {len(wallets)} wallets (rich trade counts {rich_tr}), "
          f"{len(clusters)} clusters, {len(mviews)} markets, {len(tape)} tape rows")


if __name__ == "__main__":
    main()
