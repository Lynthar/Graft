# Graft

[![license](https://img.shields.io/github/license/Lynthar/Graft)](LICENSE)
![status](https://img.shields.io/badge/status-in%20development-orange)

Self-hosted cross-seeding for private trackers, with no cloud index. Still in development.

> **Still in development.** This README describes Graft as it will be when
> finished, and not all of it works yet. Until then there are no releases, and
> the database may have to be recreated after an update.

Cross-seeding for private trackers: take the torrents you already seed, find
the same content on other trackers you belong to, and add those torrents to your
client so the data you already have seeds there too.

Everything runs on your machine. There is no cloud index and no account to
register: Graft talks only to your torrent client and the trackers you are a
member of, and sends nothing anywhere else.

It descends from IYUUPlus. I wrote the Rust source from scratch and embedded the
web UI in a single binary, but this repository carries the full upstream git
history, so most of its commits are not mine. Upstream continues at
[ledccn/iyuuplus-dev](https://github.com/ledccn/iyuuplus-dev).

## Features

- **Clients**: qBittorrent and Transmission, checked as soon as you add them.
- **Trackers**: twelve built in, across NexusPHP, Unit3D and Gazelle; your own
  are added the same way. Each asks only for the credentials its platform needs.
- **Finding matches**: NexusPHP trackers are asked directly, by piece hash, which
  of your contents they carry. For any other tracker, import its `.torrent` by
  hand.
- **Preview first**: every match shows where it comes from and goes to, how sure
  Graft is and why, and the save path it will use. Leave out single matches or a
  whole save directory.
- **Careful adding**: torrents are added stopped, at the existing save path,
  tagged `graft`, and your client checks the data before anything seeds. A run
  can be stopped and started again without adding anything twice, and can add
  to a different client from the one it read.
- **Different layouts**: when a tracker's torrent names files or folders
  differently, Graft can hard-link your data into the layout it expects, leaving
  the originals alone, and download files you don't have. Each one is asked
  separately.
- **History**: every run records what was added where, what was skipped, and
  what failed at which step.
- **Easy on trackers**: requests to each tracker are spaced out and downloads
  are capped per day, both set per tracker.
- The web UI is in Chinese.

## How it works

1. Graft reads the torrents in a qBittorrent client and recognises each one's
   tracker from its tracker domain.
2. For every complete torrent it computes the SHA-1 of the torrent's piece
   hashes and asks each NexusPHP tracker you choose, through
   `POST /api/pieces-hash`, whether it has a torrent with the same pieces. The
   lookups go only to trackers you are a member of, signed with your own
   passkey.
3. The preview lists every match. You pick which ones to add.
4. For each one it downloads the tracker's `.torrent`, checks that its pieces
   and file layout match the files you already have, and adds it to the client
   stopped, tagged `graft`, at the existing save path. Your client checks the
   data before anything seeds; starting the torrents is up to you.

## Building

With Docker:

```bash
git clone https://github.com/Lynthar/Graft.git
cd Graft
echo 'GRAFT_PASSWORD=choose-a-password' > .env
echo "PUID=$(id -u)" >> .env
echo "PGID=$(id -g)" >> .env
docker compose up -d
```

Inside the container Graft listens on all interfaces, so it needs a password;
compose reads it from `.env` and refuses to start without one. Graft runs as
the user and group in `PUID` and `PGID` (1000 if unset) and hands `./data` to
them, so the database there is yours to read and back up.

By hand, with Node.js 24 and a current stable Rust:

```bash
cd web && npm ci && npm run build && cd ..
cargo build --release
```

Build the frontend first. Without `web/dist` the binary still compiles, but
serves a page asking you to build the frontend.

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
| `server.password` | none; required unless `server.host` is loopback |

`GRAFT_HOST`, `GRAFT_PORT`, `GRAFT_PASSWORD`, `GRAFT_DATA_DIR`, `GRAFT_DB_PATH`
and `RUST_LOG` override them.

Unknown keys, or an environment variable that can't be parsed, stop Graft at
start-up rather than being ignored.

## Limitations

- **Direct lookups need NexusPHP** from July 2023 or later; older or heavily
  modified ones answer 404, and Unit3D and Gazelle have no comparable endpoint.
  For those, import `.torrent` files by hand.
- **qBittorrent is the source for lookups.** Transmission doesn't report piece
  hashes; it can still be the client torrents are added to.
- **Hard links need the data at hand.** Graft must see the client's data
  directory, on the same file system as the links, so not with a remote client
  or a container without the data volume.
- **The interface is in Chinese only.**
- **No scheduling or notifications.** Cross-seeding is manual: preview, pick,
  add.
- **Not for the public internet.** There is no TLS; for remote access, put it
  behind your own reverse proxy or VPN.

## Security

- Downloader passwords and tracker passkeys are **stored in plain text**, in a
  database file only its owner can read. They are never logged or shown back by
  the interface, but anyone who can read the file has them.
- It listens on `127.0.0.1` by default. Listening on any other address requires
  a password, and Graft won't start without one. Each device logs in once and
  stays logged in for 30 days after it was last used; changing the password logs
  every device out.
- Without TLS the password and the session cookie cross the network in the
  clear. To reach Graft from outside your own network, put it behind a reverse
  proxy with TLS that forwards the original `Host` header, or use a VPN.
- Other web pages can't drive it: requests they send are refused by their
  `Origin`, and without a password Graft answers only requests addressed to this
  machine, which also stops DNS rebinding.
- Trackers are reached over HTTPS, and each passkey is sent only to its own
  tracker.

## License

MIT — see [LICENSE](LICENSE).

The copyright line in `LICENSE` still names the upstream PHP framework's author
rather than this project's.
