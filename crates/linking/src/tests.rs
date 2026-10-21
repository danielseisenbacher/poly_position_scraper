use super::*;
use types::{MarketCategory, PosStatus, Position, Profile};

fn addr(n: u64) -> Addr {
    // Distinguishing digits up front so cluster ids (first 10 hex chars) differ.
    Addr::parse(&format!("0x{n:04x}{}", "0".repeat(36))).unwrap()
}

fn wallet(n: u64) -> Wallet {
    Wallet::new(addr(n), 0)
}

fn with_profile(n: u64, f: impl FnOnce(&mut Profile)) -> Wallet {
    let mut w = wallet(n);
    let mut p = Profile::default();
    f(&mut p);
    w.profile = Some(p);
    w
}

fn position(condition: &str) -> Position {
    Position {
        condition_id: condition.into(),
        token_id: format!("{condition}-yes"),
        title: String::new(),
        slug: String::new(),
        outcome: "Yes".into(),
        avg_price: 0.2,
        total_size: 100.0,
        current_size: 100.0,
        cost: 20.0,
        current_value: 20.0,
        current_price: 0.2,
        realized_pnl: 0.0,
        total_pnl: 0.0,
        percent_pnl: 0.0,
        status: PosStatus::Open,
        first_entry_at: None,
        last_event_at: None,
        end_date: None,
        market_volume: None,
        market_duration_h: None,
        category: MarketCategory::Other,
    }
}

fn with_positions(n: u64, conds: &[&str]) -> Wallet {
    let mut w = wallet(n);
    w.positions = conds.iter().map(|c| position(c)).collect();
    w
}

fn trade(n: u64, condition: &str, side: Side, ts: i64) -> FeedTrade {
    FeedTrade {
        proxy_wallet: addr(n),
        side,
        token_id: format!("{condition}-yes"),
        condition_id: condition.into(),
        size: 10.0,
        price: 0.3,
        timestamp: ts,
        title: String::new(),
        slug: String::new(),
        event_slug: String::new(),
        outcome: String::new(),
        outcome_index: 0,
        name: String::new(),
        pseudonym: String::new(),
        profile_image: String::new(),
        transaction_hash: String::new(),
    }
}

fn flog(proxy: u64, owner: u64, block: u64) -> FactoryLog {
    FactoryLog {
        proxy: addr(proxy),
        owner: addr(owner),
        block,
        tx: format!("0xtx{proxy}"),
    }
}

fn links_with(
    wallets: &[Wallet],
    map: &ProxyOwnerMap,
    logs: &[FactoryLog],
    feed: &[FeedTrade],
) -> Vec<Link> {
    let input = LinkInput {
        wallets,
        proxy_owner: map,
        factory_logs: logs,
        feed,
    };
    find_links(&input, &LinkingConfig::default())
}

fn links_of(wallets: &[Wallet]) -> Vec<Link> {
    links_with(wallets, &ProxyOwnerMap::default(), &[], &[])
}

fn kinds(links: &[Link]) -> Vec<(u64, u64, LinkKind)> {
    links
        .iter()
        .map(|l| {
            let n = |a: &Addr| u64::from_str_radix(&a.as_str()[2..6], 16).unwrap();
            (n(&l.a), n(&l.b), l.kind)
        })
        .collect()
}

fn link(a: u64, b: u64, kind: LinkKind, confidence: f64) -> Link {
    Link {
        a: addr(a),
        b: addr(b),
        kind,
        confidence,
        evidence: String::new(),
    }
}

// ---------- SameOwnerEoa ----------

#[test]
fn same_owner_from_map_and_profile() {
    let mut map = ProxyOwnerMap::default();
    map.entries.insert(addr(1), (addr(100), 5));
    map.entries.insert(addr(2), (addr(100), 6));
    let w3 = with_profile(3, |p| p.owner_eoa = Some(addr(100)));
    let w4 = with_profile(4, |p| p.owner_eoa = Some(addr(101)));
    let links = links_with(&[wallet(1), wallet(2), w3, w4], &map, &[], &[]);
    use LinkKind::SameOwnerEoa as K;
    assert_eq!(kinds(&links), vec![(1, 2, K), (1, 3, K), (2, 3, K)]);
    assert!(links.iter().all(|l| l.confidence == 0.95));
    assert!(links[0].evidence.contains(&addr(100).short()));
}

