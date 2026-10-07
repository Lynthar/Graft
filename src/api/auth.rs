//! Who may use Graft: every request passes `guard`, and when an access password is set
//! the API also needs a login session, kept in the `sessions` table.

use std::time::Duration;

use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde_json::json;

use super::{AppError, AppState};
use crate::config::{is_loopback, Password};
use crate::torrent::sha1_hex;

const COOKIE: &str = "graft_session";
/// A session lasts this long after it was last used.
const SESSION_SECS: i64 = 30 * 24 * 3600;
/// The expiry moves forward at most this often, so polling doesn't write on every request.
const BUMP_SECS: i64 = 3600;

pub struct Access {
    password: Option<Password>,
    /// Held while a password is checked; a wrong one keeps it a second longer, which caps guessing.
    login: tokio::sync::Mutex<()>,
}

impl Access {
    pub fn new(password: Option<Password>) -> Self {
        Self { password, login: tokio::sync::Mutex::new(()) }
    }
}

/// Without a password, only requests addressed to this machine are served: a page that
/// rebinds its own domain to 127.0.0.1 still sends that domain as `Host`. Any request that
/// can change state must come from Graft's own page or from a program, never another site.
pub async fn guard(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let headers = request.headers();
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    if state.access.password.is_none() && !is_loopback(host_name(&host)) {
        return refuse(format!(
            "Graft has no password, so it only answers requests addressed to this machine, not {host:?}. \
             To reach it from elsewhere, including through a reverse proxy, set GRAFT_PASSWORD."
        ));
    }
    let changes_state = !matches!(*request.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if changes_state && !origin_matches(headers, &host) {
        return refuse(
            "Refused a request sent by another site. Behind a reverse proxy, forward the original \
             Host header (nginx: proxy_set_header Host $host)."
                .to_string(),
        );
    }
    if state.access.password.is_none() || !needs_session(request.uri().path()) {
        return next.run(request).await;
    }
    match session(&state, request.headers()) {
        Ok(Some(live)) => {
            let mut response = next.run(request).await;
            if live.extended {
                response.headers_mut().append(header::SET_COOKIE, cookie(&live.token, SESSION_SECS));
            }
            response
        }
        Ok(None) => AppError::new(StatusCode::UNAUTHORIZED, "Log in first").into_response(),
        Err(e) => AppError::from(e).into_response(),
    }
}

/// `GET /api/auth`: whether a password is needed and whether this browser is logged in.
pub async fn status(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, AppError> {
    let required = state.access.password.is_some();
    let live = session(&state, &headers)?;
    let mut response = Json(json!({"required": required, "authenticated": !required || live.is_some()})).into_response();
    if let Some(live) = live.filter(|l| l.extended) {
        response.headers_mut().append(header::SET_COOKIE, cookie(&live.token, SESSION_SECS));
    }
    Ok(response)
}

#[derive(Deserialize)]
pub struct LoginRequest {
    password: String,
}

/// `POST /api/login`: on the right password, starts a session in a cookie.
pub async fn login(State(state): State<AppState>, Json(req): Json<LoginRequest>) -> Result<Response, AppError> {
    let Some(password) = &state.access.password else {
        return Err(AppError::bad_request("No password is set, so there is nothing to log in to"));
    };
    let _turn = state.access.login.lock().await;
    if !same_secret(&req.password, password.expose()) {
        tokio::time::sleep(Duration::from_secs(1)).await;
        return Err(AppError::new(StatusCode::UNAUTHORIZED, "Wrong password"));
    }
    let token = uuid::Uuid::new_v4().simple().to_string();
    let now = chrono::Utc::now().timestamp();
    {
        let conn = state.db.conn();
        conn.execute("DELETE FROM sessions WHERE expires_at <= ?1", [now])?;
        conn.execute(
            "INSERT INTO sessions (token, password_check, expires_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![token, password_check(&token, password), now + SESSION_SECS],
        )?;
    }
    Ok(([(header::SET_COOKIE, cookie(&token, SESSION_SECS))], Json(json!({"ok": true}))).into_response())
}

/// `POST /api/logout`: ends this browser's session.
pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, AppError> {
    if let Some(token) = cookie_token(&headers) {
        state.db.conn().execute("DELETE FROM sessions WHERE token = ?1", [token])?;
    }
    Ok(([(header::SET_COOKIE, cookie("", 0))], Json(json!({"ok": true}))).into_response())
}

struct LiveSession {
    token: String,
    /// The expiry was moved forward, so the cookie should be sent again.
    extended: bool,
}

/// The session named by the request's cookie, if it is unexpired and was started under the
/// current password.
fn session(state: &AppState, headers: &HeaderMap) -> rusqlite::Result<Option<LiveSession>> {
    let (Some(password), Some(token)) = (&state.access.password, cookie_token(headers)) else {
        return Ok(None);
    };
    let conn = state.db.conn();
    let row: Option<(String, i64)> = conn
        .query_row("SELECT password_check, expires_at FROM sessions WHERE token = ?1", [&token], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?;
    let now = chrono::Utc::now().timestamp();
    let Some((check, expires_at)) = row else { return Ok(None) };
    if expires_at <= now || !same_secret(&check, &password_check(&token, password)) {
        return Ok(None);
    }
    let extended = expires_at < now + SESSION_SECS - BUMP_SECS;
    if extended {
        conn.execute("UPDATE sessions SET expires_at = ?1 WHERE token = ?2", rusqlite::params![now + SESSION_SECS, token])?;
    }
    Ok(Some(LiveSession { token, extended }))
}

fn needs_session(path: &str) -> bool {
    (path == "/api" || path.starts_with("/api/")) && !matches!(path, "/api/health" | "/api/auth" | "/api/login")
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|pair| pair.trim().strip_prefix(COOKIE)?.strip_prefix('='))
        .filter(|token| !token.is_empty())
        .map(str::to_string)
}

fn cookie(token: &str, max_age: i64) -> HeaderValue {
    HeaderValue::try_from(format!("{COOKIE}={token}; Max-Age={max_age}; Path=/; HttpOnly; SameSite=Strict"))
        .expect("the token is hex")
}

fn password_check(token: &str, password: &Password) -> String {
    sha1_hex(format!("{token}\n{}", password.expose()).as_bytes())
}

/// Compares digests byte by byte without stopping early, so timing says nothing about how
/// much of a guess was right.
fn same_secret(a: &str, b: &str) -> bool {
    let (a, b) = (sha1_smol::Sha1::from(a).digest().bytes(), sha1_smol::Sha1::from(b).digest().bytes());
    a.iter().zip(b.iter()).fold(0, |diff, (x, y)| diff | (x ^ y)) == 0
}

/// A browser names the page that sent a request in `Origin`; it must be Graft's own.
/// Requests without one come from programs, which no other page can make the browser send.
fn origin_matches(headers: &HeaderMap, host: &str) -> bool {
    match headers.get(header::ORIGIN).map(|v| v.to_str()) {
        None => true,
        Some(Ok(origin)) => origin.split_once("://").is_some_and(|(_, authority)| authority.eq_ignore_ascii_case(host)),
        Some(Err(_)) => false,
    }
}

/// The name part of a `Host` header: `[::1]:3000` gives `::1`, `nas:3000` gives `nas`.
fn host_name(host: &str) -> &str {
    match host.strip_prefix('[') {
        Some(rest) => rest.split(']').next().unwrap_or(""),
        None => host.rsplit_once(':').map_or(host, |(name, _)| name),
    }
}

fn refuse(message: String) -> Response {
    (StatusCode::FORBIDDEN, message).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_names_lose_their_port() {
        assert_eq!(host_name("127.0.0.1:3000"), "127.0.0.1");
        assert_eq!(host_name("[::1]:3000"), "::1");
        assert_eq!(host_name("localhost"), "localhost");
        assert!(!is_loopback(host_name("evil.example:3000")));
    }

    #[test]
    fn only_graft_own_origin_or_none_may_change_state() {
        let with = |origin: &str| {
            let mut headers = HeaderMap::new();
            headers.insert(header::ORIGIN, HeaderValue::from_str(origin).unwrap());
            headers
        };
        assert!(origin_matches(&HeaderMap::new(), "127.0.0.1:3000"));
        assert!(origin_matches(&with("http://127.0.0.1:3000"), "127.0.0.1:3000"));
        assert!(!origin_matches(&with("http://evil.example"), "127.0.0.1:3000"));
        assert!(!origin_matches(&with("null"), "127.0.0.1:3000"));
    }

    #[test]
    fn the_session_cookie_is_found_among_others() {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_static("a=1; graft_session=abc; b=2"));
        assert_eq!(cookie_token(&headers).as_deref(), Some("abc"));
        headers.insert(header::COOKIE, HeaderValue::from_static("graft_session_old=x"));
        assert_eq!(cookie_token(&headers), None);
    }
}
