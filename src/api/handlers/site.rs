//! Site handlers. Built-in and custom sites are rows of the same table and behave
//! the same, except that built-in rows can only be disabled, not deleted.

use axum::{
    extract::{Path, State},
    Json,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;

use super::secret;
use crate::api::{AppError, AppState};
use crate::site::{self, SiteView, TemplateType};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateSiteRequest {
    pub id: String,
    pub name: String,
    pub base_url: String,
    #[serde(default = "default_template")]
    pub template_type: TemplateType,
    pub download_pattern: Option<String>,
    pub passkey: Option<String>,
    pub cookie: Option<String>,
    pub authkey: Option<String>,
    /// Tracker domains; defaults to the host of `base_url`.
    pub domains: Option<Vec<String>>,
    pub rate_limit_rpm: Option<u32>,
    pub daily_limit: Option<u32>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Absent fields stay as they are. For the credentials, an empty string clears the value.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateSiteRequest {
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub download_pattern: Option<String>,
    pub passkey: Option<String>,
    pub cookie: Option<String>,
    pub authkey: Option<String>,
    pub domains: Option<Vec<String>>,
    pub rate_limit_rpm: Option<u32>,
    pub daily_limit: Option<u32>,
    pub enabled: Option<bool>,
}

fn default_template() -> TemplateType {
    TemplateType::NexusPHP
}

fn default_true() -> bool {
    true
}

fn default_pattern(template: TemplateType) -> &'static str {
    match template {
        TemplateType::NexusPHP => "/download.php?id={id}&passkey={passkey}",
        TemplateType::Unit3D => "/torrent/download/{id}.{passkey}",
        TemplateType::Gazelle => "/torrents.php?action=download&id={id}&authkey={authkey}&torrent_pass={passkey}",
    }
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<SiteView>>, AppError> {
    let sites = site::load_all(&state.db.conn())?;
    Ok(Json(sites.iter().map(SiteView::from).collect()))
}

pub async fn get_one(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<SiteView>, AppError> {
    let site = site::load(&state.db.conn(), &id)?.ok_or_else(|| not_found(&id))?;
    Ok(Json(SiteView::from(&site)))
}

pub async fn create(
    State(state): State<AppState>,
    Json(req): Json<CreateSiteRequest>,
) -> Result<Json<SiteView>, AppError> {
    validate_id(&req.id)?;
    let name = non_empty("名称", &req.name)?;
    let base_url = validate_base_url(&req.base_url)?;
    let pattern = match &req.download_pattern {
        Some(p) => validate_pattern(p)?,
        None => default_pattern(req.template_type).to_string(),
    };
    let domains = match &req.domains {
        Some(d) => validate_domains(d)?,
        None => vec![default_domain(&base_url)],
    };
    let rpm = validate_rpm(req.rate_limit_rpm.unwrap_or(10))?;
    let daily = validate_daily(req.daily_limit.unwrap_or(20))?;

    let mut conn = state.db.conn();
    let tx = conn.transaction()?;
    let inserted = tx.execute(
        "INSERT INTO sites (id, name, base_url, template_type, download_pattern, passkey, cookie,
             authkey, enabled, rate_limit_rpm, daily_limit, builtin)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0)
         ON CONFLICT(id) DO NOTHING",
        params![
            req.id,
            name,
            base_url,
            req.template_type.to_string(),
            pattern,
            secret(req.passkey),
            secret(req.cookie),
            secret(req.authkey),
            req.enabled,
            rpm,
            daily
        ],
    )?;
    if inserted == 0 {
        return Err(AppError::bad_request(format!("id 为 {} 的站点已存在", req.id)));
    }
    replace_domains(&tx, &req.id, &domains)?;
    tx.commit()?;

    let site = site::load(&conn, &req.id)?.ok_or_else(|| not_found(&req.id))?;
    Ok(Json(SiteView::from(&site)))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSiteRequest>,
) -> Result<Json<SiteView>, AppError> {
    let mut conn = state.db.conn();
    let current = site::load(&conn, &id)?.ok_or_else(|| not_found(&id))?;
    let name = match &req.name {
        Some(n) => non_empty("名称", n)?,
        None => current.name.clone(),
    };
    let base_url = match &req.base_url {
        Some(u) => validate_base_url(u)?,
        None => current.base_url.clone(),
    };
    let pattern = match &req.download_pattern {
        Some(p) => validate_pattern(p)?,
        None => current.download_pattern.clone(),
    };
    let rpm = validate_rpm(req.rate_limit_rpm.unwrap_or(current.rate_limit_rpm))?;
    let daily = validate_daily(req.daily_limit.unwrap_or(current.daily_limit))?;
    let keep = |new: Option<String>, old: Option<String>| match new {
        Some(v) => secret(Some(v)),
        None => old,
    };

    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE sites SET name = ?2, base_url = ?3, download_pattern = ?4, passkey = ?5, cookie = ?6,
             authkey = ?7, enabled = ?8, rate_limit_rpm = ?9, daily_limit = ?10, updated_at = datetime('now')
         WHERE id = ?1",
        params![
            id,
            name,
            base_url,
            pattern,
            keep(req.passkey, current.passkey),
            keep(req.cookie, current.cookie),
            keep(req.authkey, current.authkey),
            req.enabled.unwrap_or(current.enabled),
            rpm,
            daily
        ],
    )?;
    if let Some(domains) = &req.domains {
        replace_domains(&tx, &id, &validate_domains(domains)?)?;
    }
    tx.commit()?;

    let site = site::load(&conn, &id)?.ok_or_else(|| not_found(&id))?;
    Ok(Json(SiteView::from(&site)))
}

