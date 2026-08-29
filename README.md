# Graft

[![license](https://img.shields.io/github/license/Lynthar/Graft)](LICENSE)
[![status](https://img.shields.io/badge/status-work%20in%20progress-orange)](#status)

Work-in-progress Rust rewrite of a PT cross-seeding tool. Not usable yet: the build chain is broken.

> **Read this before you clone.** The backend and frontend are written, but the
> build doesn't complete, there are no releases, no published images, and the
> credential handling is not fit for anything but a throwaway environment. Watch
> it if you like; don't deploy it.

Cross-seeding for private trackers: take a torrent you already have in one
client, fingerprint its content, find the same content on other trackers, and add
it back to the client.

It descends from IYUUPlus. I wrote the Rust source from scratch and embedded the
web UI in a single binary, but this repository carries the full upstream git
history, so most of its commits are not mine. Upstream continues at
[ledccn/iyuuplus-dev](https://github.com/ledccn/iyuuplus-dev).

## Status

**What's written**: sixteen HTTP endpoints with real implementations, working
qBittorrent WebUI and Transmission RPC clients that genuinely add torrents, the
preview-and-execute cross-seed flow, and six frontend pages wired to the API.

**What's broken**: `cargo build` fails. The backend embeds the frontend from
`web/dist`, which isn't in the repository and isn't built first, so compilation
stops there. The four CI workflows point at Dockerfiles that don't exist and have
never run.

**What isn't written**: the scheduler and any automatic re-seeding, RSS or
subscriptions, notifications, tracker search, API tokens, and internationalisation.

## How it works

Torrents are imported from a client you already run — that's the only source of
content, so it can only match things you already have. Each one is fingerprinted
by file layout and size. When you ask it to cross-seed, it looks for the same
fingerprint on the trackers you've configured, downloads the matching `.torrent`,
and adds it back to the client pointing at the existing files.

Thirteen tracker templates ship with it, across three tracker platforms.

## Building

Only one path currently works, because the Dockerfile builds the frontend before
the Rust code:

```bash
git clone https://github.com/Lynthar/Graft.git
cd Graft
docker compose up -d
```

Building by hand needs the same ordering:

```bash
cd web && npm install && npm run build && cd ..
cargo build --release
```

The instructions you'd expect — downloading a release binary, or pulling a
published image — don't work. Neither exists.

It listens on `0.0.0.0:3000` and creates `./data/graft.db` in the working
directory.

## Configuration

Everything is optional; the defaults work. Configuration is read from
`config.toml`, then `./data/config.toml`, then the platform config directory,
with environment variables taking precedence.

| Key | Default |
|---|---|
| `server.host` | `0.0.0.0` |
| `server.port` | `3000` |
| `database.path` | `./data/graft.db` |

`GRAFT_HOST`, `GRAFT_PORT`, `GRAFT_DATA_DIR`, `GRAFT_DB_PATH` and `RUST_LOG`
override them.

The `[reseed]` and `[logging]` sections in the example file are not read by the
current code.

## Limitations

- **Cross-seeding depends on extracting a torrent id from the tracker's announce
  URL.** Many trackers don't put one there, and when it can't be found the
  operation fails with exactly that message. How often it succeeds in practice
  hasn't been measured.
- **There's no discovery.** It can only match content already present in your
  client — no tracker search, no RSS, no shared hash database.
- **Gazelle trackers can't actually download**: the auth key field is never
  populated, so the download URL comes out incomplete.
- **Custom trackers added through the UI won't be recognised** — recognition uses
  a compiled-in table, not the database.
- **No scheduling.** Cross-seeding is manual, one click at a time.
- **No database migration path** — there's no version table, so a schema change
  means handling old databases by hand.

## Security

**Do not expose this to any network you don't control.** As it stands:

- Downloader passwords and tracker passkeys are **stored in plain text**.
- The web UI has **no authentication of any kind**.
- CORS is fully permissive, and it binds `0.0.0.0` by default.

Together that means anyone who can reach port 3000 can read every credential you
have entered. Until that's fixed, run it only on a host you trust completely,
bound to loopback.

## License

MIT — see [LICENSE](LICENSE).

The copyright line in `LICENSE` still names the upstream PHP framework's author
rather than this project's.
