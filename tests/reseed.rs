//! Acceptance tests for the reseed path, run against a real Graft process.

mod common;

use std::time::Duration;

use axum::http::StatusCode;
use common::*;
use serde_json::{json, Value};

fn movie() -> Content {
    Content::new("movie", "Movie.2024", &[("movie.mkv", 1_000_000), ("movie.nfo", 100)])
}

struct Setup {
    graft: Graft,
    qb: Qb,
    site_b: MockSite,
    client: String,
}

/// qBittorrent seeds `contents` from site A; site B is a NexusPHP site that may carry them.
async fn setup(contents: &[Content], site_b_extra: Value) -> Setup {
    let qb = Qb::default();
    for c in contents {
        qb.seed(c, "A", "tracker.a.test", "/data/a");
    }
    let qb_addr = qb.start().await;
    let site_b = MockSite::new(Lookup::Open);
    let b_addr = site_b.start().await;
    let graft = Graft::start().await;
    let client = graft.add_qb(qb_addr).await;
    graft.add_site("a", "a.test", None, json!({})).await;
    graft.add_site("b", "b.test", Some(b_addr), site_b_extra).await;
    Setup { graft, qb, site_b, client }
}

async fn execute(s: &Setup, preview_id: &str, ids: &[u64]) -> Value {
    execute_into(s, preview_id, &s.client, ids).await
}

async fn execute_into(s: &Setup, preview_id: &str, target: &str, ids: &[u64]) -> Value {
    let (status, started) = s
        .graft
        .post("/reseed/execute", json!({"preview_id": preview_id, "target_client_id": target, "candidate_ids": ids}))
        .await;
    assert_eq!(status, StatusCode::OK, "{started}");
    s.graft.wait_task(&started).await
}