pub async fn remove(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let conn = state.db.conn();
    let site = site::load(&conn, &id)?.ok_or_else(|| not_found(&id))?;
    if site.builtin {
        return Err(AppError::bad_request(format!(
            "{} 是内置站点，不能删除，可以停用",
            site.name
        )));
    }
    conn.execute("DELETE FROM sites WHERE id = ?1", [&id])?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}

fn not_found(id: &str) -> AppError {
    AppError::not_found(format!("没有 id 为 {id} 的站点"))
}

fn non_empty(field: &str, value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::bad_request(format!("{field}不能为空")));
    }
    Ok(value.to_string())
}

fn validate_id(id: &str) -> Result<(), AppError> {
    let ok = (1..=32).contains(&id.len())
        && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
    if ok {
        Ok(())
    } else {
        Err(AppError::bad_request("站点 id 须为 1–32 个字符，只能用 a–z、0–9、- 和 _"))
    }
}

/// Requests to a site carry the passkey, so they must be encrypted unless they never
/// leave the machine.
fn validate_base_url(raw: &str) -> Result<String, AppError> {
    let url = url::Url::parse(raw.trim())
        .map_err(|_| AppError::bad_request(format!("{raw:?} 不是有效的网址")))?;
    let loopback = match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        Some(url::Host::Domain(d)) => d.eq_ignore_ascii_case("localhost"),
        None => false,
    };
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return Err(AppError::bad_request("站点地址必须以 https:// 开头"));
    }
    if url.query().is_some() || url.fragment().is_some() || url.path() != "/" {
        return Err(AppError::bad_request("站点地址只写协议和主机名，例如 https://example.org"));
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}

fn validate_pattern(pattern: &str) -> Result<String, AppError> {
    let pattern = pattern.trim();
    if !pattern.starts_with('/') || !pattern.contains("{id}") {
        return Err(AppError::bad_request("下载路径必须以 / 开头并包含 {id}"));
    }
    Ok(pattern.to_string())
}

fn validate_domains(domains: &[String]) -> Result<Vec<String>, AppError> {
    let mut out: Vec<String> = Vec::new();
    for d in domains {
        let d = d.trim().trim_end_matches('.').to_ascii_lowercase();
        let ok = !d.is_empty()
            && d.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
            && !d.starts_with('.');
        if !ok {
            return Err(AppError::bad_request(format!("{d:?} 不是有效的域名")));
        }
        if !out.contains(&d) {
            out.push(d);
        }
    }
    Ok(out)
}

fn default_domain(base_url: &str) -> String {
    let host = site::host_of(base_url).unwrap_or_default();
    host.strip_prefix("www.").map(str::to_string).unwrap_or(host)
}

fn validate_rpm(rpm: u32) -> Result<u32, AppError> {
    if (1..=60).contains(&rpm) {
        Ok(rpm)
    } else {
        Err(AppError::bad_request("每分钟请求数须在 1 到 60 之间"))
    }
}

fn validate_daily(limit: u32) -> Result<u32, AppError> {
    if limit <= 1000 {
        Ok(limit)
    } else {
        Err(AppError::bad_request("每日下载上限最多 1000"))
    }
}

fn replace_domains(conn: &Connection, site_id: &str, domains: &[String]) -> Result<(), AppError> {
    conn.execute("DELETE FROM site_domains WHERE site_id = ?1", [site_id])?;
    for domain in domains {
        let owner: Option<String> = conn
            .query_row("SELECT site_id FROM site_domains WHERE domain = ?1", [domain], |r| r.get(0))
            .optional()?;
        if let Some(owner) = owner {
            return Err(AppError::bad_request(format!("域名 {domain} 已属于站点 {owner}")));
        }
        conn.execute("INSERT INTO site_domains (domain, site_id) VALUES (?1, ?2)", [domain, site_id])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_address_must_be_https_unless_loopback() {
        assert_eq!(validate_base_url("https://pt.example.org/").unwrap(), "https://pt.example.org");
        assert!(validate_base_url("http://pt.example.org").is_err());
        assert!(validate_base_url("http://127.0.0.1:8080").is_ok());
        assert!(validate_base_url("https://pt.example.org/torrents.php").is_err());
    }

    #[test]
    fn domains_are_normalised_and_checked() {
        assert_eq!(validate_domains(&["Tracker.Example.org.".into()]).unwrap(), vec!["tracker.example.org"]);
        assert!(validate_domains(&["exa mple.org".into()]).is_err());
        assert_eq!(default_domain("https://www.example.org"), "example.org");
    }
}
