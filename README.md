# Graft

[![license](https://img.shields.io/github/license/Lynthar/Graft)](LICENSE)
[![status](https://img.shields.io/badge/status-work%20in%20progress-orange)](#status)

Work-in-progress Rust rewrite of a PT cross-seeding tool. It finds cross-seeds on NexusPHP trackers by piece hash, but hasn't yet been run end to end against real trackers.

> **Read this before you clone.** The cross-seeding flow is written and tested
> against mock servers, not yet against real trackers. There are no releases or
> published images, the database layout may still change without an upgrade
> path, and credentials are stored in plain text. Watch it if you like; don't
> deploy it.

Cross-seeding for private trackers: take the torrents you already seed, find
the same content on other trackers you belong to, and add those torrents to your
client so the data you already have seeds there too.

It descends from IYUUPlus. I wrote the Rust source from scratch and embedded the
web UI in a single binary, but this repository carries the full upstream git
history, so most of its commits are not mine. Upstream continues at
[ledccn/iyuuplus-dev](https://github.com/ledccn/iyuuplus-dev).

## Status

**What works, against mocks**: reading a qBittorrent client, asking NexusPHP
trackers which of its contents they carry, previewing the candidates, and adding
the ones you confirm to qBittorrent. The test suite runs the real binary against
a mock qBittorrent and mock trackers. Adding to Transmission is written but not
covered by those tests.

**What hasn't been checked**: a full run against real trackers. Their pieces-hash
endpoint was probed with a real account on three NexusPHP sites (two answer, one
doesn't have it), but downloading and adding through Graft hasn't been tried on
a live client yet.

**What isn't written**: lookups on Unit3D and Gazelle trackers, importing a
`.torrent` by hand, hard-link cross-seeding for files laid out differently, the
scheduler and any automatic re-seeding, notifications, and internationalisation.

## How it works

1. Graft reads the torrents in a qBittorrent client and recognises each one's
   tracker from its tracker domain.
2. For every complete torrent it computes the SHA-1 of the torrent's piece
   hashes and asks each NexusPHP tracker you choose, through
   `POST /api/pieces-hash`, whether it has a torrent with the same pieces.
   Nothing else leaves your machine: the lookups go only to trackers you are a
   member of, signed with your own passkey.
3. The preview lists every match. You pick which ones to add.
4. For each one it downloads the tracker's `.torrent`, checks that its pieces
   and file layout match the files you already have, and adds it to the client
   stopped, tagged `graft`, at the existing save path. Your client checks the
   data before anything seeds; starting the torrents is up to you.

Requests to each tracker are spaced out (10 a minute by default) and capped per
day (20 downloads by default); both are set per tracker. Twelve trackers are
built in, across three tracker platforms, and you can add your own.

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

Unknown keys, or an environment variable that can't be parsed, stop Graft at
start-up rather than being ignored.

## Limitations

- **Only NexusPHP trackers can be searched**, and only those running a release
  from July 2023 or later; older or heavily modified ones answer 404. Unit3D and
  Gazelle have no comparable endpoint.
- **qBittorrent only, as the source.** Transmission doesn't report piece hashes;
  it can still be the client torrents are added to.
- **Same layout only.** A match whose file names or folders differ from yours is
  listed but not added.
- **No scheduling.** Cross-seeding is manual: preview, pick, add.
- **No stable database yet.** Until the first release the schema can change; a
  database from an older build is refused at start-up and has to be recreated.

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