#[test]
fn map_owner_overrides_profile_owner() {
    let mut map = ProxyOwnerMap::default();
    map.entries.insert(addr(1), (addr(100), 5));
    let w1 = with_profile(1, |p| p.owner_eoa = Some(addr(101)));
    let w2 = with_profile(2, |p| p.owner_eoa = Some(addr(101)));
    let links = links_with(&[w1, w2], &map, &[], &[]);
    assert!(links.is_empty());
}

#[test]
fn wallet_that_is_owner_links_to_its_proxy() {
    let mut map = ProxyOwnerMap::default();
    map.entries.insert(addr(5), (addr(1), 5));
    let links = links_with(&[wallet(1), wallet(5)], &map, &[], &[]);
    assert_eq!(kinds(&links), vec![(1, 5, LinkKind::SameOwnerEoa)]);
}

#[test]
fn owner_outside_set_and_self_owner_ignored() {
    let mut map = ProxyOwnerMap::default();
    map.entries.insert(addr(1), (addr(1), 5));
    map.entries.insert(addr(2), (addr(100), 5));
    map.entries.insert(addr(3), (addr(100), 5)); // 3 is not a candidate
    let links = links_with(&[wallet(1), wallet(2)], &map, &[], &[]);
    assert!(links.is_empty());
}

// ---------- attributes ----------

#[test]
fn x_handle_normalised() {
    let a = with_profile(1, |p| p.x_username = " @Alice ".into());
    let b = with_profile(2, |p| p.x_username = "alice".into());
    let c = with_profile(3, |p| p.x_username = "@".into());
    let d = with_profile(4, |p| p.x_username = "".into());
    let links = links_of(&[a, b, c, d]);
    assert_eq!(kinds(&links), vec![(1, 2, LinkKind::SameXUsername)]);
    assert_eq!(links[0].confidence, 0.9);
}

#[test]
fn same_profile_image() {
    let a = with_profile(1, |p| p.profile_image_id = Some(42));
    let b = with_profile(2, |p| p.profile_image_id = Some(42));
    let c = with_profile(3, |p| p.profile_image_id = Some(43));
    let d = with_profile(4, |p| p.profile_image_id = None);
    let links = links_of(&[a, b, c, d]);
    assert_eq!(kinds(&links), vec![(1, 2, LinkKind::SameProfileImageId)]);
}

#[test]
fn same_pseudonym_and_name() {
    let a = with_profile(1, |p| p.pseudonym = "Brave-Otter".into());
    let b = with_profile(2, |p| p.pseudonym = "Brave-Otter".into());
    let c = with_profile(3, |p| p.name = "WhaleHunter".into());
    let d = with_profile(4, |p| p.name = "whalehunter ".into());
    let e = with_profile(5, |p| p.pseudonym = "Shy-Fox".into());
    let links = links_of(&[a, b, c, d, e]);
    use LinkKind::SameNamePseudonym as K;
    assert_eq!(kinds(&links), vec![(1, 2, K), (3, 4, K)]);
    assert!(links[0].evidence.contains("pseudonym"));
    assert!(links[1].evidence.contains("name"));
}

#[test]
fn generic_names_ignored() {
    let names = ["", "ab", "0xdeadbeef", "123456"];
    let mut wallets = Vec::new();
    for (i, n) in names.iter().enumerate() {
        wallets.push(with_profile(10 + 2 * i as u64, |p| p.name = n.to_string()));
        wallets.push(with_profile(11 + 2 * i as u64, |p| p.name = n.to_string()));
    }
    // A name equal to the wallet's own address.
    wallets.push(with_profile(30, |p| p.name = addr(30).to_string()));
    wallets.push(with_profile(31, |p| p.name = addr(30).to_string()));
    assert!(links_of(&wallets).is_empty());
    assert!(is_generic_name("0X12", &addr(1)));
    assert!(!is_generic_name("Bob", &addr(1)));
}

#[test]
fn huge_attribute_group_skipped() {
    let wallets: Vec<Wallet> = (1..=(MAX_ATTRIBUTE_GROUP as u64 + 1))
        .map(|n| with_profile(n, |p| p.profile_image_id = Some(1)))
        .collect();
    assert!(links_of(&wallets).is_empty());
}

// ---------- SequentialUserId ----------

fn seq(n: u64, id: u64, created: Option<i64>) -> Wallet {
    with_profile(n, |p| {
        p.user_id = Some(id);
        p.created_at = created;
    })
}

#[test]
fn sequential_ids() {
    let wallets = [
        seq(1, 1000, Some(10_000)),
        seq(2, 1050, Some(10_600)), // Δid 50, Δt 600: link with 1
        seq(3, 1101, Some(10_700)), // Δid 51 to 2: no
        seq(4, 1110, Some(20_000)), // Δid 9 to 3 but 9300 s apart: no
        seq(5, 1111, None),         // created unknown: no
    ];
    let links = links_of(&wallets);
    assert_eq!(kinds(&links), vec![(1, 2, LinkKind::SequentialUserId)]);
    assert_eq!(links[0].confidence, 0.6);
}

