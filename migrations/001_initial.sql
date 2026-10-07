-- Graft schema version 1 (PRAGMA user_version = 1, set by the migration runner).
-- Plain CREATE statements: a database that already has these tables must fail here,
-- not be stamped as version 1 with a stale layout.

-- Download clients
CREATE TABLE clients (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    client_type TEXT NOT NULL CHECK (client_type IN ('qbittorrent', 'transmission')),
    host TEXT NOT NULL,
    port INTEGER NOT NULL,
    username TEXT,
    password TEXT,
    use_https INTEGER NOT NULL DEFAULT 0,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Sites: the only source of site identity. Built-in sites are ordinary rows.
CREATE TABLE sites (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    base_url TEXT NOT NULL,
    template_type TEXT NOT NULL
        CHECK (template_type IN ('nexusphp', 'unit3d', 'gazelle')),
    -- Relative to base_url; {id}, {passkey} and {authkey} are substituted.
    download_pattern TEXT NOT NULL,
    passkey TEXT,
    cookie TEXT,
    authkey TEXT,
    enabled INTEGER NOT NULL DEFAULT 0,
    rate_limit_rpm INTEGER NOT NULL DEFAULT 10 CHECK (rate_limit_rpm BETWEEN 1 AND 60),
    daily_limit INTEGER NOT NULL DEFAULT 20 CHECK (daily_limit >= 0),
    builtin INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Tracker hosts that identify a site; a host matches a domain or any subdomain of it.
CREATE TABLE site_domains (
    domain TEXT PRIMARY KEY,
    site_id TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE
);

CREATE INDEX idx_site_domains_site ON site_domains(site_id);

-- sha1(info.pieces) per info hash; both are fixed by the metainfo, so entries never go stale.
CREATE TABLE piece_hashes (
    info_hash TEXT PRIMARY KEY,
    pieces_hash TEXT NOT NULL
);

-- One row per candidate an execution run handled.
CREATE TABLE reseed_results (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL,
    source_hash TEXT NOT NULL,
    source_name TEXT NOT NULL,
    source_site TEXT,
    target_site TEXT NOT NULL,
    target_torrent_id TEXT NOT NULL,
    target_client TEXT NOT NULL,
    target_hash TEXT,
    status TEXT NOT NULL CHECK (status IN ('success', 'skipped', 'failed')),
    step TEXT NOT NULL,
    message TEXT NOT NULL,
    -- Whether a .torrent was requested from the site; counts toward its daily limit.
    downloaded INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_results_created ON reseed_results(created_at DESC);
CREATE INDEX idx_results_site_day ON reseed_results(target_site, created_at);

-- Login sessions, used only when an access password is set. password_check is
-- sha1(token, password), so a changed password ends every session.
CREATE TABLE sessions (
    token TEXT PRIMARY KEY,
    password_check TEXT NOT NULL,
    -- Unix seconds.
    expires_at INTEGER NOT NULL
);

INSERT INTO sites (id, name, base_url, template_type, download_pattern, builtin) VALUES
    ('mteam', 'M-Team', 'https://kp.m-team.cc', 'nexusphp', '/download.php?id={id}&passkey={passkey}', 1),
    ('hdsky', 'HDSky', 'https://hdsky.me', 'nexusphp', '/download.php?id={id}&passkey={passkey}', 1),
    ('ourbits', 'OurBits', 'https://ourbits.club', 'nexusphp', '/download.php?id={id}&passkey={passkey}', 1),
    ('pterclub', 'PTer', 'https://pterclub.com', 'nexusphp', '/download.php?id={id}&passkey={passkey}', 1),
    ('hdhome', 'HDHome', 'https://hdhome.org', 'nexusphp', '/download.php?id={id}&passkey={passkey}', 1),
    ('audiences', 'Audiences', 'https://audiences.me', 'nexusphp', '/download.php?id={id}&passkey={passkey}', 1),
    ('chdbits', 'CHDBits', 'https://chdbits.co', 'nexusphp', '/download.php?id={id}&passkey={passkey}', 1),
    ('ttg', 'TTG', 'https://totheglory.im', 'nexusphp', '/dl/{id}/{passkey}', 1),
    ('blutopia', 'Blutopia', 'https://blutopia.cc', 'unit3d', '/torrent/download/{id}.{passkey}', 1),
    ('aither', 'Aither', 'https://aither.cc', 'unit3d', '/torrent/download/{id}.{passkey}', 1),
    ('redacted', 'Redacted', 'https://redacted.ch', 'gazelle', '/torrents.php?action=download&id={id}&authkey={authkey}&torrent_pass={passkey}', 1),
    ('orpheus', 'Orpheus', 'https://orpheus.network', 'gazelle', '/torrents.php?action=download&id={id}&authkey={authkey}&torrent_pass={passkey}', 1);

INSERT INTO site_domains (domain, site_id) VALUES
    ('m-team.cc', 'mteam'),
    ('hdsky.me', 'hdsky'),
    ('ourbits.club', 'ourbits'),
    ('pterclub.com', 'pterclub'),
    ('hdhome.org', 'hdhome'),
    ('audiences.me', 'audiences'),
    ('chdbits.co', 'chdbits'),
    ('totheglory.im', 'ttg'),
    ('blutopia.cc', 'blutopia'),
    ('aither.cc', 'aither'),
    ('redacted.ch', 'redacted'),
    ('flacsfor.me', 'redacted'),
    ('orpheus.network', 'orpheus');
