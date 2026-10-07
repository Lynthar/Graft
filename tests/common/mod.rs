//! Black-box harness: a real Graft process against mock qBittorrent and NexusPHP servers.

#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use serde_json::{json, Value};

pub const PASSKEY: &str = "0123456789abcdef0123456789abcdef";

pub fn sha1_hex(bytes: &[u8]) -> String {
    sha1_smol::Sha1::from(bytes).digest().to_string()
}

/// Content that several sites may carry, each under its own info hash.
#[derive(Clone)]
pub struct Content {
    pub name: String,
    pub files: Vec<(String, u64)>,
    pub pieces: Vec<u8>,
}

impl Content {
    pub fn new(seed: &str, name: &str, files: &[(&str, u64)]) -> Self {
        let pieces = sha1_smol::Sha1::from(seed.as_bytes()).digest().bytes().to_vec();
        Content {
            name: name.into(),
            files: files.iter().map(|(p, l)| (p.to_string(), *l)).collect(),
            pieces,
        }
    }

    pub fn pieces_hash(&self) -> String {
        sha1_hex(&self.pieces)
    }

    /// The `.torrent` a site serves; `source` changes the info hash, as sites do.
    pub fn torrent(&self, source: &str) -> (Vec<u8>, String) {
        self.torrent_announcing("http://t/a.php", source)
    }

    pub fn torrent_announcing(&self, announce: &str, source: &str) -> (Vec<u8>, String) {
        let mut info = b"d5:filesl".to_vec();
        for (path, len) in &self.files {
            info.extend(format!("d6:lengthi{len}e4:pathl").bytes());
            for part in path.split('/') {
                info.extend(format!("{}:{}", part.len(), part).bytes());
            }
            info.extend(b"ee");
        }
        info.extend(format!("e4:name{}:{}12:piece lengthi16384e6:pieces{}:", self.name.len(), self.name, self.pieces.len()).bytes());
        info.extend(&self.pieces);
        info.extend(format!("6:source{}:{}e", source.len(), source).bytes());
        let mut bytes = format!("d8:announce{}:{announce}4:info", announce.len()).into_bytes();
        bytes.extend(&info);
        bytes.push(b'e');
        (bytes, sha1_hex(&info))
    }

    /// Files as qBittorrent reports them for this content on disk.
    pub fn on_disk(&self) -> Vec<(String, u64)> {
        self.files.iter().map(|(p, l)| (format!("{}/{p}", self.name), *l)).collect()
    }
}

pub async fn serve(router: Router) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    addr
}

// ---------- mock qBittorrent ----------

#[derive(Clone)]
pub struct QbTorrent {
    pub hash: String,
    pub name: String,
    pub size: u64,
    pub progress: f64,
    pub save_path: String,
    pub tracker: String,
    pub pieces: Vec<u8>,
    pub files: Vec<(String, u64)>,
}

#[derive(Default)]
pub struct QbState {
    pub torrents: Vec<QbTorrent>,
    pub adds: Vec<HashMap<String, String>>,
    pub piece_hash_calls: usize,
    pub failing_pieces: HashSet<String>,
    /// Info hash for each `.torrent` the mock may be handed.
    pub known: HashMap<Vec<u8>, String>,
}

#[derive(Clone, Default)]
pub struct Qb(pub Arc<Mutex<QbState>>);

impl Qb {
    pub fn seed(&self, content: &Content, source: &str, tracker_host: &str, save_path: &str) -> String {
        let (bytes, hash) = content.torrent(source);
        let mut s = self.0.lock().unwrap();
        s.known.insert(bytes, hash.clone());
        s.torrents.push(QbTorrent {
            hash: hash.clone(),
            name: content.name.clone(),
            size: content.files.iter().map(|f| f.1).sum(),
            progress: 1.0,
            save_path: save_path.into(),
            tracker: if tracker_host.is_empty() { String::new() } else { format!("https://{tracker_host}/announce.php?passkey=SECRET") },
            pieces: content.pieces.clone(),
            files: content.on_disk(),
        });
        hash
    }

