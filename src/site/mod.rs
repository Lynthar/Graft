//! PT sites: the stored site table, tracker-host recognition, the pieces-hash
//! lookup, rate limiting and `.torrent` downloads.

pub mod gate;
pub mod lookup;
pub mod templates;

pub use templates::{SiteTemplate, TemplateType};

use rusqlite::{Connection, OptionalExtension, Row};
use serde::Serialize;

/// A row of the `sites` table with its tracker domains.
#[derive(Debug, Clone)]
pub struct Site {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub template_type: TemplateType,
    pub download_pattern: String,
    pub passkey: Option<String>,
    pub cookie: Option<String>,
    pub authkey: Option<String>,
    pub enabled: bool,
    pub rate_limit_rpm: u32,
    pub daily_limit: u32,
    pub builtin: bool,
    pub domains: Vec<String>,
}

/// What the API shows of a site: never the credentials themselves.
#[derive(Debug, Serialize)]
pub struct SiteView {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub template_type: TemplateType,
    pub download_pattern: String,
    pub has_passkey: bool,
    pub has_cookie: bool,
    pub has_authkey: bool,
    pub enabled: bool,
    pub rate_limit_rpm: u32,
    pub daily_limit: u32,
    pub builtin: bool,
    pub domains: Vec<String>,
}

impl From<&Site> for SiteView {
    fn from(s: &Site) -> Self {
        SiteView {
            id: s.id.clone(),
            name: s.name.clone(),
            base_url: s.base_url.clone(),
            template_type: s.template_type,
            download_pattern: s.download_pattern.clone(),
            has_passkey: s.passkey.is_some(),
            has_cookie: s.cookie.is_some(),
            has_authkey: s.authkey.is_some(),
            enabled: s.enabled,
            rate_limit_rpm: s.rate_limit_rpm,
            daily_limit: s.daily_limit,
            builtin: s.builtin,
            domains: s.domains.clone(),
        }
    }
}

impl Site {
    pub fn create_template(&self) -> Box<dyn SiteTemplate> {
        match self.template_type {
            TemplateType::NexusPHP => Box::new(templates::NexusPHPTemplate::new(self.clone())),
            TemplateType::Unit3D => Box::new(templates::Unit3DTemplate::new(self.clone())),
            TemplateType::Gazelle => Box::new(templates::GazelleTemplate::new(self.clone())),
        }
    }

    /// Whether `host` is one of this site's tracker domains or a subdomain of one.
    pub fn owns_host(&self, host: &str) -> bool {
        let host = host.to_ascii_lowercase();
        self.domains
            .iter()
            .any(|d| host == *d || host.strip_suffix(d.as_str()).is_some_and(|p| p.ends_with('.')))
    }
}

const COLUMNS: &str = "id, name, base_url, template_type, download_pattern, passkey, cookie, \
                       authkey, enabled, rate_limit_rpm, daily_limit, builtin";

fn from_row(row: &Row) -> rusqlite::Result<Site> {
    let template: String = row.get(3)?;
    Ok(Site {
        id: row.get(0)?,
        name: row.get(1)?,
        base_url: row.get(2)?,
        // The CHECK constraint on template_type keeps unknown values out of the table.
        template_type: template.parse().map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
        })?,
        download_pattern: row.get(4)?,
        passkey: row.get(5)?,
        cookie: row.get(6)?,
        authkey: row.get(7)?,
        enabled: row.get(8)?,
        rate_limit_rpm: row.get(9)?,
        daily_limit: row.get(10)?,
        builtin: row.get(11)?,
        domains: Vec::new(),
    })
}

fn with_domains(conn: &Connection, mut site: Site) -> rusqlite::Result<Site> {
    let mut stmt = conn.prepare("SELECT domain FROM site_domains WHERE site_id = ?1 ORDER BY domain")?;
    site.domains = stmt
        .query_map([&site.id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(site)
}

pub fn load_all(conn: &Connection) -> rusqlite::Result<Vec<Site>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLUMNS} FROM sites ORDER BY name"))?;
    let sites = stmt.query_map([], from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    sites.into_iter().map(|s| with_domains(conn, s)).collect()
}

pub fn load(conn: &Connection, id: &str) -> rusqlite::Result<Option<Site>> {
    let site = conn
        .query_row(&format!("SELECT {COLUMNS} FROM sites WHERE id = ?1"), [id], from_row)
        .optional()?;
    site.map(|s| with_domains(conn, s)).transpose()
}

/// The site whose tracker domain matches the host of `tracker_url`.
pub fn recognize<'a>(sites: &'a [Site], tracker_url: &str) -> Option<&'a Site> {
    recognize_host(sites, url::Url::parse(tracker_url).ok()?.host_str()?)
}

/// The site whose domains cover `host`.
pub fn recognize_host<'a>(sites: &'a [Site], host: &str) -> Option<&'a Site> {
    sites.iter().find(|s| s.owns_host(host))
}

/// Host part of a URL, for reports; never the full URL, which may carry a passkey.
pub fn host_of(url: &str) -> Option<String> {
    url::Url::parse(url).ok()?.host_str().map(str::to_ascii_lowercase)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subdomains_match_but_lookalikes_do_not() {
        let site = Site {
            id: "s".into(),
            name: "S".into(),
            base_url: "https://hdsky.me".into(),
            template_type: TemplateType::NexusPHP,
            download_pattern: String::new(),
            passkey: None,
            cookie: None,
            authkey: None,
            enabled: true,
            rate_limit_rpm: 10,
            daily_limit: 20,
            builtin: false,
            domains: vec!["hdsky.me".into()],
        };
        let sites = [site];
        assert!(recognize(&sites, "https://tracker.hdsky.me/announce.php?passkey=x").is_some());
        assert!(recognize(&sites, "https://HDSKY.me/announce.php").is_some());
        assert!(recognize(&sites, "https://nothdsky.me/announce.php").is_none());
        assert!(recognize(&sites, "not a url").is_none());
    }
}
