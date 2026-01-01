//! Content fingerprinting for cross-site matching
//!
//! This module implements content-based matching to identify identical resources
//! across different PT sites. Since info_hash differs between sites (due to different
//! tracker URLs), we use file structure fingerprinting instead.

use serde::{Deserialize, Serialize};
use sha1_smol::Sha1;
use std::collections::HashMap;

use crate::client::TorrentFile;

/// Content fingerprint for a torrent
///
/// Used to identify identical content across different sites.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ContentFingerprint {
    /// Total size of all files in bytes (primary matching key)
    pub total_size: u64,

    /// Number of files in the torrent
    pub file_count: usize,

    /// Size of the largest file in bytes
    pub largest_file_size: u64,

    /// Hash of the file list (paths + sizes) for strict matching
    pub files_hash: Option<String>,
}

impl ContentFingerprint {
    /// Create a fingerprint from a list of torrent files
    pub fn from_files(files: &[TorrentFile]) -> Self {
        let total_size: u64 = files.iter().map(|f| f.size).sum();
        let file_count = files.len();
        let largest_file_size = files.iter().map(|f| f.size).max().unwrap_or(0);

        // Calculate files hash for strict matching
        let files_hash = if !files.is_empty() {
            let mut hasher = Sha1::new();

            // Sort files by name for consistent hashing
            let mut sorted_files: Vec<_> = files.iter().collect();
            sorted_files.sort_by(|a, b| a.name.cmp(&b.name));

            for file in sorted_files {
                hasher.update(file.name.as_bytes());
                hasher.update(&file.size.to_le_bytes());
            }

            Some(hasher.digest().to_string())
        } else {
            None
        };

        Self {
            total_size,
            file_count,
            largest_file_size,
            files_hash,
        }
    }

    /// Create a fingerprint with just size information (quick mode)
    pub fn from_size(total_size: u64, file_count: usize, largest_file_size: u64) -> Self {
        Self {
            total_size,
            file_count,
            largest_file_size,
            files_hash: None,
        }
    }

    /// Check if two fingerprints match
    ///
    /// Uses a layered matching strategy:
    /// 1. Total size must match exactly
    /// 2. File count should be close (allowing for small metadata files)
    /// 3. Largest file size should match (high confidence)
    /// 4. If files_hash is available, use for verification
    pub fn matches(&self, other: &ContentFingerprint) -> MatchResult {
        // Primary key: total size must match exactly
        if self.total_size != other.total_size {
            return MatchResult::NoMatch;
        }

        // If files_hash is available on both, use it for definitive matching
        if let (Some(ref hash1), Some(ref hash2)) = (&self.files_hash, &other.files_hash) {
            if hash1 == hash2 {
                return MatchResult::ExactMatch;
            } else {
                // Same size but different file structure - could be different content
                return MatchResult::NoMatch;
            }
        }

        // Check largest file size
        if self.largest_file_size != other.largest_file_size {
            // Could be different content or just different small files
            return MatchResult::LowConfidence;
        }

        // Check file count (allow ±2 for metadata files like .nfo, .txt)
        let count_diff = (self.file_count as i64 - other.file_count as i64).abs();
        if count_diff > 2 {
            return MatchResult::LowConfidence;
        }

        // All checks passed
        if count_diff == 0 {
            MatchResult::HighConfidence
        } else {
            MatchResult::MediumConfidence
        }
    }
}

/// Result of fingerprint matching
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchResult {
    /// No match - fingerprints are definitely different
    NoMatch,
    /// Low confidence match - only total size matches
    LowConfidence,
    /// Medium confidence match - size and structure similar but not identical
    MediumConfidence,
    /// High confidence match - all primary fields match
    HighConfidence,
    /// Exact match - files_hash matches
    ExactMatch,
}

impl MatchResult {
    /// Check if this is a usable match (medium confidence or higher)
    pub fn is_match(&self) -> bool {
        matches!(
            self,
            MatchResult::MediumConfidence | MatchResult::HighConfidence | MatchResult::ExactMatch
        )
    }