    pub fn expect_add(&self, bytes: &[u8], hash: &str) {
        self.0.lock().unwrap().known.insert(bytes.to_vec(), hash.into());
    }

    pub async fn start(&self) -> SocketAddr {
        let router = Router::new()
            .route("/api/v2/auth/login", post(qb_login))
            .route("/api/v2/app/version", get(|| async { "v5.2.4" }))
            .route("/api/v2/torrents/info", get(qb_info))
            .route("/api/v2/torrents/pieceHashes", get(qb_pieces))
            .route("/api/v2/torrents/files", get(qb_files))
            .route("/api/v2/torrents/add", post(qb_add))
            .with_state(self.clone());
        serve(router).await
    }
}

fn logged_in(headers: &HeaderMap) -> bool {
    headers.get(header::COOKIE).and_then(|v| v.to_str().ok()).is_some_and(|c| c.contains("SID=ok"))
}

async fn qb_login(body: String) -> Response {
    if body.contains("username=u") && body.contains("password=p") {
        ([(header::SET_COOKIE, "SID=ok; path=/")], "Ok.").into_response()
    } else {
        "Fails.".into_response()
    }
}

async fn qb_info(State(qb): State<Qb>, headers: HeaderMap, Query(q): Query<HashMap<String, String>>) -> Response {
    if !logged_in(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let s = qb.0.lock().unwrap();
    let list: Vec<Value> = s
        .torrents
        .iter()
        .filter(|t| q.get("hashes").is_none_or(|h| h.split('|').any(|h| h == t.hash)))
        .map(|t| json!({"hash": t.hash, "name": t.name, "total_size": t.size, "size": 0, "progress": t.progress, "save_path": t.save_path, "tracker": t.tracker}))
        .collect();
    axum::Json(list).into_response()
}

async fn qb_pieces(State(qb): State<Qb>, headers: HeaderMap, Query(q): Query<HashMap<String, String>>) -> Response {
    if !logged_in(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut s = qb.0.lock().unwrap();
    s.piece_hash_calls += 1;
    let hash = q.get("hash").cloned().unwrap_or_default();
    if s.failing_pieces.contains(&hash) {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    match s.torrents.iter().find(|t| t.hash == hash) {
        Some(t) => axum::Json(t.pieces.chunks(20).map(|c| c.iter().map(|b| format!("{b:02x}")).collect::<String>()).collect::<Vec<_>>()).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn qb_files(State(qb): State<Qb>, headers: HeaderMap, Query(q): Query<HashMap<String, String>>) -> Response {
    if !logged_in(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let s = qb.0.lock().unwrap();
    match s.torrents.iter().find(|t| Some(&t.hash) == q.get("hash")) {
        Some(t) => axum::Json(t.files.iter().map(|(n, l)| json!({"name": n, "size": l})).collect::<Vec<_>>()).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn qb_add(State(qb): State<Qb>, headers: HeaderMap, body: Bytes) -> Response {
    if !logged_in(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let content_type = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
    let boundary = content_type.split("boundary=").nth(1).unwrap_or("").to_string();
    let (fields, file) = parse_multipart(&body, &boundary);
    let mut s = qb.0.lock().unwrap();
    s.adds.push(fields.clone());
    let Some(hash) = file.and_then(|f| s.known.get(&f).cloned()) else {
        return (StatusCode::UNSUPPORTED_MEDIA_TYPE, "not a torrent").into_response();
    };
    if s.torrents.iter().any(|t| t.hash == hash) {
        return StatusCode::CONFLICT.into_response();
    }
    s.torrents.push(QbTorrent {
        hash: hash.clone(),
        name: "added".into(),
        size: 0,
        progress: 0.0,
        save_path: fields.get("savepath").cloned().unwrap_or_default(),
        tracker: String::new(),
        pieces: Vec::new(),
        files: Vec::new(),
    });
    axum::Json(json!({"success_count": 1, "failure_count": 0, "pending_count": 0, "added_torrent_ids": [hash]})).into_response()
}

fn parse_multipart(body: &[u8], boundary: &str) -> (HashMap<String, String>, Option<Vec<u8>>) {
    let delimiter = format!("--{boundary}").into_bytes();
    let mut fields = HashMap::new();
    let mut file = None;
    let mut rest = body;
    while let Some(start) = find(rest, &delimiter) {
        rest = &rest[start + delimiter.len()..];
        let Some(head_end) = find(rest, b"\r\n\r\n") else { break };
        let head = String::from_utf8_lossy(&rest[..head_end]).to_string();
        let content = &rest[head_end + 4..];
        let end = find(content, &delimiter).unwrap_or(content.len());
        let value = content[..end].strip_suffix(b"\r\n").unwrap_or(&content[..end]);
        let name = head.split("name=\"").nth(1).and_then(|n| n.split('"').next()).unwrap_or("").to_string();
        if name == "torrents" {
            file = Some(value.to_vec());
        } else if !name.is_empty() {
            fields.insert(name, String::from_utf8_lossy(value).to_string());
        }
    }
    (fields, file)
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

// ---------- mock Transmission ----------

const TR_SESSION: &str = "mock-session";

#[derive(Default)]
pub struct TrState {
    /// Info hashes of the torrents the client has.
    pub torrents: HashSet<String>,
    /// The arguments of every `torrent-add` received.
    pub adds: Vec<Value>,
    /// Info hash for each `.torrent` the mock may be handed.
    pub known: HashMap<Vec<u8>, String>,
    /// Labels per info hash, as `torrent-set` left them.
    pub labels: HashMap<String, Value>,
    /// Torrents the user adds, labelled `mine`, just before Graft's `torrent-add` lands.
    pub racing: HashSet<String>,
    /// `torrent-set` answers with an error.
    pub fail_set: bool,
}

#[derive(Clone, Default)]
pub struct Transmission(pub Arc<Mutex<TrState>>);

impl Transmission {
    pub fn expect_add(&self, bytes: &[u8], hash: &str) {
        self.0.lock().unwrap().known.insert(bytes.to_vec(), hash.into());
    }

    pub fn adds(&self) -> Vec<Value> {
        self.0.lock().unwrap().adds.clone()
    }

    pub fn labels(&self, hash: &str) -> Option<Value> {
        self.0.lock().unwrap().labels.get(hash).cloned()
    }

    pub async fn start(&self) -> SocketAddr {
        serve(Router::new().route("/transmission/rpc", post(tr_rpc)).with_state(self.clone())).await
    }
}

/// The pre-4.1 RPC protocol: a request without the session id gets a 409 that hands it out.
/// Like 3.x, `torrent-add` ignores `labels`; only `torrent-set` applies them.
async fn tr_rpc(State(tr): State<Transmission>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    if headers.get("X-Transmission-Session-Id").and_then(|v| v.to_str().ok()) != Some(TR_SESSION) {
        return (StatusCode::CONFLICT, [("X-Transmission-Session-Id", TR_SESSION)]).into_response();
    }
    let args = &body["arguments"];
    let mut s = tr.0.lock().unwrap();
    let arguments = match body["method"].as_str() {
        Some("torrent-add") => {
            s.adds.push(args.clone());
            let bytes = base64::engine::general_purpose::STANDARD.decode(args["metainfo"].as_str().unwrap()).unwrap();
            let Some(hash) = s.known.get(&bytes).cloned() else {
                return Json(json!({"result": "invalid or corrupt torrent file"})).into_response();
            };
            if s.racing.remove(&hash) {
                s.torrents.insert(hash.clone());
                s.labels.insert(hash.clone(), json!(["mine"]));
            }
            let key = if s.torrents.insert(hash.clone()) { "torrent-added" } else { "torrent-duplicate" };
            json!({key: {"id": 1, "name": "x", "hashString": hash}})
        }
        Some("torrent-get") => {
            let ids: Vec<&str> = args["ids"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
            let found: Vec<Value> = ids.iter().filter(|h| s.torrents.contains(**h)).map(|h| json!({"hashString": h})).collect();
            json!({"torrents": found})
        }
        Some("torrent-set") if s.fail_set => return Json(json!({"result": "labels failed"})).into_response(),
        Some("torrent-set") => {
            for id in args["ids"].as_array().unwrap().iter().filter_map(Value::as_str) {
                if s.torrents.contains(id) {
                    s.labels.insert(id.to_string(), args["labels"].clone());
                }
            }
            json!({})
        }
        _ => return Json(json!({"result": "method name not recognized"})).into_response(),
    };
    Json(json!({"result": "success", "arguments": arguments})).into_response()
}

// ---------- mock NexusPHP site ----------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Lookup {
    Open,
    NoEndpoint,
    LoginRedirect,
    /// The first n lookups break off mid-response, then the site answers normally.
    CutOffFirst(usize),
}

pub struct SiteState {
    pub lookup: Lookup,
    pub index: HashMap<String, String>,
    pub torrents: HashMap<String, Vec<u8>>,
    pub lookups: usize,
    pub downloads: Vec<Instant>,
    pub download_status: Option<StatusCode>,
    pub download_delay: Duration,
}

#[derive(Clone)]
pub struct MockSite(pub Arc<Mutex<SiteState>>);

impl MockSite {
    pub fn new(lookup: Lookup) -> Self {
        MockSite(Arc::new(Mutex::new(SiteState {
            lookup,
            index: HashMap::new(),
            torrents: HashMap::new(),
            lookups: 0,
            downloads: Vec::new(),
            download_status: None,
            download_delay: Duration::ZERO,
        })))
    }

    /// Publish `content` as torrent `id`; returns the info hash the site's file has.
    pub fn publish(&self, content: &Content, id: &str, source: &str) -> (Vec<u8>, String) {
        let (bytes, hash) = content.torrent(source);
        let mut s = self.0.lock().unwrap();
        s.index.insert(content.pieces_hash(), id.into());
        s.torrents.insert(id.into(), bytes.clone());
        (bytes, hash)
    }

    pub fn downloads(&self) -> usize {
        self.0.lock().unwrap().downloads.len()
    }

    pub fn lookups(&self) -> usize {
        self.0.lock().unwrap().lookups
    }

    pub async fn start(&self) -> SocketAddr {
        let router = Router::new()
            .route("/api/pieces-hash", post(site_lookup))
            .route("/download.php", get(site_download))
            .with_state(self.clone());
        serve(router).await
    }
}

async fn site_lookup(State(site): State<MockSite>, headers: HeaderMap, body: String) -> Response {
    let mut s = site.0.lock().unwrap();
    s.lookups += 1;
    match s.lookup {
        Lookup::NoEndpoint => return (StatusCode::NOT_FOUND, [(header::CONTENT_TYPE, "text/html")], "<html>404</html>").into_response(),
        Lookup::LoginRedirect => return (StatusCode::FOUND, [(header::LOCATION, "/login.php?returnto=x")]).into_response(),
        // Promising more bytes than are sent makes the server drop the connection mid-body;
        // the body must be a stream, or hyper asserts the two lengths agree.
        Lookup::CutOffFirst(n) if s.lookups <= n => {
            let body = Body::from_stream(Body::from("{").into_data_stream());
            return ([(header::CONTENT_LENGTH, "1000")], body).into_response();
        }
        Lookup::CutOffFirst(_) | Lookup::Open => {}
    }
    let pairs: Vec<(String, String)> = url::form_urlencoded::parse(body.as_bytes()).into_owned().collect();
    let passkey = pairs.iter().find(|(k, _)| k == "passkey").map(|(_, v)| v.as_str());
    let wants_json = headers.get(header::ACCEPT).and_then(|v| v.to_str().ok()).is_some_and(|a| a.contains("json"));
    if passkey != Some(PASSKEY) {
        return if wants_json {
            (StatusCode::UNAUTHORIZED, axum::Json(json!({"message": "Unauthenticated."}))).into_response()
        } else {
            (StatusCode::FOUND, [(header::LOCATION, "/login.php")]).into_response()
        };
    }
    let hashes: Vec<&String> = pairs.iter().filter(|(k, _)| k == "pieces_hash[]").map(|(_, v)| v).collect();
    if hashes.len() > 100 {
        return (StatusCode::INTERNAL_SERVER_ERROR, "too many").into_response();
    }
    let data: serde_json::Map<String, Value> = hashes
        .into_iter()
        .filter_map(|h| s.index.get(h).map(|id| (h.clone(), json!(id.parse::<u64>().unwrap()))))
        .collect();
    axum::Json(json!({"ret": 0, "msg": "torrent.querybypieceshash", "data": data})).into_response()
}

async fn site_download(State(site): State<MockSite>, Query(q): Query<HashMap<String, String>>) -> Response {
    let (delay, status) = {
        let mut s = site.0.lock().unwrap();
        s.downloads.push(Instant::now());
        (s.download_delay, s.download_status)
    };
    tokio::time::sleep(delay).await;
    if let Some(status) = status {
        return status.into_response();
    }
    if q.get("passkey").map(String::as_str) != Some(PASSKEY) {
        return (StatusCode::OK, [(header::CONTENT_TYPE, "text/html")], "<html>login</html>").into_response();
    }
    let s = site.0.lock().unwrap();
    match q.get("id").and_then(|id| s.torrents.get(id)) {
        Some(bytes) => ([(header::CONTENT_TYPE, "application/x-bittorrent")], bytes.clone()).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

// ---------- the Graft process ----------

static INSTANCE: AtomicUsize = AtomicUsize::new(0);

pub struct Graft {
    child: Child,
    dir: PathBuf,
    pub base: String,
    http: reqwest::Client,
}

impl Drop for Graft {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        // `restart` hands the directory on and leaves this one empty.
        if !self.dir.as_os_str().is_empty() {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

pub fn temp_dir() -> PathBuf {
    let n = INSTANCE.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("graft-it-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn graft_command(dir: &PathBuf, port: u16) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_graft"));
    cmd.current_dir(dir)
        .env("GRAFT_HOST", "127.0.0.1")
        .env("GRAFT_PORT", port.to_string())
        .env("GRAFT_DATA_DIR", dir.join("data"))
        .env("RUST_LOG", "graft=warn")
        .env_remove("GRAFT_PASSWORD")
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    cmd
}

impl Graft {
    pub async fn start() -> Graft {
        Self::start_in(temp_dir(), &[]).await
    }

    /// Start on `dir` with extra environment variables.
    pub async fn start_in(dir: PathBuf, env: &[(&str, &str)]) -> Graft {
        let http = reqwest::Client::builder().cookie_store(true).build().unwrap();
        let mut stderr = String::new();
        // The port is free when picked, but another socket can take it before graft binds it.
        for _ in 0..5 {
            let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
            let mut child = graft_command(&dir, port).envs(env.iter().copied()).spawn().unwrap();
            let base = format!("http://127.0.0.1:{port}/api");
            for _ in 0..100 {
                let health = http.get(format!("{base}/health")).send().await;
                if health.is_ok_and(|r| r.status().is_success()) {
                    return Graft { child, dir, base, http };
                }
                if child.try_wait().unwrap().is_some() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            let _ = child.kill();
            let _ = child.wait();
            stderr.clear();
            std::io::Read::read_to_string(child.stderr.as_mut().unwrap(), &mut stderr).unwrap();
        }
        panic!("graft did not start:\n{stderr}");
    }

    /// Stop this instance and start another on the same data directory, keeping cookies.
    pub async fn restart(mut self, env: &[(&str, &str)]) -> Graft {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let dir = std::mem::take(&mut self.dir);
        let http = self.http.clone();
        drop(self);
        let mut next = Self::start_in(dir, env).await;
        next.http = http;
        next
    }

    /// Send SIGTERM, as `docker stop` does, and wait for the process to exit.
    #[cfg(unix)]
    pub async fn terminate(&mut self) -> (std::process::ExitStatus, Duration) {
        let started = Instant::now();
        let pid = self.child.id().to_string();
        assert!(Command::new("kill").args(["-TERM", &pid]).status().unwrap().success());
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return (status, started.elapsed());
            }
            assert!(started.elapsed() < Duration::from_secs(20), "graft ignored SIGTERM");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// `host:port` this instance listens on.
    pub fn authority(&self) -> &str {
        self.base.trim_start_matches("http://").trim_end_matches("/api")
    }

    pub async fn call(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut req = self.http.request(method, format!("{}{path}", self.base));
        if let Some(b) = body {
            req = req.json(&b);
        }
        let res = req.send().await.unwrap();
        let status = StatusCode::from_u16(res.status().as_u16()).unwrap();
        (status, res.json().await.unwrap_or(Value::Null))
    }

    pub async fn post(&self, path: &str, body: Value) -> (StatusCode, Value) {
        self.call(reqwest::Method::POST, path, Some(body)).await
    }

    pub async fn get(&self, path: &str) -> Value {
        let (status, body) = self.call(reqwest::Method::GET, path, None).await;
        assert_eq!(status, StatusCode::OK, "GET {path}: {body}");
        body
    }

    pub async fn add_qb(&self, addr: SocketAddr) -> String {
        let (status, body) = self
            .post("/clients", json!({"name": "qb", "client_type": "qbittorrent", "host": "127.0.0.1", "port": addr.port(), "username": "u", "password": "p"}))
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["id"].as_str().unwrap().to_string()
    }

    pub async fn add_transmission(&self, addr: SocketAddr) -> String {
        let (status, body) = self
            .post("/clients", json!({"name": "tr", "client_type": "transmission", "host": "127.0.0.1", "port": addr.port()}))
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["id"].as_str().unwrap().to_string()
    }

    /// A custom NexusPHP site; `addr` None means the site is only used for recognition.
    pub async fn add_site(&self, id: &str, domain: &str, addr: Option<SocketAddr>, extra: Value) -> Value {
        let base_url = match addr {
            Some(a) => format!("http://127.0.0.1:{}", a.port()),
            None => format!("https://{domain}"),
        };
        let mut body = json!({"id": id, "name": id.to_uppercase(), "base_url": base_url, "passkey": PASSKEY, "domains": [domain], "rate_limit_rpm": 60});
        for (k, v) in extra.as_object().unwrap() {
            body[k] = v.clone();
        }
        let (status, body) = self.post("/sites", body).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    /// Run a task to completion and return its final snapshot.
    pub async fn wait_task(&self, started: &Value) -> Value {
        let id = started["task_id"].as_str().unwrap_or_else(|| panic!("no task id in {started}"));
        for _ in 0..600 {
            let snap = self.get(&format!("/tasks/{id}")).await;
            if snap["status"] != "running" {
                return snap;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("task {id} did not finish");
    }

    /// Upload `.torrent` files to match against `client`; returns the preview id and result.
    pub async fn import(&self, client: &str, files: &[(&str, &[u8])]) -> (String, Value) {
        let files: Vec<Value> = files
            .iter()
            .map(|(name, bytes)| json!({"name": name, "data": base64::engine::general_purpose::STANDARD.encode(bytes)}))
            .collect();
        let (status, started) = self.post("/reseed/import", json!({"source_client_id": client, "files": files})).await;
        assert_eq!(status, StatusCode::OK, "{started}");
        let snap = self.wait_task(&started).await;
        assert_eq!(snap["status"], "done", "{snap}");
        (started["task_id"].as_str().unwrap().to_string(), snap["result"].clone())
    }

    pub async fn preview(&self, client: &str, sites: &[&str]) -> (String, Value) {
        let (status, started) = self.post("/reseed/preview", json!({"source_client_id": client, "target_site_ids": sites})).await;
        assert_eq!(status, StatusCode::OK, "{started}");
        let snap = self.wait_task(&started).await;
        assert_eq!(snap["status"], "done", "{snap}");
        (started["task_id"].as_str().unwrap().to_string(), snap["result"].clone())
    }
}
