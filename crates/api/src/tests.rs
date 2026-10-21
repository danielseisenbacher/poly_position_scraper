//! Wiremock tests serving the recorded `fixtures/live` responses.

use std::time::{Duration, Instant};

use serde_json::{json, Value};
use types::api::*;
use types::config::ApiConfig;
use types::{Addr, MarketCategory, PosStatus};
use wiremock::matchers::{
    body_partial_json, header, method, path, query_param, query_param_is_missing,
};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

use crate::http::now_s;
use crate::Client;

const USER: &str = "0xf3238cc8d3ef945c39b36712040886e3847eb0d7";
const COND: &str = "0xa398aadfd49f48fcc3690f1aa5146dd4a7eab1e6bc346bdcf0f97d00d002dc23";
const COMBO: &str = "0x038e536f830e26cc80c6b155230ea0a93c0000000000000000000000000000";

fn fixture(name: &str) -> String {
    let p = format!(
        "{}/../../fixtures/live/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

fn fixture_json(name: &str) -> Value {
    serde_json::from_str(&fixture(name)).unwrap()
}

fn ok(name: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(fixture(name), "application/json")
}

fn config(server: &MockServer) -> ApiConfig {
    ApiConfig {
        data_base: format!("{}/v2", server.uri()),
        gamma_base: format!("{}/gamma", server.uri()),
        rpc_url: format!("{}/rpc", server.uri()),
        max_retries: 3,
        timeout_s: 5,
        ..ApiConfig::default()
    }
}

fn client(server: &MockServer) -> Client {
    client_with(config(server))
}

fn client_with(cfg: ApiConfig) -> Client {
    let mut c = Client::new(&cfg).unwrap();
    c.backoff_base = Duration::from_millis(5);
    c
}

fn user() -> Addr {
    Addr::parse(USER).unwrap()
}

async fn mount_get(server: &MockServer, p: &str, fixture_name: &str) {
    Mock::given(method("GET"))
        .and(path(p))
        .respond_with(ok(fixture_name))
        .mount(server)
        .await;
}

async fn received_query(server: &MockServer, p: &str) -> Vec<(String, String)> {
    let reqs = server.received_requests().await.unwrap();
    let r = reqs
        .iter()
        .find(|r| r.url.path() == p)
        .unwrap_or_else(|| panic!("no request to {p}"));
    r.url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

fn param<'a>(q: &'a [(String, String)], key: &str) -> Option<&'a str> {
    q.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

#[tokio::test]
async fn decodes_every_data_endpoint_from_fixtures() {
    let server = MockServer::start().await;
    let ua = ApiConfig::default().user_agent;
    Mock::given(method("GET"))
        .and(path("/v2/trades"))
        .and(query_param_is_missing("user"))
        .and(header("user-agent", ua.as_str()))
        .respond_with(ok("trades_global"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/trades"))
        .and(query_param("user", USER))
        .respond_with(ok("trades_user"))
        .mount(&server)
        .await;
    mount_get(&server, "/v2/activity", "activity_user").await;
    for s in ["OPEN", "CLOSED", "REDEEMABLE", "REDEEMABLE_LOST"] {
        Mock::given(method("GET"))
            .and(path("/v2/positions"))
            .and(query_param("status", s))
            .respond_with(ok(&format!("positions_user_{s}")))
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/v2/positions"))
        .and(query_param("condition", COND))
        .respond_with(ok("positions_condition"))
        .mount(&server)
        .await;
    mount_get(&server, "/v2/holders", "holders_pnl").await;
    mount_get(&server, "/v2/user-stats", "user_stats").await;
    mount_get(&server, "/v2/value", "value_user").await;
    mount_get(&server, "/v2/leaderboard", "leaderboard").await;
    mount_get(&server, "/v2/biggest-winners", "biggest_winners").await;
    mount_get(&server, "/v2/tokens", "tokens_resolved").await;
    mount_get(&server, "/v2/prices-history", "prices_history").await;
    mount_get(&server, "/v2/status", "status").await;
    let c = client(&server);

    let feed = c
        .trades_page(
            &TradesQuery {
                limit: 200,
                taker_only: true,
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(feed.data.len(), 200);
    assert!(feed.next_cursor.is_some());
    assert!(feed.data.iter().any(FeedTrade::is_combo));

    let ut = c.user_trades(&user(), None, None).await.unwrap();
    assert!(ut.data.iter().all(|t| t.proxy_wallet == user()));

    let act = c
        .activity(
            &ActivityQuery {
                user: user(),
                start: Some(1),
                end: None,
                limit: 5,
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(act.data.len(), 5);
    assert_eq!(act.data[0].kind, ActivityType::Trade);

    for s in [
        PosStatus::Open,
        PosStatus::Closed,
        PosStatus::Redeemable,
        PosStatus::RedeemableLost,
    ] {
        let p = c.positions(&user(), s, None).await.unwrap();
        assert_eq!(p.data.len(), 5, "{s:?}");
    }
    let mp = c.market_positions(COND, None, None, None).await.unwrap();
    assert!(mp.data.iter().all(|p| p.condition_id == COND));

    let h = c.holders(COND, true, 3).await.unwrap();
    assert_eq!(h.len(), 2);
    assert!(h[0].holders[0].avg_price.is_some());

    let st = c.user_stats(&user()).await.unwrap().unwrap();
    assert_eq!(st.trades, 57);
    assert_eq!(st.all_time_pnl.unwrap().trade_count, 62);
    assert_eq!(c.value(&user()).await.unwrap(), 57.543);

    let lb = c
        .leaderboard(TimePeriod::Week, LeaderboardSort::Pnl, None)
        .await
        .unwrap();
    assert_eq!(lb.data[0].rank, 1);
    let bw = c.biggest_winners(TimePeriod::Day, None).await.unwrap();
    assert_eq!(bw.data[0].win_rank, 1);
    let tk = c
        .tokens("0xd2bf910585399de5a79980e4160f4366111c540e3f55f8ff75c300f89e57eac8")
        .await
        .unwrap();
    assert_eq!(tk.len(), 2);
    let ph = c
        .prices_history("123", 1_792_496_000, 1_792_582_400)
        .await
        .unwrap();
    assert_eq!(ph.len(), 286);
    assert!(ph.windows(2).all(|w| w[0].timestamp < w[1].timestamp));
    let status = c.status().await.unwrap();
    assert_eq!(
        status,
        ApiStatus {
            lag_seconds: 2,
            max_synced_block: Some(95_918_131)
        }
    );

    let stats = c.stats();
    assert_eq!(stats.requests[&RateClass::Trades], 2);
    assert_eq!(stats.requests[&RateClass::Positions], 6);
    assert_eq!(stats.requests[&RateClass::Data], 8);
    assert_eq!((stats.retries, stats.rate_limited, stats.errors), (0, 0, 0));

    let q = received_query(&server, "/v2/leaderboard").await;
    assert_eq!(param(&q, "time_period"), Some("week"));
    assert_eq!(param(&q, "sort_by"), Some("PNL"));
    assert_eq!(param(&q, "limit"), Some("1000"));
    let q = received_query(&server, "/v2/holders").await;
    assert_eq!(param(&q, "include_pnl"), Some("true"));
}

#[tokio::test]
async fn null_data_and_holder_limit_clamp() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/user-stats"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": null})))
        .mount(&server)
        .await;
    Mock::given(path("/v2/value"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {"value": null}})))
        .mount(&server)
        .await;
    Mock::given(path("/v2/holders"))
        .and(query_param("limit", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": []})))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    assert_eq!(c.user_stats(&user()).await.unwrap(), None);
    assert_eq!(c.value(&user()).await.unwrap(), 0.0);
    assert!(c.holders(COND, true, 500).await.unwrap().is_empty());
}

#[tokio::test]
async fn follows_next_cursor_until_exhausted() {
    let server = MockServer::start().await;
    let page2 = json!({"data": [], "pagination": {"has_more": false, "next_cursor": null}});
    Mock::given(path("/v2/trades"))
        .and(query_param("cursor", "abc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page2))
        .mount(&server)
        .await;
    Mock::given(path("/v2/trades"))
        .and(query_param_is_missing("cursor"))
        .respond_with(ok("trades_user"))
        .mount(&server)
        .await;
    let c = client(&server);
    let mut cursor: Option<String> = None;
    let mut rows = 0;
    let mut pages = 0;
    loop {
        let p = c
            .user_trades(&user(), None, cursor.as_deref())
            .await
            .unwrap();
        rows += p.data.len();
        pages += 1;
        match p.next_cursor {
            // Substitute a short cursor for the recorded one to keep the mock simple.
            Some(_) if pages == 1 => cursor = Some("abc".into()),
            Some(c) => panic!("unexpected cursor {c}"),
            None => break,
        }
    }
    assert_eq!((pages, rows), (2, 5));
}

#[tokio::test]
async fn honours_retry_after_on_429() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/status"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "1"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    mount_get(&server, "/v2/status", "status").await;
    let c = client(&server);
    let t0 = Instant::now();
    c.status().await.unwrap();
    assert!(t0.elapsed() >= Duration::from_secs(1));
    let s = c.stats();
    assert_eq!((s.rate_limited, s.retries, s.errors), (1, 1, 0));
    assert_eq!(s.requests[&RateClass::Data], 2);
}

#[tokio::test]
async fn exhausted_429_is_rate_limited() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/status"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
        .expect(4)
        .mount(&server)
        .await;
    let c = client(&server);
    assert!(matches!(
        c.status().await,
        Err(ApiError::RateLimited { .. })
    ));
    let s = c.stats();
    assert_eq!((s.rate_limited, s.retries, s.errors), (4, 3, 1));
}

#[tokio::test]
async fn retries_503_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/value"))
        .respond_with(ResponseTemplate::new(503).set_body_string(r#"{"code":"request_timeout"}"#))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    mount_get(&server, "/v2/value", "value_user").await;
    let c = client(&server);
    assert_eq!(c.value(&user()).await.unwrap(), 57.543);
    assert_eq!(c.stats().retries, 2);
}

#[tokio::test]
async fn persistent_5xx_returns_http_error() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/value"))
        .respond_with(ResponseTemplate::new(502))
        .expect(4)
        .mount(&server)
        .await;
    let c = client(&server);
    assert!(matches!(
        c.value(&user()).await,
        Err(ApiError::Http { status: 502, .. })
    ));
}

#[tokio::test]
async fn client_errors_are_not_retried() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/value"))
        .respond_with(
            ResponseTemplate::new(400).set_body_string(r#"{"error":"unknown parameter"}"#),
        )
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    match c.value(&user()).await {
        Err(ApiError::Http {
            status: 400, body, ..
        }) => assert!(body.contains("unknown parameter")),
        other => panic!("{other:?}"),
    }
    let s = c.stats();
    assert_eq!((s.retries, s.errors), (0, 1));
}

#[tokio::test]
async fn transport_errors_are_retried_then_reported() {
    let server = MockServer::start().await;
    let mut cfg = config(&server);
    cfg.data_base = "http://127.0.0.1:9/v2".into();
    cfg.max_retries = 1;
    let c = client_with(cfg);
    assert!(matches!(c.status().await, Err(ApiError::Transport { .. })));
    assert_eq!(c.stats().retries, 1);
}

#[tokio::test]
async fn bad_json_is_a_decode_error_with_excerpt() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/status"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>oops</html>"))
        .mount(&server)
        .await;
    let c = client(&server);
    match c.status().await {
        Err(ApiError::Decode { msg, .. }) => assert!(msg.contains("<html>oops")),
        other => panic!("{other:?}"),
    }
    assert_eq!(c.stats().errors, 1);
}

#[tokio::test]
async fn cache_busters_are_sent() {
    let server = MockServer::start().await;
    let empty = ResponseTemplate::new(200)
        .set_body_json(json!({"data": [], "pagination": {"has_more": false}}));
    for p in ["/v2/trades", "/v2/activity", "/v2/positions"] {
        Mock::given(path(p))
            .respond_with(empty.clone())
            .mount(&server)
            .await;
    }
    let c = client(&server);
    let now = now_s();

    c.trades_page(
        &TradesQuery {
            limit: 1000,
            taker_only: true,
            min_cash: None,
            side: Some(Side::Buy),
        },
        None,
    )
    .await
    .unwrap();
    let q = received_query(&server, "/v2/trades").await;
    assert_eq!(param(&q, "filter_type"), Some("CASH"));
    let amount: f64 = param(&q, "filter_amount").unwrap().parse().unwrap();
    assert!(amount > 0.0 && amount < 0.000_002, "{amount}");
    assert_eq!(param(&q, "side"), Some("BUY"));
    assert_eq!(param(&q, "taker_only"), Some("true"));
    assert_eq!(param(&q, "cursor"), None);

    c.activity(
        &ActivityQuery {
            user: user(),
            start: Some(100),
            end: None,
            limit: 0,
        },
        None,
    )
    .await
    .unwrap();
    let q = received_query(&server, "/v2/activity").await;
    let end: i64 = param(&q, "end").unwrap().parse().unwrap();
    assert!((now + 59..=now + 62).contains(&end));
    assert_eq!(param(&q, "start"), Some("100"));
    assert_eq!(param(&q, "limit"), Some("1000"));
    assert_eq!(param(&q, "sort_direction"), Some("DESC"));
    assert_eq!(param(&q, "exclude_deposits_withdrawals"), Some("false"));

    c.positions(&user(), PosStatus::RedeemableLost, Some("cur"))
        .await
        .unwrap();
    let q = received_query(&server, "/v2/positions").await;
    let end: i64 = param(&q, "end").unwrap().parse().unwrap();
    assert!((now + 86_399..=now + 86_402).contains(&end));
    assert_eq!(param(&q, "status"), Some("REDEEMABLE_LOST"));
    assert_eq!(param(&q, "cursor"), Some("cur"));
    assert_eq!(param(&q, "filter_amount"), None);
}

#[tokio::test]
async fn explicit_min_cash_replaces_cache_buster() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/trades"))
        .and(query_param("filter_amount", "500"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": []})))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    let p = c
        .trades_page(
            &TradesQuery {
                limit: 10,
                min_cash: Some(500.0),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(p, Page::default());
}

#[tokio::test]
async fn market_positions_sends_status_only_when_given() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/positions"))
        .and(query_param("condition", COND))
        .and(query_param("status", "CLOSED"))
        .respond_with(ok("positions_condition"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/positions"))
        .and(query_param("condition", COND))
        .and(query_param_is_missing("status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": []})))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    let closed = c
        .market_positions(COND, Some(PosStatus::Closed), None, Some("abc"))
        .await
        .unwrap();
    assert!(!closed.data.is_empty());
    let current = c.market_positions(COND, None, None, None).await.unwrap();
    assert!(current.data.is_empty());
    let reqs = server.received_requests().await.unwrap();
    let q: Vec<(String, String)> = reqs[0]
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(param(&q, "status"), Some("CLOSED"));
    assert_eq!(param(&q, "cursor"), Some("abc"));
    assert_eq!(param(&q, "limit"), Some("1000"));
    assert_eq!(param(&q, "start"), None);
}

#[tokio::test]
async fn synthetic_conditions_short_circuit() {
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(400))
        .expect(0)
        .mount(&server)
        .await;
    let c = client(&server);
    assert!(c
        .market_positions(COMBO, None, None, None)
        .await
        .unwrap()
        .data
        .is_empty());
    assert!(c.holders(COMBO, false, 10).await.unwrap().is_empty());
    assert!(c.tokens(COMBO).await.unwrap().is_empty());
    assert!(c
        .markets_by_condition(&[COMBO.to_string()])
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn prices_history_splits_long_windows() {
    let server = MockServer::start().await;
    Mock::given(path("/v2/prices-history"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"data": [{"timestamp": 5, "price": 0.1}]})),
        )
        .expect(3)
        .mount(&server)
        .await;
    let c = client(&server);
    let pts = c.prices_history("tok", 0, 40 * 86_400).await.unwrap();
    assert_eq!(pts.len(), 1, "duplicate timestamps are merged");
    let reqs = server.received_requests().await.unwrap();
    for r in &reqs {
        let q: Vec<(String, String)> = r
            .url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        let (s, e): (i64, i64) = (
            param(&q, "start").unwrap().parse().unwrap(),
            param(&q, "end").unwrap().parse().unwrap(),
        );
        assert!(e - s <= 15 * 86_400);
        assert_eq!(param(&q, "bucket_seconds"), None);
    }
    assert!(c.prices_history("tok", 10, 10).await.unwrap().is_empty());
}

#[tokio::test]
async fn gamma_closed_markets_are_normalised() {
    let server = MockServer::start().await;
    Mock::given(path("/gamma/markets"))
        .and(query_param("closed", "true"))
        .and(query_param("order", "closedTime"))
        .and(query_param("ascending", "false"))
        .and(query_param("volume_num_min", "10000"))
        .and(query_param("offset", "0"))
        .respond_with(ok("gamma_markets_closed"))
        .expect(3)
        .mount(&server)
        .await;
    let c = client(&server);
    let ms = c.recently_closed_markets(0, 10_000.0, 50).await.unwrap();
    assert_eq!(ms.len(), 3);
    let m = &ms[0];
    assert_eq!(
        m.condition_id,
        "0xd2bf910585399de5a79980e4160f4366111c540e3f55f8ff75c300f89e57eac8"
    );
    assert_eq!(
        m.outcomes,
        vec![
            "Ponomarenko Dmytro".to_string(),
            "Kozoriz Denys".to_string()
        ]
    );
    assert_eq!(m.outcome_prices, vec![0.0, 1.0]);
    assert_eq!(m.token_ids.len(), 2);
    assert_eq!(
        m.token_ids[0],
        "97073274700832664511346065307918266018912790796509036438093703972522680790130"
    );
    assert_eq!(m.winning_index, Some(1));
    assert_eq!(m.winning_token(), Some(m.token_ids[1].as_str()));
    assert_eq!(m.closed_time, Some(1_792_582_562));
    assert_eq!(m.end_date, Some(1_793_184_900));
    assert_eq!(m.start_date, Some(1_792_526_912));
    assert_eq!(m.event_slug, "setkameua-ponom-den-2026-10-21");
    assert_eq!(m.category, MarketCategory::Sports);
    assert_eq!(m.liquidity, Some(0.0));
    assert_eq!(ms[2].winning_index, Some(0));

    // Stops once markets are older than `since`, and honours `max`.
    let since = ms[1].closed_time.unwrap();
    assert!(
        c.recently_closed_markets(since + 1, 10_000.0, 50)
            .await
            .unwrap()
            .len()
            <= 1
    );
    assert_eq!(
        c.recently_closed_markets(0, 10_000.0, 2)
            .await
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn gamma_markets_by_condition_batches_open_then_closed() {
    let server = MockServer::start().await;
    let closed: Vec<Value> = serde_json::from_value(fixture_json("gamma_markets_closed")).unwrap();
    Mock::given(path("/gamma/markets"))
        .and(query_param_is_missing("closed"))
        .respond_with(ok("gamma_markets_by_condition"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/gamma/markets"))
        .and(query_param("closed", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&closed[..1]))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    let closed_id = closed[0]["conditionId"].as_str().unwrap().to_string();
    let ids = vec![
        COND.to_uppercase().replace("0X", "0x"),
        COND.to_string(),
        closed_id.clone(),
        COMBO.to_string(),
    ];
    let ms = c.markets_by_condition(&ids).await.unwrap();
    assert_eq!(ms.len(), 2);
    let open = ms.iter().find(|m| m.condition_id == COND).unwrap();
    assert_eq!(open.category, MarketCategory::Crypto);
    assert_eq!(open.winning_index, None, "open market has no winner");
    assert_eq!(open.volume, Some(93.44));
    assert_eq!(open.duration_h().map(|h| h.round()), Some(24.0));
    assert!(ms.iter().any(|m| m.condition_id == closed_id));

    let reqs = server.received_requests().await.unwrap();
    let first: Vec<String> = reqs[0]
        .url
        .query_pairs()
        .filter(|(k, _)| k == "condition_ids")
        .map(|(_, v)| v.into_owned())
        .collect();
    assert_eq!(first.len(), 2, "deduped, synthetic skipped: {first:?}");
    let second: Vec<String> = reqs[1]
        .url
        .query_pairs()
        .filter(|(k, _)| k == "condition_ids")
        .map(|(_, v)| v.into_owned())
        .collect();
    assert_eq!(second, vec![closed_id]);
}

#[tokio::test]
async fn gamma_profile_and_404() {
    let server = MockServer::start().await;
    Mock::given(path("/gamma/public-profile"))
        .and(query_param("address", USER))
        .respond_with(ok("gamma_profile"))
        .mount(&server)
        .await;
    Mock::given(path("/gamma/public-profile"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(json!({"type": "not found error", "error": "profile not found"})),
        )
        .mount(&server)
        .await;
    let c = client(&server);
    let p = c.profile(&user()).await.unwrap().unwrap();
    assert_eq!(p.name, "EddyPol");
    assert_eq!(p.pseudonym, "Vigorous-Gambling");
    assert_eq!(p.user_id, Some(8_778_498));
    assert_eq!(p.created_at, Some(1_781_863_405));
    assert!(!p.verified);
    assert_eq!(p.profile_image_id, None);
    assert_eq!(p.owner_eoa, None);

    let other = Addr::parse("0x0000000000000000000000000000000000000001").unwrap();
    assert_eq!(c.profile(&other).await.unwrap(), None);
    assert_eq!(c.stats().errors, 0);
    assert_eq!(c.stats().requests[&RateClass::Gamma], 2);
}

/// Fake node: rejects spans wider than `max_span`, otherwise serves the recorded logs in range.
struct FakeNode {
    max_span: u64,
    logs: Vec<Value>,
}

impl Respond for FakeNode {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&req.body).unwrap();
        let hex = |v: &Value| {
            u64::from_str_radix(v.as_str().unwrap().trim_start_matches("0x"), 16).unwrap()
        };
        let f = &body["params"][0];
        let (from, to) = (hex(&f["fromBlock"]), hex(&f["toBlock"]));
        if to - from + 1 > self.max_span {
            return ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0", "id": 1,
                "error": {"code": -32701, "message": format!("exceed maximum block range: {}", self.max_span)}
            }));
        }
        let mut result: Vec<Value> = self
            .logs
            .iter()
            .filter(|l| (from..=to).contains(&hex(&l["blockNumber"])))
            .cloned()
            .collect();
        result.reverse();
        ResponseTemplate::new(200)
            .set_body_json(json!({"jsonrpc": "2.0", "id": 1, "result": result}))
    }
}

#[tokio::test]
async fn factory_logs_decode_and_halve_chunks() {
    let server = MockServer::start().await;
    let cfg = ApiConfig {
        rpc_log_chunk: 8,
        ..config(&server)
    };
    let logs = fixture_json("factory_logs")["result"]
        .as_array()
        .unwrap()
        .clone();
    Mock::given(method("POST"))
        .and(path("/rpc"))
        .and(body_partial_json(
            json!({"method": "eth_getLogs", "params": [{
                "address": cfg.factory_address, "topics": [cfg.factory_topic0]
            }]}),
        ))
        .respond_with(FakeNode { max_span: 3, logs })
        .mount(&server)
        .await;
    let c = client_with(cfg);
    let (from, to) = (95_917_568, 95_917_590);
    let out = c.factory_logs(from, to).await.unwrap();
    assert_eq!(out.len(), 5);
    assert!(out.windows(2).all(|w| w[0].block <= w[1].block));
    assert_eq!(
        out[0].proxy.as_str(),
        "0xc3964d24c189ef737578715b47aa67b0546c3cfe"
    );
    assert_eq!(
        out[0].owner.as_str(),
        "0x6a7589abd14f5076026fbc15964f638a163b4447"
    );
    assert_eq!(out[0].block, 95_917_570);
    assert_eq!(
        out[0].tx,
        "0x8ee573afc746bf6ed2f504f162c379d74a28d38ea347986f9df5b04ccaf4ac75"
    );

    // Every successful span is within the node's limit and together they cover the range exactly once.
    let mut covered = Vec::new();
    for r in server.received_requests().await.unwrap() {
        let b: Value = serde_json::from_slice(&r.body).unwrap();
        let hex = |v: &Value| {
            u64::from_str_radix(v.as_str().unwrap().trim_start_matches("0x"), 16).unwrap()
        };
        let (f, t) = (
            hex(&b["params"][0]["fromBlock"]),
            hex(&b["params"][0]["toBlock"]),
        );
        if t - f < 3 {
            covered.push((f, t));
        }
    }
    covered.sort();
    assert_eq!(covered.first().unwrap().0, from);
    assert_eq!(covered.last().unwrap().1, to);
    assert!(covered.windows(2).all(|w| w[0].1 + 1 == w[1].0));
}

#[tokio::test]
async fn rpc_block_number_and_node_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_partial_json(json!({"method": "eth_blockNumber"})))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"jsonrpc": "2.0", "id": 1, "result": "0x5b79ae1"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(body_partial_json(json!({"method": "eth_getLogs"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"jsonrpc": "2.0", "id": 1, "error": {"code": -32000, "message": "header not found"}})))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    assert_eq!(c.block_number().await.unwrap(), 95_918_817);
    match c.factory_logs(1, 10).await {
        Err(ApiError::Http { body, .. }) => assert!(body.contains("header not found")),
        other => panic!("{other:?}"),
    }
    assert_eq!(c.stats().requests[&RateClass::Rpc], 2);
}
