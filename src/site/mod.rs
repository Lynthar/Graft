//! Site management module
//!
//! This module handles PT site identification, configuration, and template-based
//! torrent downloading.

mod tracker;
pub mod templates;

pub use tracker::{TrackerIdentifier, SiteIdentification};
pub use templates::{SiteTemplate, NexusPHPTemplate, TemplateType};

use serde::{Deserialize, Serialize};

/// Site configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteConfig {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub template_type: TemplateType,
    pub tracker_domains: Vec<String>,
    pub download_pattern: String,
    pub passkey: Option<String>,
    pub cookie: Option<String>,
    pub enabled: bool,
    pub rate_limit_rpm: Option<u32>,
}

impl SiteConfig {
    /// Create a site template instance
    pub fn create_template(&self) -> Box<dyn SiteTemplate> {
        match self.template_type {
            TemplateType::NexusPHP => Box::new(NexusPHPTemplate::new(self.clone())),
            TemplateType::Unit3D => Box::new(templates::Unit3DTemplate::new(self.clone())),
            TemplateType::Gazelle => Box::new(templates::GazelleTemplate::new(self.clone())),
        }
    }
}

/// Built-in site configurations
pub fn builtin_sites() -> Vec<SiteConfig> {
    vec![
        // NexusPHP sites
        SiteConfig {
            id: "mteam".to_string(),
            name: "M-Team".to_string(),
            base_url: "https://kp.m-team.cc".to_string(),
            template_type: TemplateType::NexusPHP,
            tracker_domains: vec![
                "m-team.cc".to_string(),
                "kp.m-team.cc".to_string(),
                "pt.m-team.cc".to_string(),
            ],
            download_pattern: "/download.php?id={id}&passkey={passkey}".to_string(),
            passkey: None,
            cookie: None,
            enabled: false,
            rate_limit_rpm: Some(10),
        },
        SiteConfig {
            id: "hdsky".to_string(),
            name: "HDSky".to_string(),
            base_url: "https://hdsky.me".to_string(),
            template_type: TemplateType::NexusPHP,
            tracker_domains: vec!["hdsky.me".to_string()],
            download_pattern: "/download.php?id={id}&passkey={passkey}".to_string(),
            passkey: None,
            cookie: None,
            enabled: false,
            rate_limit_rpm: Some(10),
        },
        SiteConfig {
            id: "ourbits".to_string(),
            name: "OurBits".to_string(),
            base_url: "https://ourbits.club".to_string(),
            template_type: TemplateType::NexusPHP,
            tracker_domains: vec!["ourbits.club".to_string()],
            download_pattern: "/download.php?id={id}&passkey={passkey}".to_string(),
            passkey: None,
            cookie: None,
            enabled: false,
            rate_limit_rpm: Some(10),
        },
        SiteConfig {
            id: "pterclub".to_string(),
            name: "PTer".to_string(),
            base_url: "https://pterclub.com".to_string(),
            template_type: TemplateType::NexusPHP,
            tracker_domains: vec!["pterclub.com".to_string()],
            download_pattern: "/download.php?id={id}&passkey={passkey}".to_string(),
            passkey: None,
            cookie: None,
            enabled: false,
            rate_limit_rpm: Some(10),
        },
        SiteConfig {
            id: "hdhome".to_string(),
            name: "HDHome".to_string(),
            base_url: "https://hdhome.org".to_string(),
            template_type: TemplateType::NexusPHP,
            tracker_domains: vec!["hdhome.org".to_string()],
            download_pattern: "/download.php?id={id}&passkey={passkey}".to_string(),
            passkey: None,
            cookie: None,
            enabled: false,
            rate_limit_rpm: Some(10),
        },
        SiteConfig {
            id: "audiences".to_string(),
            name: "Audiences".to_string(),
            base_url: "https://audiences.me".to_string(),
            template_type: TemplateType::NexusPHP,
            tracker_domains: vec!["audiences.me".to_string()],
            download_pattern: "/download.php?id={id}&passkey={passkey}".to_string(),
            passkey: None,
            cookie: None,
            enabled: false,
            rate_limit_rpm: Some(10),
        },
        SiteConfig {
            id: "chdbits".to_string(),
            name: "CHDBits".to_string(),
            base_url: "https://chdbits.co".to_string(),
            template_type: TemplateType::NexusPHP,
            tracker_domains: vec!["chdbits.co".to_string()],
            download_pattern: "/download.php?id={id}&passkey={passkey}".to_string(),
            passkey: None,
            cookie: None,
            enabled: false,
            rate_limit_rpm: Some(10),
        },
        SiteConfig {
            id: "ttg".to_string(),
            name: "TTG".to_string(),
            base_url: "https://totheglory.im".to_string(),
            template_type: TemplateType::NexusPHP,
            tracker_domains: vec!["totheglory.im".to_string(), "t.totheglory.im".to_string()],
            download_pattern: "/dl/{id}/{passkey}".to_string(),
            passkey: None,
            cookie: None,
            enabled: false,
            rate_limit_rpm: Some(10),
        },
        // Unit3D sites
        SiteConfig {
            id: "blutopia".to_string(),
            name: "Blutopia".to_string(),
            base_url: "https://blutopia.cc".to_string(),
            template_type: TemplateType::Unit3D,
            tracker_domains: vec!["blutopia.cc".to_string()],
            download_pattern: "/torrent/download/{id}.{passkey}".to_string(),
            passkey: None,
            cookie: None,
            enabled: false,
            rate_limit_rpm: Some(10),
        },
        SiteConfig {
            id: "aither".to_string(),
            name: "Aither".to_string(),
            base_url: "https://aither.cc".to_string(),
            template_type: TemplateType::Unit3D,
            tracker_domains: vec!["aither.cc".to_string()],
            download_pattern: "/torrent/download/{id}.{passkey}".to_string(),
            passkey: None,
            cookie: None,
            enabled: false,
            rate_limit_rpm: Some(10),
        },
        // Gazelle sites
        SiteConfig {
            id: "redacted".to_string(),
            name: "Redacted".to_string(),
            base_url: "https://redacted.ch".to_string(),
            template_type: TemplateType::Gazelle,
            tracker_domains: vec!["redacted.ch".to_string(), "flacsfor.me".to_string()],
            download_pattern: "/torrents.php?action=download&id={id}&authkey={authkey}&torrent_pass={passkey}".to_string(),
            passkey: None,
            cookie: None,
            enabled: false,
            rate_limit_rpm: Some(5),
        },
        SiteConfig {
            id: "orpheus".to_string(),
            name: "Orpheus".to_string(),
            base_url: "https://orpheus.network".to_string(),
            template_type: TemplateType::Gazelle,
            tracker_domains: vec!["orpheus.network".to_string()],
            download_pattern: "/torrents.php?action=download&id={id}&authkey={authkey}&torrent_pass={passkey}".to_string(),
            passkey: None,
            cookie: None,
            enabled: false,
            rate_limit_rpm: Some(5),
        },
    ]
}
