//! `GitHubClient` against a wiremock GitHub.

use std::time::Duration;

use alerts::{GitHubApi, GitHubClient, IssueUpdate};
use serde_json::{json, Value};
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const REPO: &str = "owner/repo";

fn client(server: &MockServer) -> GitHubClient {
    GitHubClient::with_base_url("tok123".into(), REPO.into(), server.uri())
        .with_write_delay(Duration::ZERO)
}

fn issue_json(n: u64, pr: bool) -> Value {
    let mut v = json!({
        "number": n,
        "title": format!("issue {n}"),
        "state": "open",
        "labels": [{ "id": 1, "name": "insider-candidate", "color": "d93f0b" }],
        "body": if n.is_multiple_of(2) { Value::Null } else { json!(format!("body {n}")) },
    });
    if pr {
        v["pull_request"] = json!({ "url": "https://api.github.com/repos/owner/repo/pulls/1" });
    }
    v
}

#[tokio::test]
async fn create_issue_sends_headers_and_payload() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/repos/owner/repo/issues"))
        .and(header("authorization", "Bearer tok123"))
        .and(header("accept", "application/vnd.github+json"))
        .and(header("x-github-api-version", "2022-11-28"))
        .and(header("user-agent", "polywatch"))
        .and(body_json(
            json!({ "title": "t", "body": "b", "labels": ["x", "tier:high"] }),
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "number": 42 })))
        .expect(1)
        .mount(&server)
        .await;
    let n = client(&server)
        .create_issue("t", "b", &["x".into(), "tier:high".into()])
        .await
        .unwrap();
    assert_eq!(n, 42);
}

#[tokio::test]
async fn update_issue_sends_only_set_fields_and_comment_posts() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/repos/owner/repo/issues/7"))
        .and(body_json(
            json!({ "labels": ["a", "legacy"], "state": "closed" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "number": 7 })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/repos/owner/repo/issues/7/comments"))
        .and(body_json(json!({ "body": "hello" })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "id": 1 })))
        .expect(1)
        .mount(&server)
        .await;
    let gh = client(&server);
    let u = IssueUpdate {
        labels: Some(vec!["a".into(), "legacy".into()]),
        state: Some("closed".into()),
        ..Default::default()
    };
    gh.update_issue(7, &u).await.unwrap();
    gh.comment(7, "hello").await.unwrap();
}

#[tokio::test]
async fn list_issues_paginates_and_skips_prs() {
    let server = MockServer::start().await;
    let page1: Vec<Value> = (1..=100).map(|n| issue_json(n, n == 5)).collect();
    let page2: Vec<Value> = (101..=103).map(|n| issue_json(n, n == 102)).collect();
    for (page, items) in [("1", page1), ("2", page2)] {
        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/issues"))
            .and(query_param("labels", "tier:high"))
            .and(query_param("state", "all"))
            .and(query_param("per_page", "100"))
            .and(query_param("page", page))
            .respond_with(ResponseTemplate::new(200).set_body_json(items))
            .expect(1)
            .mount(&server)
            .await;
    }
    let issues = client(&server)
        .list_issues("tier:high", "all")
        .await
        .unwrap();
    assert_eq!(issues.len(), 101);
    assert!(issues.iter().all(|i| i.number != 5 && i.number != 102));
    assert_eq!(issues[0].labels, vec!["insider-candidate"]);
    assert_eq!(issues[0].body, "body 1");
    assert_eq!(issues[1].body, "", "null body");
    assert_eq!(issues.last().unwrap().number, 103);
}

#[tokio::test]
async fn ensure_label_accepts_already_exists() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/repos/owner/repo/labels"))
        .and(body_json(
            json!({ "name": "legacy", "color": "cccccc", "description": "d" }),
        ))
        .respond_with(ResponseTemplate::new(422).set_body_json(json!({
            "message": "Validation Failed",
            "errors": [{ "resource": "Label", "code": "already_exists", "field": "name" }],
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/repos/owner/repo/labels"))
        .and(body_json(
            json!({ "name": "new", "color": "c5def5", "description": "d" }),
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "name": "new" })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/repos/owner/repo/labels"))
        .and(body_json(
            json!({ "name": "bad", "color": "zz", "description": "d" }),
        ))
        .respond_with(ResponseTemplate::new(422).set_body_json(json!({
            "message": "Validation Failed",
            "errors": [{ "resource": "Label", "code": "invalid", "field": "color" }],
        })))
        .mount(&server)
        .await;
    let gh = client(&server);
    gh.ensure_label("legacy", "cccccc", "d").await.unwrap();
    gh.ensure_label("new", "c5def5", "d").await.unwrap();
    assert!(gh.ensure_label("bad", "zz", "d").await.is_err());
}

#[tokio::test]
async fn retries_secondary_rate_limit_with_retry_after() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/repos/owner/repo/issues/3/comments"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("retry-after", "1")
                .set_body_json(json!({ "message": "You have exceeded a secondary rate limit." })),
        )
        .up_to_n_times(2)
        .with_priority(1)
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/repos/owner/repo/issues/3/comments"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "id": 9 })))
        .with_priority(2)
        .expect(1)
        .mount(&server)
        .await;
    let t = std::time::Instant::now();
    client(&server).comment(3, "x").await.unwrap();
    assert!(
        t.elapsed() >= Duration::from_secs(2),
        "waited Retry-After twice"
    );
}

#[tokio::test]
async fn gives_up_after_three_retries_and_on_plain_403() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/repos/owner/repo/issues/1/comments"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
        .expect(4)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/repos/owner/repo/issues/2/comments"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(json!({ "message": "Resource not accessible by integration" })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let gh = client(&server);
    let err = gh.comment(1, "x").await.unwrap_err().to_string();
    assert!(err.contains("429"), "{err}");
    let err = gh.comment(2, "x").await.unwrap_err().to_string();
    assert!(err.contains("not accessible"), "{err}");
}

#[tokio::test]
async fn long_rate_limit_wait_fails_fast() {
    let server = MockServer::start().await;
    let reset = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3000;
    Mock::given(method("GET"))
        .and(path("/repos/owner/repo/issues"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", reset.to_string().as_str()),
        )
        .expect(1)
        .mount(&server)
        .await;
    let err = client(&server)
        .list_issues("x", "open")
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("rate limited"), "{err}");
}

#[tokio::test]
async fn spaces_mutating_requests() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/repos/owner/repo/issues/1/comments"))
        .respond_with(move |_: &Request| {
            ResponseTemplate::new(201).set_body_json(json!({ "id": 1 }))
        })
        .expect(3)
        .mount(&server)
        .await;
    let gh = GitHubClient::with_base_url("t".into(), REPO.into(), format!("{}/", server.uri()))
        .with_write_delay(Duration::from_millis(200));
    let t = std::time::Instant::now();
    for _ in 0..3 {
        gh.comment(1, "x").await.unwrap();
    }
    assert!(t.elapsed() >= Duration::from_millis(400));
}