#[tokio::test]
async fn content_found_on_another_site_is_added_stopped_with_the_tag() {
    let content = movie();
    let s = setup(std::slice::from_ref(&content), json!({})).await;
    let (bytes, b_hash) = s.site_b.publish(&content, "77", "B");
    s.qb.expect_add(&bytes, &b_hash);

    let (preview_id, preview) = s.graft.preview(&s.client, &["b"]).await;
    let candidates = preview["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 1, "{preview}");
    assert_eq!(candidates[0]["target_torrent_id"], "77");
    assert_eq!(candidates[0]["source_site"], "a");
    assert_eq!(candidates[0]["evidence"], "pieces_equal");
    assert_eq!(candidates[0]["needs_confirmation"], false);
    assert_eq!(preview["read"]["recognized"]["a"], 1);

    let done = execute(&s, &preview_id, &[0]).await;
    assert_eq!(done["result"]["success"], 1, "{done}");

    {
        let qb = s.qb.0.lock().unwrap();
        let add = &qb.adds[0];
        assert_eq!(add["stopped"], "true");
        assert_eq!(add["paused"], "true");
        assert_eq!(add["autoTMM"], "false");
        assert_eq!(add["contentLayout"], "Original");
        assert_eq!(add["savepath"], "/data/a");
        assert_eq!(add["tags"], "graft");
        assert!(!add.contains_key("skip_checking"));
        assert!(qb.torrents.iter().any(|t| t.hash == b_hash));
    }

    let history = s.graft.get("/reseed/history").await;
    assert_eq!(history[0]["status"], "success");
    assert_eq!(history[0]["target_torrent_id"], "77");

    // Neither the site passkey nor the one in the client's tracker URLs is ever echoed.
    let sites = s.graft.get("/sites").await;
    for body in [&preview, &done, &history, &sites] {
        let text = body.to_string();
        assert!(!text.contains(PASSKEY) && !text.contains("SECRET"), "{text}");
    }
}

#[tokio::test]
async fn only_confirmed_candidates_are_downloaded_and_added() {
    let contents = [
        movie(),
        Content::new("show", "Show.S01", &[("e01.mkv", 500)]),
        Content::new("album", "Album", &[("01.flac", 300)]),
    ];
    let s = setup(&contents, json!({})).await;
    for (i, c) in contents.iter().enumerate() {
        let (bytes, hash) = s.site_b.publish(c, &format!("{}", 10 + i), "B");
        s.qb.expect_add(&bytes, &hash);
    }

    let (preview_id, preview) = s.graft.preview(&s.client, &["b"]).await;
    assert_eq!(preview["candidates"].as_array().unwrap().len(), 3);

    let done = execute(&s, &preview_id, &[1]).await;
    assert_eq!(done["result"]["success"], 1, "{done}");
    assert_eq!(s.site_b.downloads(), 1);
    assert_eq!(s.qb.0.lock().unwrap().adds.len(), 1);
}

#[tokio::test]
async fn unreadable_piece_hashes_are_reported_and_never_become_candidates() {
    let contents = [movie(), Content::new("show", "Show.S01", &[("e01.mkv", 500)])];
    let s = setup(&contents, json!({})).await;
    for (i, c) in contents.iter().enumerate() {
        s.site_b.publish(c, &format!("{}", 20 + i), "B");
    }
    let broken = s.qb.0.lock().unwrap().torrents[1].hash.clone();
    s.qb.0.lock().unwrap().failing_pieces.insert(broken.clone());

    let (_, preview) = s.graft.preview(&s.client, &["b"]).await;
    let candidates = preview["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 1, "{preview}");
    assert_ne!(candidates[0]["source_hash"], broken.as_str());
    let missing = &preview["read"]["without_pieces"][0];
    assert_eq!(missing["count"], 1);
    assert!(missing["reason"].as_str().unwrap().contains("500"), "{missing}");

    // Piece hashes are cached: a second preview only asks again for the one that failed.
    let calls = s.qb.0.lock().unwrap().piece_hash_calls;
    s.graft.preview(&s.client, &["b"]).await;
    assert_eq!(s.qb.0.lock().unwrap().piece_hash_calls, calls + 1);
}

#[tokio::test]
async fn the_same_content_from_two_sites_is_fetched_and_added_once() {
    let content = movie();
    let s = setup(std::slice::from_ref(&content), json!({})).await;
    s.qb.seed(&content, "C", "tracker.c.test", "/data/c");
    s.graft.add_site("c", "c.test", None, json!({})).await;
    let (bytes, hash) = s.site_b.publish(&content, "77", "B");
    s.qb.expect_add(&bytes, &hash);

    let (preview_id, preview) = s.graft.preview(&s.client, &["b"]).await;
    assert_eq!(preview["candidates"].as_array().unwrap().len(), 1, "{preview}");
    let done = execute(&s, &preview_id, &[0, 0]).await;
    assert_eq!(done["result"]["success"], 1, "{done}");
    assert_eq!(s.site_b.downloads(), 1);
}

#[tokio::test]
async fn a_second_run_into_the_same_client_is_refused_while_one_is_running() {
    let content = movie();
    let s = setup(std::slice::from_ref(&content), json!({})).await;
    let (bytes, hash) = s.site_b.publish(&content, "77", "B");
    s.qb.expect_add(&bytes, &hash);
    s.site_b.0.lock().unwrap().download_delay = Duration::from_millis(1500);

    let (preview_id, _) = s.graft.preview(&s.client, &["b"]).await;
    let body = json!({"preview_id": preview_id, "target_client_id": s.client, "candidate_ids": [0]});
    let (first, started) = s.graft.post("/reseed/execute", body.clone()).await;
    assert_eq!(first, StatusCode::OK);
    let (second, message) = s.graft.post("/reseed/execute", body.clone()).await;
    assert_eq!(second, StatusCode::CONFLICT, "{message}");
    s.graft.wait_task(&started).await;

    // Once the first run is over, running again neither downloads nor adds anything new.
    let (_, rerun) = s.graft.post("/reseed/execute", body).await;
    let rerun = s.graft.wait_task(&rerun).await;
    assert_eq!(rerun["result"]["skipped"], 1, "{rerun}");
    assert_eq!(s.site_b.downloads(), 1);
    assert_eq!(s.qb.0.lock().unwrap().adds.len(), 1);
}

#[tokio::test]
async fn a_different_file_layout_is_listed_but_not_added() {
    let content = movie();
    let s = setup(std::slice::from_ref(&content), json!({})).await;
    let mut renamed = content.clone();
    renamed.name = "Movie 2024 1080p".into();
    s.site_b.publish(&renamed, "77", "B");

    let (preview_id, preview) = s.graft.preview(&s.client, &["b"]).await;
    assert_eq!(preview["candidates"].as_array().unwrap().len(), 1);
    let done = execute(&s, &preview_id, &[0]).await;
    let item = &done["result"]["items"][0];
    assert_eq!(item["status"], "skipped", "{done}");
    assert_eq!(item["step"], "layout");
    assert!(s.qb.0.lock().unwrap().adds.is_empty());
}

#[tokio::test]
async fn a_torrent_from_an_unrecognised_site_needs_its_own_confirmation() {
    let content = movie();
    let qb = Qb::default();
    qb.seed(&content, "X", "tracker.unknown.test", "/data/x");
    let qb_addr = qb.start().await;
    let site_b = MockSite::new(Lookup::Open);
    let b_addr = site_b.start().await;
    let graft = Graft::start().await;
    let client = graft.add_qb(qb_addr).await;
    graft.add_site("b", "b.test", Some(b_addr), json!({})).await;
    let (bytes, hash) = site_b.publish(&content, "77", "B");
    qb.expect_add(&bytes, &hash);

    let (preview_id, preview) = graft.preview(&client, &["b"]).await;
    assert_eq!(preview["candidates"][0]["needs_confirmation"], true);
    assert_eq!(preview["read"]["unrecognized"][0]["reason"], "没有站点认领域名 tracker.unknown.test");

    let body = json!({"preview_id": preview_id, "target_client_id": client, "candidate_ids": [0]});
    let (status, _) = graft.post("/reseed/execute", body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(site_b.downloads(), 0);

    let body = json!({"preview_id": preview_id, "target_client_id": client, "candidate_ids": [0], "confirmed_risky_ids": [0]});
    let (status, started) = graft.post("/reseed/execute", body).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(graft.wait_task(&started).await["result"]["success"], 1);
}

#[tokio::test]
async fn a_disabled_or_unknown_target_site_is_named_in_the_error() {
    let s = setup(&[movie()], json!({"enabled": false})).await;
    let (status, body) = s.graft.post("/reseed/preview", json!({"source_client_id": s.client, "target_site_ids": ["b"]})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("（b）"), "{body}");
    let (status, body) = s.graft.post("/reseed/preview", json!({"source_client_id": s.client, "target_site_ids": ["nope"]})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("nope"), "{body}");
}

#[tokio::test]
async fn sites_without_the_endpoint_or_rejecting_the_passkey_are_reported_per_site() {
    let content = movie();
    let qb = Qb::default();
    qb.seed(&content, "A", "tracker.a.test", "/data/a");
    let qb_addr = qb.start().await;
    let old = MockSite::new(Lookup::NoEndpoint);
    let old_addr = old.start().await;
    let locked = MockSite::new(Lookup::LoginRedirect);
    let locked_addr = locked.start().await;
    let graft = Graft::start().await;
    let client = graft.add_qb(qb_addr).await;
    graft.add_site("old", "old.test", Some(old_addr), json!({})).await;
    graft.add_site("locked", "locked.test", Some(locked_addr), json!({})).await;

    let (_, preview) = graft.preview(&client, &["old", "locked"]).await;
    assert!(preview["candidates"].as_array().unwrap().is_empty());
    let errors: Vec<String> = preview["sites"].as_array().unwrap().iter().map(|s| s["error"].to_string()).collect();
    assert!(errors[0].contains("没有 pieces-hash 接口"), "{errors:?}");
    assert!(errors[1].contains("拒绝了 passkey"), "{errors:?}");
}

#[tokio::test]
async fn a_match_is_added_to_transmission_paused_at_the_source_path_and_only_once() {
    let content = movie();
    let s = setup(std::slice::from_ref(&content), json!({})).await;
    let (bytes, b_hash) = s.site_b.publish(&content, "77", "B");
    let tr = Transmission::default();
    tr.expect_add(&bytes, &b_hash);
    let target = s.graft.add_transmission(tr.start().await).await;

    let (preview_id, preview) = s.graft.preview(&s.client, &["b"]).await;
    assert_eq!(preview["candidates"].as_array().unwrap().len(), 1, "{preview}");
    let first = execute_into(&s, &preview_id, &target, &[0]).await;
    assert_eq!(first["result"]["success"], 1, "{first}");
    let adds = tr.adds();
    assert_eq!(adds.len(), 1);
    assert_eq!(adds[0]["paused"], true);
    assert_eq!(adds[0]["download-dir"], "/data/a");
    assert_eq!(adds[0]["labels"], json!(["graft"]));
    assert_eq!(tr.labels(&b_hash), Some(json!(["graft"])), "3.x only takes labels from torrent-set");

    let again = execute_into(&s, &preview_id, &target, &[0]).await;
    assert_eq!(again["result"]["skipped"], 1, "{again}");
    assert_eq!(again["result"]["items"][0]["step"], "exists", "{again}");
    assert_eq!((tr.adds().len(), s.site_b.downloads()), (1, 1), "the rerun only asks the client");
}

#[tokio::test]
async fn a_transmission_add_whose_label_fails_is_a_success_with_a_warning() {
    let content = movie();
    let s = setup(std::slice::from_ref(&content), json!({})).await;
    let (bytes, b_hash) = s.site_b.publish(&content, "77", "B");
    let tr = Transmission::default();
    tr.expect_add(&bytes, &b_hash);
    tr.0.lock().unwrap().fail_set = true;
    let target = s.graft.add_transmission(tr.start().await).await;

    let (preview_id, _) = s.graft.preview(&s.client, &["b"]).await;
    let done = execute_into(&s, &preview_id, &target, &[0]).await;
    let item = &done["result"]["items"][0];
    assert_eq!(item["status"], "success", "{done}");
    assert!(item["message"].as_str().unwrap().contains("没能打上标签"), "{done}");
}

#[tokio::test]
async fn a_torrent_the_user_adds_to_transmission_meanwhile_keeps_its_labels() {
    let content = movie();
    let s = setup(std::slice::from_ref(&content), json!({})).await;
    let (bytes, b_hash) = s.site_b.publish(&content, "77", "B");
    let tr = Transmission::default();
    tr.expect_add(&bytes, &b_hash);
    tr.0.lock().unwrap().racing.insert(b_hash.clone());
    let target = s.graft.add_transmission(tr.start().await).await;

    let (preview_id, _) = s.graft.preview(&s.client, &["b"]).await;
    let done = execute_into(&s, &preview_id, &target, &[0]).await;
    let item = &done["result"]["items"][0];
    assert_eq!((&item["status"], &item["step"]), (&json!("skipped"), &json!("exists")), "{done}");
    assert_eq!(tr.labels(&b_hash), Some(json!(["mine"])));
}

#[tokio::test]
async fn a_lookup_cut_off_mid_response_is_sent_once_more() {
    let content = movie();
    let qb = Qb::default();
    qb.seed(&content, "A", "tracker.a.test", "/data/a");
    let qb_addr = qb.start().await;
    let flaky = MockSite::new(Lookup::CutOffFirst(1));
    flaky.publish(&content, "7", "B");
    let flaky_addr = flaky.start().await;
    let down = MockSite::new(Lookup::CutOffFirst(2));
    down.publish(&content, "8", "C");
    let down_addr = down.start().await;
    let graft = Graft::start().await;
    let client = graft.add_qb(qb_addr).await;
    graft.add_site("flaky", "flaky.test", Some(flaky_addr), json!({})).await;
    graft.add_site("down", "down.test", Some(down_addr), json!({})).await;

    let (_, preview) = graft.preview(&client, &["flaky", "down"]).await;
    let sites = preview["sites"].as_array().unwrap();
    assert_eq!(sites[0]["error"], Value::Null, "{preview}");
    assert_eq!(sites[0]["found"], 1, "{preview}");
    assert!(sites[1]["error"].as_str().unwrap().contains("连不上站点"), "{preview}");
    assert_eq!((flaky.lookups(), down.lookups()), (2, 2), "one resend, no more");
}

#[tokio::test]
async fn cancelling_keeps_what_was_added_and_a_rerun_only_does_the_rest() {
    let contents = [
        movie(),
        Content::new("show", "Show.S01", &[("e01.mkv", 500)]),
        Content::new("album", "Album", &[("01.flac", 300)]),
    ];
    let s = setup(&contents, json!({})).await;
    for (i, c) in contents.iter().enumerate() {
        let (bytes, hash) = s.site_b.publish(c, &format!("{}", 30 + i), "B");
        s.qb.expect_add(&bytes, &hash);
    }
    s.site_b.0.lock().unwrap().download_delay = Duration::from_secs(2);
    let (preview_id, _) = s.graft.preview(&s.client, &["b"]).await;
    let body = json!({"preview_id": preview_id, "target_client_id": s.client, "candidate_ids": [0, 1, 2]});
    let (_, started) = s.graft.post("/reseed/execute", body.clone()).await;

    // Cancel while the second download is in flight: it finishes, the third never starts.
    for _ in 0..400 {
        if s.site_b.downloads() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let id = started["task_id"].as_str().unwrap();
    s.graft.post(&format!("/tasks/{id}/cancel"), json!({})).await;
    let done = s.graft.wait_task(&started).await;
    assert_eq!(done["status"], "cancelled", "{done}");
    assert_eq!(done["result"]["success"], 2, "{done}");
    assert_eq!(done["result"]["not_attempted"], 1, "{done}");
    assert_eq!(s.qb.0.lock().unwrap().adds.len(), 2);

    s.site_b.0.lock().unwrap().download_delay = Duration::ZERO;
    let (_, rerun) = s.graft.post("/reseed/execute", body).await;
    let rerun = s.graft.wait_task(&rerun).await;
    assert_eq!(rerun["result"]["skipped"], 2, "{rerun}");
    assert_eq!(rerun["result"]["success"], 1, "{rerun}");
    assert_eq!(s.site_b.downloads(), 3);
    assert_eq!(s.qb.0.lock().unwrap().adds.len(), 3);
}

#[cfg(unix)]
#[tokio::test]
async fn sigterm_finishes_the_torrent_in_flight_records_it_and_exits_cleanly() {
    let contents = [
        movie(),
        Content::new("show", "Show.S01", &[("e01.mkv", 500)]),
        Content::new("album", "Album", &[("01.flac", 300)]),
    ];
    let mut s = setup(&contents, json!({})).await;
    for (i, c) in contents.iter().enumerate() {
        let (bytes, hash) = s.site_b.publish(c, &format!("{}", 60 + i), "B");
        s.qb.expect_add(&bytes, &hash);
    }
    s.site_b.0.lock().unwrap().download_delay = Duration::from_secs(2);
    let (preview_id, _) = s.graft.preview(&s.client, &["b"]).await;
    let body = json!({"preview_id": preview_id, "target_client_id": s.client, "candidate_ids": [0, 1, 2]});
    s.graft.post("/reseed/execute", body).await;
    for _ in 0..400 {
        if s.site_b.downloads() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let (status, took) = s.graft.terminate().await;
    assert!(status.success(), "{status}");
    assert!(took < Duration::from_secs(6), "waited {took:?}, not just for the download in flight");
    assert_eq!((s.site_b.downloads(), s.qb.0.lock().unwrap().adds.len()), (2, 2));
    let graft = s.graft.restart(&[]).await;
    let history = graft.get("/reseed/history").await;
    let rows = history.as_array().unwrap();
    assert_eq!(rows.len(), 2, "{history}");
    assert!(rows.iter().all(|r| r["status"] == "success"), "{history}");
}

#[tokio::test]
async fn editing_a_client_keeps_its_password_unless_a_new_one_is_given() {
    let qb = Qb::default();
    let qb_addr = qb.start().await;
    let graft = Graft::start().await;
    let id = graft.add_qb(qb_addr).await;
    let edit = |password: Option<&str>| {
        let mut body = json!({"name": "renamed", "client_type": "qbittorrent", "host": "127.0.0.1",
            "port": qb_addr.port(), "username": "u", "password": ""});
        if let Some(p) = password {
            body["password"] = json!(p);
        }
        body
    };
    let (path, test_path) = (format!("/clients/{id}"), format!("/clients/{id}/test"));
    let test = || graft.post(&test_path, json!({}));

    let (status, body) = graft.call(reqwest::Method::PUT, &path, Some(edit(None))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["name"], "renamed");
    assert_eq!(test().await.1["success"], true, "a blank password keeps the stored one");

    graft.call(reqwest::Method::PUT, &path, Some(edit(Some("wrong")))).await;
    assert_eq!(test().await.1["success"], false, "a new password replaces it");
}

#[tokio::test]
async fn failed_downloads_still_wait_for_the_site_rate_limit() {
    let contents = [movie(), Content::new("show", "Show.S01", &[("e01.mkv", 500)])];
    let s = setup(&contents, json!({"rate_limit_rpm": 60})).await;
    for (i, c) in contents.iter().enumerate() {
        s.site_b.publish(c, &format!("{}", 40 + i), "B");
    }
    s.site_b.0.lock().unwrap().download_status = Some(StatusCode::INTERNAL_SERVER_ERROR);

    let (preview_id, _) = s.graft.preview(&s.client, &["b"]).await;
    let done = execute(&s, &preview_id, &[0, 1]).await;
    assert_eq!(done["result"]["failed"], 2, "{done}");
    let times = s.site_b.0.lock().unwrap().downloads.clone();
    assert_eq!(times.len(), 2);
    assert!(times[1] - times[0] >= Duration::from_millis(950), "{:?}", times[1] - times[0]);
}

#[tokio::test]
async fn the_daily_download_limit_skips_the_rest() {
    let contents = [movie(), Content::new("show", "Show.S01", &[("e01.mkv", 500)])];
    let s = setup(&contents, json!({"daily_limit": 1})).await;
    for (i, c) in contents.iter().enumerate() {
        let (bytes, hash) = s.site_b.publish(c, &format!("{}", 50 + i), "B");
        s.qb.expect_add(&bytes, &hash);
    }
    let (preview_id, _) = s.graft.preview(&s.client, &["b"]).await;
    let done = execute(&s, &preview_id, &[0, 1]).await;
    assert_eq!(done["result"]["success"], 1, "{done}");
    assert_eq!(done["result"]["items"][1]["step"], "daily_limit");
    assert_eq!(s.site_b.downloads(), 1);
}

#[tokio::test]
async fn settings_that_would_be_ignored_stop_the_start() {
    let dir = temp_dir();
    std::fs::write(dir.join("config.toml"), "[reseed]\ndefault_paused = true\n").unwrap();
    let out = graft_command(&dir, 1).output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown field"));

    std::fs::remove_file(dir.join("config.toml")).unwrap();
    let mut cmd = graft_command(&dir, 1);
    let out = cmd.env("GRAFT_PORT", "abc").output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("GRAFT_PORT"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn requests_for_another_host_or_from_another_page_are_refused() {
    let graft = Graft::start().await;
    let http = reqwest::Client::new();
    let root = format!("http://{}/", graft.authority());
    let rebound = http.get(&root).header("Host", "evil.example").send().await.unwrap();
    assert_eq!(rebound.status(), StatusCode::FORBIDDEN);
    let rebound = http.get(format!("{}/clients", graft.base)).header("Host", "evil.example:80").send().await.unwrap();
    assert_eq!(rebound.status(), StatusCode::FORBIDDEN);

    let client = json!({"name": "qb", "client_type": "qbittorrent", "host": "127.0.0.1", "port": 1});
    let from = |origin: String| http.post(format!("{}/clients", graft.base)).header("Origin", origin).json(&client).send();
    assert_eq!(from("http://evil.example".into()).await.unwrap().status(), StatusCode::FORBIDDEN);
    assert_eq!(from("null".into()).await.unwrap().status(), StatusCode::FORBIDDEN);
    assert_eq!(from(format!("http://{}", graft.authority())).await.unwrap().status(), StatusCode::OK);
}

#[tokio::test]
async fn with_a_password_the_api_needs_a_login_that_outlives_restarts_but_not_a_new_password() {
    let graft = Graft::start_in(temp_dir(), &[("GRAFT_PASSWORD", "first")]).await;
    assert_eq!(graft.call(reqwest::Method::GET, "/clients", None).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(graft.get("/auth").await, json!({"required": true, "authenticated": false}));
    let page = reqwest::get(format!("http://{}/", graft.authority())).await.unwrap();
    assert_ne!(page.status(), StatusCode::UNAUTHORIZED, "the page loads so the login form can show");

    assert_eq!(graft.post("/login", json!({"password": "wrong"})).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(graft.post("/login", json!({"password": "first"})).await.0, StatusCode::OK);
    assert_eq!(graft.call(reqwest::Method::GET, "/clients", None).await.0, StatusCode::OK);

    let graft = graft.restart(&[("GRAFT_PASSWORD", "first")]).await;
    assert_eq!(graft.call(reqwest::Method::GET, "/clients", None).await.0, StatusCode::OK);

    let graft = graft.restart(&[("GRAFT_PASSWORD", "second")]).await;
    assert_eq!(graft.call(reqwest::Method::GET, "/clients", None).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(graft.post("/login", json!({"password": "second"})).await.0, StatusCode::OK);
    assert_eq!(graft.post("/logout", json!({})).await.0, StatusCode::OK);
    assert_eq!(graft.call(reqwest::Method::GET, "/clients", None).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn listening_beyond_loopback_without_a_password_is_refused_at_start() {
    // Graft must exit before it binds: binding 0.0.0.0 here would open a port on the network.
    let dir = temp_dir();
    let mut child = graft_command(&dir, 1).env("GRAFT_HOST", "0.0.0.0").spawn().unwrap();
    let mut status = None;
    for _ in 0..100 {
        status = child.try_wait().unwrap();
        if status.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let _ = child.kill();
    let mut stderr = String::new();
    std::io::Read::read_to_string(child.stderr.as_mut().unwrap(), &mut stderr).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(status.is_some_and(|s| !s.success()), "graft kept running");
    assert!(stderr.contains("GRAFT_PASSWORD"), "{stderr}");
}
