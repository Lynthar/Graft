-- Graft Database Schema v1
-- Initial migration

-- Clients (download clients like qBittorrent, Transmission)
CREATE TABLE IF NOT EXISTS clients (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    client_type TEXT NOT NULL CHECK (client_type IN ('qbittorrent', 'transmission')),
    host TEXT NOT NULL,
    port INTEGER NOT NULL,
    username TEXT,
    password_encrypted TEXT,
    use_https INTEGER NOT NULL DEFAULT 0,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Sites (PT sites configuration)
CREATE TABLE IF NOT EXISTS sites (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    base_url TEXT NOT NULL,
    template_type TEXT NOT NULL DEFAULT 'nexusphp',
    passkey TEXT,
    cookie_encrypted TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    rate_limit_rpm INTEGER DEFAULT 10,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Content fingerprints (for matching identical content across sites)
CREATE TABLE IF NOT EXISTS content_fingerprints (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    total_size INTEGER NOT NULL,
    file_count INTEGER NOT NULL,
    largest_file_size INTEGER NOT NULL,
    files_hash TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_fingerprint_size ON content_fingerprints(total_size);
CREATE INDEX IF NOT EXISTS idx_fingerprint_composite ON content_fingerprints(total_size, file_count, largest_file_size);

-- Torrent index (maps torrents to sites and content fingerprints)
CREATE TABLE IF NOT EXISTS torrent_index (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    info_hash TEXT NOT NULL,
    site_id TEXT NOT NULL,
    torrent_id TEXT,
    fingerprint_id INTEGER,
    name TEXT,
    size INTEGER,
    save_path TEXT,
    source_client TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(info_hash, site_id),
    FOREIGN KEY (site_id) REFERENCES sites(id) ON DELETE CASCADE,
    FOREIGN KEY (fingerprint_id) REFERENCES content_fingerprints(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_torrent_hash ON torrent_index(info_hash);
CREATE INDEX IF NOT EXISTS idx_torrent_site ON torrent_index(site_id);
CREATE INDEX IF NOT EXISTS idx_torrent_fingerprint ON torrent_index(fingerprint_id);

-- Reseed tasks (scheduled reseed jobs)
CREATE TABLE IF NOT EXISTS reseed_tasks (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    source_client TEXT NOT NULL,
    target_client TEXT NOT NULL,
    target_sites TEXT NOT NULL,  -- JSON array of site IDs
    cron_expression TEXT,
    add_paused INTEGER NOT NULL DEFAULT 0,
    enabled INTEGER NOT NULL DEFAULT 1,
    last_run_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (source_client) REFERENCES clients(id) ON DELETE CASCADE,
    FOREIGN KEY (target_client) REFERENCES clients(id) ON DELETE CASCADE
);

-- Reseed history (log of reseed operations)
CREATE TABLE IF NOT EXISTS reseed_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id TEXT,
    info_hash TEXT NOT NULL,
    source_site TEXT,
    target_site TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('success', 'failed', 'skipped')),
    message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (task_id) REFERENCES reseed_tasks(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_history_hash ON reseed_history(info_hash);
CREATE INDEX IF NOT EXISTS idx_history_date ON reseed_history(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_history_status ON reseed_history(status);

-- System settings (key-value store)
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Tracker domain mappings (for site identification)
CREATE TABLE IF NOT EXISTS tracker_domains (
    domain TEXT PRIMARY KEY,
    site_id TEXT NOT NULL,
    FOREIGN KEY (site_id) REFERENCES sites(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_tracker_site ON tracker_domains(site_id);