    /// Get confidence score (0.0 - 1.0)
    pub fn confidence(&self) -> f64 {
        match self {
            MatchResult::NoMatch => 0.0,
            MatchResult::LowConfidence => 0.3,
            MatchResult::MediumConfidence => 0.7,
            MatchResult::HighConfidence => 0.9,
            MatchResult::ExactMatch => 1.0,
        }
    }
}

/// Fingerprint matcher for finding matching content across sites
pub struct FingerprintMatcher {
    /// Fingerprints indexed by total_size for fast lookup
    size_index: HashMap<u64, Vec<FingerprintEntry>>,
}

#[derive(Debug, Clone)]
pub struct FingerprintEntry {
    pub fingerprint: ContentFingerprint,
    pub info_hash: String,
    pub site_id: String,
    pub torrent_id: Option<String>,
    pub name: Option<String>,
    pub save_path: Option<String>,
}

impl FingerprintMatcher {
    pub fn new() -> Self {
        Self {
            size_index: HashMap::new(),
        }
    }

    /// Add a fingerprint entry to the matcher
    pub fn add(&mut self, entry: FingerprintEntry) {
        let size = entry.fingerprint.total_size;
        self.size_index.entry(size).or_default().push(entry);
    }

    /// Find matching entries for a given fingerprint
    ///
    /// Returns entries that match with medium confidence or higher.
    pub fn find_matches(&self, fingerprint: &ContentFingerprint) -> Vec<MatchedEntry> {
        let mut matches = Vec::new();

        // Fast lookup by size
        if let Some(candidates) = self.size_index.get(&fingerprint.total_size) {
            for candidate in candidates {
                let result = fingerprint.matches(&candidate.fingerprint);
                if result.is_match() {
                    matches.push(MatchedEntry {
                        entry: candidate.clone(),
                        match_result: result,
                    });
                }
            }
        }

        // Sort by confidence (highest first)
        matches.sort_by(|a, b| {
            b.match_result
                .confidence()
                .partial_cmp(&a.match_result.confidence())
                .unwrap()
        });

        matches
    }

    /// Find matches for a torrent, excluding entries from the same site
    pub fn find_cross_site_matches(
        &self,
        fingerprint: &ContentFingerprint,
        exclude_site: &str,
    ) -> Vec<MatchedEntry> {
        self.find_matches(fingerprint)
            .into_iter()
            .filter(|m| m.entry.site_id != exclude_site)
            .collect()
    }

    /// Get total number of entries
    pub fn len(&self) -> usize {
        self.size_index.values().map(|v| v.len()).sum()
    }

    /// Check if the matcher is empty
    pub fn is_empty(&self) -> bool {
        self.size_index.is_empty()
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.size_index.clear();
    }
}

impl Default for FingerprintMatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// A matched entry with its match result
#[derive(Debug, Clone)]
pub struct MatchedEntry {
    pub entry: FingerprintEntry,
    pub match_result: MatchResult,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fingerprint_exact_match() {
        let files = vec![
            TorrentFile {
                name: "movie.mkv".to_string(),
                size: 10_000_000_000,
                progress: 1.0,
            },
            TorrentFile {
                name: "movie.nfo".to_string(),
                size: 1000,
                progress: 1.0,
            },
        ];

        let fp1 = ContentFingerprint::from_files(&files);
        let fp2 = ContentFingerprint::from_files(&files);

        assert_eq!(fp1.matches(&fp2), MatchResult::ExactMatch);
    }

    #[test]
    fn test_fingerprint_high_confidence() {
        let fp1 = ContentFingerprint::from_size(10_000_001_000, 2, 10_000_000_000);
        let fp2 = ContentFingerprint::from_size(10_000_001_000, 2, 10_000_000_000);

        assert_eq!(fp1.matches(&fp2), MatchResult::HighConfidence);
    }

    #[test]
    fn test_fingerprint_no_match_different_size() {
        let fp1 = ContentFingerprint::from_size(10_000_000_000, 2, 9_999_999_000);
        let fp2 = ContentFingerprint::from_size(10_000_001_000, 2, 9_999_999_000);

        assert_eq!(fp1.matches(&fp2), MatchResult::NoMatch);
    }
}