#[test]
fn sequential_ids_window_scan_finds_non_adjacent() {
    let wallets = [
        seq(1, 100, Some(0)),
        seq(2, 110, Some(5000)),
        seq(3, 120, Some(30)),
    ];
    let links = links_of(&wallets);
    assert_eq!(kinds(&links), vec![(1, 3, LinkKind::SequentialUserId)]);
}

// ---------- FactoryCoCreation ----------

#[test]
fn factory_cocreation() {
    let wallets = [
        with_positions(1, &["m1", "m2"]),
        with_positions(2, &["m2"]), // 10 blocks after 1, other owner, shares m2 → link
        with_positions(3, &["m1"]), // same owner as 1 → not co-creation
        with_positions(4, &["m9"]), // no shared market
        with_positions(5, &["m1"]), // 31 blocks after 1 → outside 30-block window
    ];
    let logs = [
        flog(1, 100, 1000),
        flog(2, 200, 1010),
        flog(3, 100, 1012),
        flog(4, 300, 1005),
        flog(5, 400, 1031),
    ];
    let links = links_with(&wallets, &ProxyOwnerMap::default(), &logs, &[]);
    use LinkKind::FactoryCoCreation as K;
    // 3–5 are 19 blocks apart, different owners, share m1. 2–3 are close but share nothing.
    assert_eq!(kinds(&links), vec![(1, 2, K), (3, 5, K)]);
    assert!(links.iter().all(|l| l.confidence == 0.4));
}

#[test]
fn factory_logs_outside_set_ignored() {
    let wallets = [with_positions(1, &["m1"])];
    let logs = [flog(1, 100, 1000), flog(2, 200, 1001)];
    assert!(links_with(&wallets, &ProxyOwnerMap::default(), &logs, &[]).is_empty());
}

// ---------- CoTrade ----------

#[test]
fn cotrade_needs_three_occasions_across_two_markets() {
    let wallets = [wallet(1), wallet(2), wallet(3)];
    let feed = [
        // 1 and 2: three occasions over two markets.
        trade(1, "m1", Side::Buy, 1000),
        trade(2, "m1", Side::Buy, 1030),
        trade(1, "m1", Side::Buy, 5000),
        trade(2, "m1", Side::Buy, 5010),
        trade(2, "m2", Side::Buy, 9000),
        trade(1, "m2", Side::Buy, 9060),
        // 1 and 3: three occasions but all in m3.
        trade(1, "m3", Side::Buy, 100),
        trade(3, "m3", Side::Buy, 101),
        trade(1, "m3", Side::Buy, 300),
        trade(3, "m3", Side::Buy, 301),
        trade(1, "m3", Side::Buy, 500),
        trade(3, "m3", Side::Buy, 501),
        // 2 and 3: two markets but only two occasions; plus opposite side and too far apart.
        trade(2, "m4", Side::Buy, 100),
        trade(3, "m4", Side::Buy, 110),
        trade(2, "m5", Side::Sell, 100),
        trade(3, "m5", Side::Sell, 110),
        trade(2, "m6", Side::Buy, 100),
        trade(3, "m6", Side::Sell, 101),
        trade(2, "m7", Side::Buy, 100),
        trade(3, "m7", Side::Buy, 161),
    ];
    let links = links_with(&wallets, &ProxyOwnerMap::default(), &[], &feed);
    assert_eq!(kinds(&links), vec![(1, 2, LinkKind::CoTrade)]);
    assert_eq!(links[0].confidence, 0.7);
    assert!(
        links[0].evidence.starts_with("3 co-trades"),
        "{}",
        links[0].evidence
    );
}

#[test]
fn cotrade_same_bucket_counts_once() {
    let wallets = [wallet(1), wallet(2)];
    // Many trades in one minute of m1 are one occasion; plus one occasion in m2 → 2 < 3.
    let mut feed: Vec<FeedTrade> = (0..10)
        .flat_map(|k| {
            [
                trade(1, "m1", Side::Buy, 600 + k),
                trade(2, "m1", Side::Buy, 601 + k),
            ]
        })
        .collect();
    feed.push(trade(1, "m2", Side::Buy, 2000));
    feed.push(trade(2, "m2", Side::Buy, 2001));
    assert!(links_with(&wallets, &ProxyOwnerMap::default(), &[], &feed).is_empty());
}

