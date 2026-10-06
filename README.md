# Graft

[![license](https://img.shields.io/github/license/Lynthar/Graft)](LICENSE)
[![status](https://img.shields.io/badge/status-work%20in%20progress-orange)](#status)

Work-in-progress Rust rewrite of a PT cross-seeding tool. Not usable yet: it builds, but finds nothing it can cross-seed.

> **Read this before you clone.** The backend and frontend are written and the
> build works, but the cross-seeding flow can't produce a result yet, there are
> no releases or published images, and credentials are stored in plain text.
> Watch it if you like; don't deploy it.

Cross-seeding for private trackers: take a torrent you already have in one
client, fingerprint its content, find the same content on other trackers, and add
it back to the client.

It descends from IYUUPlus. I wrote the Rust source from scratch and embedded the
web UI in a single binary, but this repository carries the full upstream git
history, so most of its commits are not mine. Upstream continues at
[ledccn/iyuuplus-dev](https://github.com/ledccn/iyuuplus-dev).

## Status

**What's written**: fifteen HTTP endpoints with real implementations, working
qBittorrent WebUI and Transmission RPC clients that genuinely add torrents, the
preview-and-execute cross-seed flow, and six frontend pages wired to the API.
CI type-checks and builds the frontend, builds and tests the backend, and
builds the Docker image.

**What doesn't work**: matching never yields a torrent it can add. Candidates
come from an index of torrents already in your own clients, and a tracker's
announce URL carries your passkey, not a torrent id — so every match points at
something you already seed, with no id to download it by. The planned fix is to
ask each tracker directly by piece hash (NexusPHP's `/api/pieces-hash`), which
isn't written yet.

**What isn't written**: the scheduler and any automatic re-seeding, RSS or
subscriptions, notifications, tracker search, API tokens, and internationalisation.

## How it works

Torrents are imported from a client you already run — that's the only source of
content, so it can only match things you already have. Each one is fingerprinted
by file layout and size. When you ask it to cross-seed, it looks for the same
fingerprint on the trackers you've configured, downloads the matching `.torrent`,
and adds it back to the client pointing at the existing files.

Twelve trackers are built in, across three tracker platforms.

## Building

With Docker:

```bash
git clone https://github.com/Lynthar/Graft.git
cd Graft
docker compose up -d
```

By hand, with Node.js 24 and a current stable Rust:

```bash
cd web && npm ci && npm run build && cd ..
cargo build --release
```

Build the frontend first. Without `web/dist` the binary still compiles, but
serves a page asking you to build the frontend.

There are no release binaries or published images.

It listens on `127.0.0.1:3000` and creates `./data/graft.db` in the working
directory. The compose file publishes the port on `127.0.0.1` only.

## Configuration

Everything is optional; the defaults work. Configuration is read from
`config.toml`, then `./data/config.toml`, then the platform config directory,
with environment variables taking precedence.

| Key | Default |
|---|---|
| `server.host` | `127.0.0.1` |
| `server.port` | `3000` |
| `database.path` | `./data/graft.db` |

`GRAFT_HOST`, `GRAFT_PORT`, `GRAFT_DATA_DIR`, `GRAFT_DB_PATH` and `RUST_LOG`
override them.

The `[reseed]` and `[logging]` sections in the example file are not read by the
current code.

## Limitations

- **Importing fails if your client holds torrents from a tracker you haven't
  added.** Recognition uses a compiled-in table of 22 trackers, and any torrent
  from one that isn't configured stops the whole import with a foreign-key
  error. Ten of those 22 aren't among the built-in trackers you can add.
- **There's no discovery.** It can only match content already present in your
  client — no tracker search, no RSS, no shared hash database.
- **Gazelle trackers can't download**: their download URL needs an auth key that
  can't be configured yet, so the attempt stops with "Missing authkey".
- **Custom trackers added through the UI won't be recognised** — recognition uses
  the compiled-in table, not the database.
- **No scheduling.** Cross-seeding is manual, one click at a time.
- **No database migration path** — there's no version table, so a schema change
  means handling old databases by hand.

## Security

**Do not expose this to any network you don't control.** As it stands:

- Downloader passwords and tracker passkeys are **stored in plain text**, in a
  database file created with default permissions.
- The web UI has **no authentication of any kind**.
- It binds `127.0.0.1` by default and sends no CORS headers, so ordinary web
  pages can't read its responses. It doesn't yet check the `Host` or `Origin`
  header, so a DNS-rebinding page could still drive it.

Anything that can reach the port can read and change every setting, and can make
it send your passkeys to an address of its choosing. Keep it on loopback, on a
host you trust.

## License

MIT — see [LICENSE](LICENSE).

The copyright line in `LICENSE` still names the upstream PHP framework's author
rather than this project's.
