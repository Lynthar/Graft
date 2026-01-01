//! Tracker URL identification
//!
//! Identifies PT sites from tracker URLs and extracts torrent IDs when possible.

use std::collections::HashMap;
use url::Url;

/// Result of site identification from a tracker URL
#[derive(Debug, Clone)]
pub struct SiteIdentification {
    pub site_id: String,
    pub torrent_id: Option<String>,
}

/// Identifies PT sites from tracker URLs
pub struct TrackerIdentifier {
    /// domain -> site_id mapping
    domain_map: HashMap<String, String>,
}

impl TrackerIdentifier {
    pub fn new() -> Self {
        let mut identifier = Self {
            domain_map: HashMap::new(),
        };
        identifier.register_builtin_sites();
        identifier
    }

    fn register_builtin_sites(&mut self) {
        let mappings = [
            // M-Team
            ("m-team.cc", "mteam"),
            ("kp.m-team.cc", "mteam"),
            ("pt.m-team.cc", "mteam"),
            // HDSky
            ("hdsky.me", "hdsky"),
            // OurBits
            ("ourbits.club", "ourbits"),
            // PTer
            ("pterclub.com", "pterclub"),
            // HDHome
            ("hdhome.org", "hdhome"),
            // Audiences
            ("audiences.me", "audiences"),
            // CHDBits
            ("chdbits.co", "chdbits"),
            // TTG
            ("totheglory.im", "ttg"),
            ("t.totheglory.im", "ttg"),
            // SSD
            ("springsunday.net", "ssd"),
            // HDArea
            ("hdarea.club", "hdarea"),
            // HDAtmos
            ("hdatmos.club", "hdatmos"),
            // HDFans
            ("hdfans.org", "hdfans"),
            // HDTime
            ("hdtime.org", "hdtime"),
            // 1PTBA
            ("1ptba.com", "1ptba"),
            // HDZone
            ("hdzone.me", "hdzone"),
            // HDUPT
            ("pt.hdupt.com", "hdupt"),
            // BTSchool
            ("pt.btschool.club", "btschool"),
            // Unit3D sites
            ("blutopia.cc", "blutopia"),
            ("aither.cc", "aither"),
            ("reelflix.xyz", "reelflix"),
            // Gazelle sites
            ("redacted.ch", "redacted"),
            ("flacsfor.me", "redacted"),
            ("orpheus.network", "orpheus"),
            // More sites can be added here
        ];

        for (domain, site_id) in mappings {
            self.domain_map.insert(domain.to_string(), site_id.to_string());
        }
    }

    /// Identify site from a tracker URL
    ///
    /// Returns the site ID and optionally the torrent ID if it can be extracted.
    pub fn identify(&self, tracker_url: &str) -> Option<SiteIdentification> {
        let url = Url::parse(tracker_url).ok()?;
        let host = url.host_str()?;

        let site_id = self.find_site_by_host(host)?;
        let torrent_id = self.extract_torrent_id(&url);

        Some(SiteIdentification {
            site_id,
            torrent_id,
        })
    }

    /// Identify site from multiple tracker URLs
    ///
    /// Returns the first successful identification.
    pub fn identify_from_trackers(&self, trackers: &[String]) -> Option<SiteIdentification> {
        for tracker in trackers {
            if let Some(result) = self.identify(tracker) {
                return Some(result);
            }
        }
        None
    }

    fn find_site_by_host(&self, host: &str) -> Option<String> {
        // Direct match
        if let Some(site_id) = self.domain_map.get(host) {
            return Some(site_id.clone());
        }

        // Try matching without subdomain
        let parts: Vec<&str> = host.split('.').collect();
        if parts.len() >= 2 {
            let base_domain = parts[parts.len() - 2..].join(".");
            if let Some(site_id) = self.domain_map.get(&base_domain) {
                return Some(site_id.clone());
            }
        }

        // Try matching with one subdomain level
        if parts.len() >= 3 {
            let with_subdomain = parts[parts.len() - 3..].join(".");
            if let Some(site_id) = self.domain_map.get(&with_subdomain) {
                return Some(site_id.clone());
            }
        }

        None
    }

    fn extract_torrent_id(&self, url: &Url) -> Option<String> {
        // Common patterns for torrent ID in tracker URLs:
        // - ?torrent_id=xxx
        // - ?id=xxx
        // - /announce/xxx (path-based)

        for (key, value) in url.query_pairs() {
            match key.as_ref() {
                "torrent_id" | "id" | "tid" => {
                    return Some(value.to_string());
                }
                _ => {}
            }
        }

        // Try path-based extraction (e.g., /announce/12345)
        let path = url.path();
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        for segment in segments.iter().rev() {
            if segment.chars().all(|c| c.is_ascii_digit()) && segment.len() > 0 {
                return Some(segment.to_string());
            }
        }

        None
    }

    /// Register a custom site domain mapping
    pub fn register_site(&mut self, domain: &str, site_id: &str) {
        self.domain_map.insert(domain.to_string(), site_id.to_string());
    }

    /// Get all registered domains
    pub fn get_domains(&self) -> Vec<(&String, &String)> {
        self.domain_map.iter().collect()
    }
}

impl Default for TrackerIdentifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identify_mteam() {
        let identifier = TrackerIdentifier::new();

        let result = identifier.identify("https://kp.m-team.cc/announce.php?passkey=abc123");
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.site_id, "mteam");
    }

    #[test]
    fn test_identify_with_torrent_id() {
        let identifier = TrackerIdentifier::new();

        let result = identifier.identify("https://hdsky.me/announce.php?passkey=abc&torrent_id=12345");
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.site_id, "hdsky");
        assert_eq!(result.torrent_id, Some("12345".to_string()));
    }

    #[test]
    fn test_unknown_site() {
        let identifier = TrackerIdentifier::new();

        let result = identifier.identify("https://unknown-site.com/announce");
        assert!(result.is_none());
    }
}
