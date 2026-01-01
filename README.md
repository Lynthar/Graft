# 🌿 Graft

A lightweight, self-hosted PT cross-seeding tool.

**Graft** (嫁接) helps you automatically cross-seed torrents across multiple PT sites by matching content fingerprints locally, without relying on cloud services.

## Features

- 🔒 **Privacy First**: All data stays local, no cloud dependencies
- 🚀 **Single Binary**: One executable, no runtime dependencies
- 🎯 **Smart Matching**: Content fingerprinting for accurate cross-site matching
- 🌐 **Multi-Client**: Supports qBittorrent and Transmission
- 🎨 **Modern UI**: Clean, responsive web interface
- ⚡ **Fast**: Built with Rust for high performance

## Quick Start

### Binary Release

```bash
# Download the latest release
wget https://github.com/lynthar/graft/releases/latest/download/graft-linux-amd64.tar.gz
tar -xzf graft-linux-amd64.tar.gz

# Run
./graft
```

### Docker

```bash
docker run -d \
  --name graft \
  -p 3000:3000 \
  -v ./data:/app/data \
  ghcr.io/lynthar/graft:latest
```

### Docker Compose

```yaml
version: '3.8'
services:
  graft:
    image: ghcr.io/lynthar/graft:latest
    container_name: graft
    restart: unless-stopped
    ports:
      - "3000:3000"
    volumes:
      - ./data:/app/data
    environment:
      - TZ=Asia/Shanghai
```

## How It Works

1. **Index Building**: Import torrents from your download clients
2. **Site Identification**: Automatically identify sites from tracker URLs
3. **Content Fingerprinting**: Calculate fingerprints based on file structure (size, count, largest file)
4. **Cross-Site Matching**: Find matching content across different sites locally
5. **Reseed**: Download and add torrents to your client with correct save paths

```
┌─────────────┐
│  Downloader │ ──→ Extract Files ──→ Calculate Fingerprint ──→ Build Index
└─────────────┘                                                      │
                                                                     ▼
┌─────────────┐                                              ┌─────────────┐
│  PT Site A  │ ◄──────── Match Fingerprints ────────────── │  PT Site B  │
└─────────────┘                                              └─────────────┘
```

## Comparison with IYUU

| Feature | IYUU | Graft |
|---------|------|-------|
| Hash Matching | Cloud API | **Local Database** |
| Index Source | Cloud-maintained | **From your downloader** |
| User Auth | WeChat binding | **None required** |
| Deployment | PHP + MySQL | **Single binary** |
| Site Config | Cloud-maintained | **Built-in + Community** |
| Data Privacy | Hash uploaded | **Data stays local** |

## Tech Stack

- **Backend**: Rust + Axum + SQLite
- **Frontend**: SolidJS + Tailwind CSS + DaisyUI
- **Packaging**: Single binary / Docker

## Supported Sites

### Built-in Templates

- **NexusPHP**: M-Team, HDSky, OurBits, PTer, HDHome, CHDBits, TTG, and more
- **Unit3D**: Blutopia, Aither
- **Gazelle**: Redacted, Orpheus

### Custom Sites

You can add custom site configurations through the web UI.

## Configuration

Copy `config.example.toml` to `config.toml`:

```toml
[server]
host = "0.0.0.0"
port = 3000

[database]
path = "./data/graft.db"

[reseed]
default_paused = false
request_interval_ms = 500
max_per_run = 100
```

## Development

### Prerequisites

- Rust 1.75+
- Node.js 20+

### Build

```bash
# Clone
git clone https://github.com/lynthar/graft.git
cd graft

# Build frontend
cd web && npm install && npm run build && cd ..

# Build backend
cargo build --release
```

### Run in development

```bash
# Terminal 1: Backend
cargo run

# Terminal 2: Frontend (with hot reload)
cd web && npm run dev
```

## License

MIT

## Credits

Inspired by [IYUUPlus](https://github.com/ledccn/iyuuplus-dev), rebuilt from scratch with a focus on privacy and simplicity.