#[test]
fn cotrade_ignores_crowded_windows() {
    let crowd = COTRADE_MAX_WALLETS_PER_WINDOW as u64;
    let wallets: Vec<Wallet> = (1..=crowd + 2).map(wallet).collect();
    let mut feed = Vec::new();
    for (k, m) in ["m1", "m2", "m3"].iter().enumerate() {
        let t0 = 10_000 * k as i64;
        feed.push(trade(1, m, Side::Buy, t0));
        feed.push(trade(2, m, Side::Buy, t0 + 1));
        if k < 2 {
            // Crowd joins two of the three occasions; those don't count.
            for n in 3..=crowd + 2 {
                feed.push(trade(n, m, Side::Buy, t0 + 2));
            }
        }
    }
    let links = links_with(&wallets, &ProxyOwnerMap::default(), &[], &feed);
    assert!(
        links.iter().all(|l| l.kind != LinkKind::CoTrade),
        "{:?}",
        kinds(&links)
    );

    // Without the crowd the same three occasions link 1 and 2.
    let feed: Vec<FeedTrade> = feed
        .into_iter()
        .filter(|t| t.proxy_wallet <= addr(2))
        .collect();
    let links = links_with(&wallets, &ProxyOwnerMap::default(), &[], &feed);
    assert_eq!(kinds(&links), vec![(1, 2, LinkKind::CoTrade)]);
}

#[test]
fn cotrade_ignores_wallets_outside_set() {
    let wallets = [wallet(1)];
    let feed: Vec<FeedTrade> = ["m1", "m2", "m3"]
        .iter()
        .enumerate()
        .flat_map(|(k, m)| {
            [
                trade(1, m, Side::Buy, k as i64 * 1000),
                trade(2, m, Side::Buy, k as i64 * 1000 + 1),
            ]
        })
        .collect();
    assert!(links_with(&wallets, &ProxyOwnerMap::default(), &[], &feed).is_empty());
}

// ---------- determinism ----------

#[test]
fn links_sorted_deduplicated_and_order_independent() {
    let mk = || {
        vec![
            with_profile(3, |p| {
                p.x_username = "x".into();
                p.pseudonym = "P-Q".into();
                p.name = "Carol".into();
            }),
            with_profile(1, |p| {
                p.x_username = "@X".into();
                p.pseudonym = "P-Q".into();
                p.name = "carol".into();
            }),
            with_profile(2, |p| p.x_username = "x".into()),
            with_profile(2, |p| p.x_username = "x".into()), // duplicate wallet
        ]
    };
    let mut ws = mk();
    let first = links_of(&ws);
    ws.reverse();
    let second = links_of(&ws);
    assert_eq!(first, second);
    use LinkKind::*;
    assert_eq!(
        kinds(&first),
        vec![
            (1, 2, SameXUsername),
            (1, 3, SameXUsername),
            (1, 3, SameNamePseudonym),
            (2, 3, SameXUsername)
        ]
    );
    assert!(first.iter().all(|l| l.a < l.b));
}

// ---------- cluster ----------

#[test]
fn cluster_transitive_and_id_format() {
    let links = vec![
        link(2, 3, LinkKind::SameXUsername, 0.9),
        link(1, 2, LinkKind::SameOwnerEoa, 0.95),
        link(7, 8, LinkKind::CoTrade, 0.7),
    ];
    let cs = cluster(&links, &LinkingConfig::default());
    assert_eq!(cs.len(), 2);
    assert_eq!(cs[0].members, vec![addr(1), addr(2), addr(3)]);
    assert_eq!(cs[0].id, "c-0001000000");
    assert_eq!(cs[0].links.len(), 2);
    assert_eq!(cs[0].links[0].kind, LinkKind::SameOwnerEoa);
    assert_eq!(cs[1].members, vec![addr(7), addr(8)]);
    assert_eq!(cs[1].links.len(), 1);
}

#[test]
fn cluster_id_uses_smallest_member_hex() {
    let a = Addr::parse("0xabcdef0123456789000000000000000000000000").unwrap();
    let b = Addr::parse("0xffff000000000000000000000000000000000000").unwrap();
    let l = Link {
        a: a.clone(),
        b,
        kind: LinkKind::SameOwnerEoa,
        confidence: 1.0,
        evidence: String::new(),
    };
    let cs = cluster(&[l], &LinkingConfig::default());
    assert_eq!(cs[0].id, "c-abcdef0123");
}

