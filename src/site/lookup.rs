//! NexusPHP `POST /api/pieces-hash`: which of these contents does the site carry?

use std::collections::HashMap;

use reqwest::StatusCode;
use serde::Deserialize;

use super::Site;

/// Most pieces hashes the endpoint accepts per request.
pub const MAX_BATCH: usize = 100;

#[derive(Debug, thiserror::Error)]
pub enum LookupError {
    #[error("the site has no pieces-hash endpoint (HTTP 404); it needs NexusPHP from 2023-07 or later")]
    NoEndpoint,
    #[error("the site rejected the passkey: {0}")]
    Rejected(String),
    #[error("the site is rate limiting requests (HTTP 429)")]
    RateLimited,
    #[error("unexpected answer from the site: {0}")]
    Unexpected(String),
    #[error("could not reach the site: {0}")]
    Network(String),
}

#[derive(Deserialize)]
struct Envelope {
    ret: Option<i64>,
    msg: Option<String>,
    message: Option<String>,
    data: Option<serde_json::Value>,
}

/// Ask `site` which of `pieces_hashes` it has, as pieces hash → torrent id.
///
/// `http` must not follow redirects: an unauthenticated request is answered with a
/// redirect to the login page, which must surface as `Rejected`, not as an empty result.
///
/// # Errors
/// Any answer other than a success envelope is an error; ids that are not plain
/// digits are refused, because they end up in a download URL.
///
/// # Panics
/// If more than [`MAX_BATCH`] hashes are passed; the site would reject the whole call.
pub async fn query(
    http: &reqwest::Client,
    site: &Site,
    pieces_hashes: &[String],
) -> Result<HashMap<String, String>, LookupError> {
    assert!(pieces_hashes.len() <= MAX_BATCH, "pieces-hash batch over {MAX_BATCH}");
    let passkey = site.passkey.as_deref().unwrap_or_default();
    let mut form = vec![("passkey", passkey)];
    form.extend(pieces_hashes.iter().map(|h| ("pieces_hash[]", h.as_str())));

    let response = http
        .post(format!("{}/api/pieces-hash", site.base_url))
        .header("Accept", "application/json")
        .header("User-Agent", concat!("Graft/", env!("CARGO_PKG_VERSION")))
        .form(&form)
        .send()
        .await
        .map_err(|e| LookupError::Network(crate::error_chain(&e.without_url())))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| LookupError::Network(crate::error_chain(&e.without_url())))?;
    let scrub = |text: &str| -> String {
        let text: String = text.chars().take(200).collect();
        if passkey.is_empty() { text } else { text.replace(passkey, "<passkey>") }
    };
    let envelope: Option<Envelope> = serde_json::from_str(&body).ok();

    match (status, envelope) {
        (StatusCode::TOO_MANY_REQUESTS, _) => Err(LookupError::RateLimited),
        (s, _) if s.is_redirection() => Err(LookupError::Rejected(format!("HTTP {s}, redirected to login"))),
        (StatusCode::NOT_FOUND, _) => Err(LookupError::NoEndpoint),
        (StatusCode::OK, Some(Envelope { ret: Some(0), data, .. })) => parse_hits(data),
        (s, Some(e)) if s == StatusCode::UNAUTHORIZED || s == StatusCode::FORBIDDEN => Err(
            LookupError::Rejected(scrub(&e.msg.or(e.message).unwrap_or_else(|| format!("HTTP {s}")))),
        ),
        (s, Some(e)) => Err(LookupError::Unexpected(format!(
            "HTTP {s}, ret {:?}: {}",
            e.ret,
            scrub(&e.msg.or(e.message).unwrap_or_default())
        ))),
        (s, None) => Err(LookupError::Unexpected(format!("HTTP {s}, not JSON: {}", scrub(&body)))),
    }
}

fn parse_hits(data: Option<serde_json::Value>) -> Result<HashMap<String, String>, LookupError> {
    let map = match data {
        Some(serde_json::Value::Object(map)) => map,
        // PHP encodes an empty result as [] unless it is cast to an object.
        Some(serde_json::Value::Array(a)) if a.is_empty() => return Ok(HashMap::new()),
        None | Some(serde_json::Value::Null) => return Ok(HashMap::new()),
        Some(other) => return Err(LookupError::Unexpected(format!("data is {other}"))),
    };
    map.into_iter()
        .map(|(hash, id)| {
            let id = match id {
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => s,
                other => return Err(LookupError::Unexpected(format!("torrent id {other}"))),
            };
            if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
                return Err(LookupError::Unexpected(format!("torrent id {id:?} is not a number")));
            }
            Ok((hash.to_ascii_lowercase(), id))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn hits_accept_numeric_ids_and_empty_results() {
        let hits = parse_hits(Some(json!({"AB": 12, "cd": "34"}))).unwrap();
        assert_eq!(hits["ab"], "12");
        assert_eq!(hits["cd"], "34");
        assert!(parse_hits(Some(json!([]))).unwrap().is_empty());
        assert!(parse_hits(Some(json!({}))).unwrap().is_empty());
    }

    #[test]
    fn ids_that_could_reshape_a_download_url_are_refused() {
        assert!(parse_hits(Some(json!({"ab": "1&passkey=x"}))).is_err());
        assert!(parse_hits(Some(json!({"ab": "../1"}))).is_err());
        assert!(parse_hits(Some(json!({"ab": null}))).is_err());
    }
}