#[test]
fn low_confidence_links_attach_but_do_not_merge() {
    let links = vec![
        link(1, 2, LinkKind::SameOwnerEoa, 0.95),
        link(1, 2, LinkKind::FactoryCoCreation, 0.4), // attached: both ends in cluster
        link(2, 3, LinkKind::FactoryCoCreation, 0.4), // does not pull 3 in
        link(4, 5, LinkKind::FactoryCoCreation, 0.4), // no cluster at all
    ];
    let cs = cluster(&links, &LinkingConfig::default());
    assert_eq!(cs.len(), 1);
    assert_eq!(cs[0].members, vec![addr(1), addr(2)]);
    assert_eq!(
        cs[0].links.iter().map(|l| l.kind).collect::<Vec<_>>(),
        vec![LinkKind::SameOwnerEoa, LinkKind::FactoryCoCreation]
    );
}

#[test]
fn low_confidence_link_between_clusters_not_attached() {
    let links = vec![
        link(1, 2, LinkKind::SameOwnerEoa, 0.95),
        link(3, 4, LinkKind::SameOwnerEoa, 0.95),
        link(2, 3, LinkKind::FactoryCoCreation, 0.4),
    ];
    let cs = cluster(&links, &LinkingConfig::default());
    assert_eq!(cs.len(), 2);
    assert!(cs.iter().all(|c| c.links.len() == 1));
}

#[test]
fn cluster_order_independent() {
    let mut links = vec![
        link(5, 6, LinkKind::CoTrade, 0.7),
        link(1, 9, LinkKind::SameXUsername, 0.9),
        link(6, 9, LinkKind::SameProfileImageId, 0.8),
        link(2, 3, LinkKind::SequentialUserId, 0.6),
    ];
    let first = cluster(&links, &LinkingConfig::default());
    links.reverse();
    assert_eq!(first, cluster(&links, &LinkingConfig::default()));
    assert_eq!(first.len(), 2);
    assert_eq!(first[0].members, vec![addr(1), addr(5), addr(6), addr(9)]);
    assert!(first.windows(2).all(|w| w[0].id < w[1].id));
}

#[test]
fn cluster_of_found_links_end_to_end() {
    let mut map = ProxyOwnerMap::default();
    map.entries.insert(addr(1), (addr(100), 1));
    map.entries.insert(addr(2), (addr(100), 2));
    let w3 = with_profile(3, |p| p.profile_image_id = Some(7));
    let mut w2 = wallet(2);
    w2.profile = Some(Profile {
        profile_image_id: Some(7),
        ..Profile::default()
    });
    let ws = [wallet(1), w2, w3, wallet(4)];
    let links = links_with(&ws, &map, &[], &[]);
    let cs = cluster(&links, &LinkingConfig::default());
    assert_eq!(cs.len(), 1);
    assert_eq!(cs[0].members, vec![addr(1), addr(2), addr(3)]);
}

// ---------- proxy-owner map ----------

#[test]
fn proxy_owner_update_and_retention() {
    let mut map = ProxyOwnerMap::default();
    map.entries.insert(addr(1), (addr(100), 100));
    map.entries.insert(addr(2), (addr(100), 900));
    update_proxy_owner(
        &mut map,
        &[flog(3, 200, 1000), flog(2, 300, 950), flog(4, 100, 400)],
        1000,
        600,
    );
    // 1: 100 + 600 < 1000 → dropped. 4: 400 + 600 = 1000 → kept (boundary).
    let mut keys: Vec<_> = map.entries.keys().cloned().collect();
    keys.sort();
    assert_eq!(keys, vec![addr(2), addr(3), addr(4)]);
    assert_eq!(map.entries[&addr(2)], (addr(300), 950));
    assert_eq!(proxies_of(&map, &addr(100)), vec![addr(4)]);
    assert_eq!(proxies_of(&map, &addr(300)), vec![addr(2)]);
    assert!(proxies_of(&map, &addr(999)).is_empty());
}

#[test]
fn proxy_owner_retention_saturates() {
    let mut map = ProxyOwnerMap::default();
    update_proxy_owner(&mut map, &[flog(1, 2, 5)], 10, u64::MAX);
    assert_eq!(map.entries.len(), 1);
    update_proxy_owner(&mut map, &[], 10, 0);
    assert!(map.entries.is_empty());
}

#[test]
fn proxies_of_sorted() {
    let mut map = ProxyOwnerMap::default();
    for n in [9, 3, 7, 1] {
        map.entries.insert(addr(n), (addr(100), n));
    }
    assert_eq!(
        proxies_of(&map, &addr(100)),
        vec![addr(1), addr(3), addr(7), addr(9)]
    );
}
