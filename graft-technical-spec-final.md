# Graft - PT 辅种工具技术方案

> 版本：1.0.0  
> 日期：2025-01-01  
> 作者：Arthur Evans

---

## 目录

1. [设计理念](#1-设计理念)
2. [技术选型](#2-技术选型)
3. [系统架构](#3-系统架构)
4. [核心功能设计](#4-核心功能设计)
5. [数据模型](#5-数据模型)
6. [API 设计](#6-api-设计)
7. [前端设计](#7-前端设计)
8. [部署方案](#8-部署方案)
9. [开发路线图](#9-开发路线图)
10. [与 IYUU 的对比](#10-与-iyuu-的对比)

---

## 1. 设计理念

### 1.1 核心原则

| 原则 | 说明 |
|------|------|
| **专注** | 只做辅种一件事，做到极致 |
| **轻量** | 单二进制分发，最小化依赖 |
| **自主** | 完全本地运行，无云依赖，无需注册 |
| **透明** | 开源、行为可预测、日志清晰 |
| **优雅** | 简洁的 UI，流畅的交互体验 |

### 1.2 设计哲学

```
"嫁接"（Graft）—— 将同一内容无缝接入多个站点

┌─────────────┐
│  下载器种子  │ ──→ 自动识别站点 ──→ 建立本地索引 ──→ 跨站辅种
└─────────────┘
       ↑
  用户完全掌控，数据不出本地
```

### 1.3 与 IYUU 的核心差异

| 维度 | IYUU | Graft |
|------|------|-------|
| Hash 匹配 | 依赖云端 API | **本地数据库** |
| 索引来源 | 云端维护 | **从用户下载器直接读取** |
| 用户认证 | 微信扫码绑定 | **无需任何认证** |
| 部署方式 | PHP + MySQL + 多扩展 | **单二进制文件** |
| 站点配置 | 云端维护更新 | **内置 + 社区仓库订阅** |
| 数据隐私 | Hash 上传云端 | **数据不出本地** |

---

## 2. 技术选型

### 2.1 技术栈总览

```
┌────────────────────────────────────────────────────────┐
│                      Graft                             │
├────────────────────────────────────────────────────────┤
│  Frontend    │  SolidJS + Tailwind CSS + DaisyUI       │
├──────────────┼─────────────────────────────────────────┤
│  Backend     │  Rust + Axum + Tower                    │
├──────────────┼─────────────────────────────────────────┤
│  Database    │  SQLite (rusqlite) + FTS5               │
├──────────────┼─────────────────────────────────────────┤
│  Packaging   │  单二进制 / Docker                      │
└──────────────┴─────────────────────────────────────────┘
```

### 2.2 后端选型：Rust + Axum

**为什么选 Rust？**

| 优势 | 说明 |
|------|------|
| 单二进制 | 编译后一个可执行文件，无运行时依赖 |
| 高性能 | 内存安全 + 零成本抽象，适合长时间运行 |
| 跨平台 | 轻松编译到 Linux/Windows/macOS |
| 生态成熟 | Axum/Tokio 异步生态完善 |

**为什么选 Axum？**

```rust
// Axum 的优雅 API 设计
async fn reseed_handler(
    State(state): State<AppState>,
    Json(req): Json<ReseedRequest>,
) -> Result<Json<ReseedResponse>, AppError> {
    let result = state.reseed_service.execute(req).await?;
    Ok(Json(result))
}
```

- Tower 生态兼容，中间件丰富
- 类型安全的路由提取
- 原生支持 WebSocket（用于实时日志）

### 2.3 前端选型：SolidJS + Tailwind

**为什么选 SolidJS 而非 React/Vue？**

| 对比 | React | Vue | SolidJS |
|------|-------|-----|---------|
| 打包体积 | ~40KB | ~30KB | **~7KB** |
| 响应式 | Virtual DOM | Proxy | **细粒度响应式** |
| 性能 | 良好 | 良好 | **极佳** |
| 学习曲线 | 中等 | 低 | 低（类 React 语法） |

**为什么选 Tailwind + DaisyUI？**

- Tailwind：原子化 CSS，打包时自动 tree-shaking
- DaisyUI：提供预设组件类，减少重复代码
- 总 CSS 体积可控制在 10KB 以内

### 2.4 数据库选型：SQLite

**为什么是 SQLite？**

| 优势 | 说明 |
|------|------|
| 零配置 | 单文件数据库，无需安装服务 |
| 嵌入式 | 编译进二进制，一起分发 |
| 高性能 | 对于本地应用，读写速度极快 |
| FTS5 | 内置全文搜索，用于站点/种子搜索 |
| 可靠 | 成熟稳定，广泛使用 |

### 2.5 依赖清单

```toml
# Cargo.toml
[package]
name = "graft"
version = "0.1.0"
edition = "2021"
description = "A lightweight, self-hosted PT cross-seeding tool"
license = "MIT"
repository = "https://github.com/lynthar/graft"

[dependencies]
# Web 框架
axum = "0.7"
tokio = { version = "1", features = ["full"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "fs", "compression-gzip", "trace"] }

# 序列化
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"

# 数据库
rusqlite = { version = "0.31", features = ["bundled", "fts5"] }

# HTTP 客户端 (调用下载器 API)
reqwest = { version = "0.12", features = ["json", "cookies"] }

# 种子解析
lava_torrent = "0.9"
sha1 = "0.10"

# 异步任务调度
tokio-cron-scheduler = "0.10"

# 日志
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# 配置
config = "0.14"
dotenvy = "0.15"

# 前端资源嵌入
rust-embed = "8"

# 工具
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4", "serde"] }
thiserror = "1"
anyhow = "1"
async-trait = "0.1"
urlencoding = "2"
base64 = "0.21"
```

---

## 3. 系统架构

### 3.1 整体架构图

```
┌─────────────────────────────────────────────────────────────────┐
│                           Graft                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐      │
│  │   Web UI     │    │   REST API   │    │  WebSocket   │      │
│  │  (SolidJS)   │◄──►│   (Axum)     │◄──►│  (实时日志)  │      │
│  └──────────────┘    └──────┬───────┘    └──────────────┘      │
│                             │                                   │
│         ┌───────────────────┼───────────────────┐               │
│         │                   │                   │               │
│         ▼                   ▼                   ▼               │
│  ┌────────────┐     ┌─────────────┐     ┌─────────────┐        │
│  │  Reseed    │     │   Client    │     │    Site     │        │
│  │  Service   │     │   Manager   │     │   Manager   │        │
│  └─────┬──────┘     └──────┬──────┘     └──────┬──────┘        │
│        │                   │                   │                │
│        │           ┌───────▼───────┐           │                │
│        │           │    Index      │           │                │
│        └──────────►│   Service     │◄──────────┘                │
│                    └───────┬───────┘                            │
│                            │                                    │
│                     ┌──────▼──────┐                             │
│                     │   SQLite    │                             │
│                     │  Database   │                             │
│                     └─────────────┘                             │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│                      External Systems                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐      │
│  │ qBittorrent  │    │ Transmission │    │  PT Sites    │      │
│  │    API       │    │     API      │    │  (下载种子)  │      │
│  └──────────────┘    └──────────────┘    └──────────────┘      │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 3.2 模块划分

```
graft/
├── src/
│   ├── main.rs                 # 入口点
│   ├── lib.rs                  # 库导出
│   │
│   ├── api/                    # HTTP API 层
│   │   ├── mod.rs
│   │   ├── router.rs           # 路由定义
│   │   ├── handlers/           # 请求处理器
│   │   │   ├── mod.rs
│   │   │   ├── client.rs       # 下载器管理
│   │   │   ├── site.rs         # 站点管理
│   │   │   ├── reseed.rs       # 辅种操作
│   │   │   ├── task.rs         # 任务管理
│   │   │   ├── index.rs        # 索引管理
│   │   │   └── system.rs       # 系统设置
│   │   ├── middleware.rs       # 中间件
│   │   ├── error.rs            # 错误处理
│   │   └── ws.rs               # WebSocket 处理
│   │
│   ├── service/                # 业务逻辑层
│   │   ├── mod.rs
│   │   ├── reseed.rs           # 辅种核心逻辑
│   │   ├── index.rs            # 索引服务（从下载器导入）
│   │   ├── scheduler.rs        # 定时任务调度
│   │   └── notification.rs     # 通知服务
│   │
│   ├── client/                 # 下载器抽象层
│   │   ├── mod.rs
│   │   ├── traits.rs           # 统一接口定义
│   │   ├── qbittorrent.rs      # qBittorrent 实现
│   │   ├── transmission.rs     # Transmission 实现
│   │   └── models.rs           # 共享数据模型
│   │
│   ├── site/                   # 站点适配层
│   │   ├── mod.rs
│   │   ├── manager.rs          # 站点管理器
│   │   ├── template.rs         # 站点模板 trait
│   │   ├── tracker.rs          # Tracker URL 识别
│   │   ├── templates/          # 具体模板实现
│   │   │   ├── mod.rs
│   │   │   ├── nexusphp.rs     # NexusPHP 通用模板
│   │   │   ├── unit3d.rs       # Unit3D 通用模板
│   │   │   └── gazelle.rs      # Gazelle 通用模板
│   │   └── builtin/            # 内置站点配置
│   │       ├── mod.rs
│   │       └── *.yaml          # 各站点 YAML 配置
│   │
│   ├── db/                     # 数据访问层
│   │   ├── mod.rs
│   │   ├── connection.rs       # 数据库连接
│   │   ├── migration.rs        # 数据库迁移
│   │   └── repository/         # 数据仓库
│   │       ├── mod.rs
│   │       ├── client.rs
│   │       ├── site.rs
│   │       ├── index.rs
│   │       ├── task.rs
│   │       └── history.rs
│   │
│   ├── config/                 # 配置管理
│   │   ├── mod.rs
│   │   └── settings.rs
│   │
│   └── utils/                  # 工具函数
│       ├── mod.rs
│       ├── crypto.rs           # 加密工具（密码存储）
│       └── http.rs             # HTTP 工具
│
├── web/                        # 前端项目
│   ├── src/
│   │   ├── index.tsx
│   │   ├── App.tsx
│   │   ├── components/
│   │   ├── pages/
│   │   ├── stores/
│   │   └── api/
│   ├── package.json
│   ├── vite.config.ts
│   └── tailwind.config.js
│
├── migrations/                 # SQL 迁移文件
│   └── 001_initial.sql
│
├── Cargo.toml
├── Cargo.lock
├── Dockerfile
├── docker-compose.yml
├── config.example.toml
├── LICENSE
└── README.md
```

### 3.3 核心流程

#### 3.3.1 索引构建流程（从下载器导入）

```
┌─────────────────────────────────────────────────────────────────┐
│                    索引构建流程                                  │
└─────────────────────────────────────────────────────────────────┘

     用户点击「从下载器导入」
            │
            ▼
    ┌───────────────┐
    │ 1. 连接下载器  │  ←── qBittorrent / Transmission API
    └───────┬───────┘
            │
            ▼
    ┌───────────────┐
    │ 2. 获取种子列表│  ←── 所有正在做种的种子
    └───────┬───────┘
            │
            ▼
    ┌───────────────┐
    │ 3. 遍历每个种子│
    └───────┬───────┘
            │
            ▼
    ┌───────────────┐      从 tracker URL 提取域名
    │ 4. 识别站点    │  ←── kp.m-team.cc → mteam
    └───────┬───────┘      hdsky.me → hdsky
            │
            ▼
    ┌───────────────┐      从 tracker URL 提取
    │ 5. 提取信息    │  ←── info_hash, torrent_id, name, size
    └───────┬───────┘
            │
            ▼
    ┌───────────────┐
    │ 6. 写入索引库  │  ←── SQLite torrent_index 表
    └───────────────┘
```

#### 3.3.2 辅种执行流程

```
┌─────────────────────────────────────────────────────────────────┐
│                      辅种执行流程                                │
└─────────────────────────────────────────────────────────────────┘

     用户触发 / 定时任务
            │
            ▼
    ┌───────────────┐
    │ 1. 获取种子列表 │  ←── 从源下载器获取当前做种
    └───────┬───────┘
            │
            ▼
    ┌───────────────┐
    │ 2. 提取 Hash   │  ←── info_hash 列表
    └───────┬───────┘
            │
            ▼
    ┌───────────────┐
    │ 3. 本地匹配    │  ←── 查询本地 torrent_index 表
    └───────┬───────┘      找出在目标站点也存在的种子
            │
            ▼
    ┌───────────────┐
    │ 4. 筛选结果    │  ←── 排除已存在、排除黑名单站点
    └───────┬───────┘
            │
            ▼
    ┌───────────────┐
    │ 5. 下载种子    │  ←── 从目标站点下载 .torrent 文件
    └───────┬───────┘
            │
            ▼
    ┌───────────────┐
    │ 6. 推送下载器  │  ←── 添加种子到目标下载器
    └───────┬───────┘      设置相同的保存路径
            │
            ▼
    ┌───────────────┐
    │ 7. 记录结果    │  ←── 写入辅种历史
    └───────────────┘
```

---

## 4. 核心功能设计

### 4.1 功能模块总览

```
Graft 功能模块
├── 核心功能
│   ├── 从下载器导入索引（核心！）
│   ├── 辅种匹配与执行
│   └── 定时任务调度
│
├── 下载器管理
│   ├── 多客户端支持 (qBittorrent, Transmission)
│   ├── 连接测试
│   └── 种子列表查看
│
├── 站点管理
│   ├── 内置站点配置
│   ├── 自定义站点
│   ├── Cookie/Passkey 管理
│   └── 社区配置订阅（后续版本）
│
├── 辅助功能
│   ├── 操作日志（WebSocket 实时推送）
│   ├── 辅种历史记录
│   └── 统计面板
│
└── 系统设置
    ├── 基础配置
    └── 数据导出
```

### 4.2 下载器抽象层

```rust
// src/client/traits.rs

use async_trait::async_trait;
use crate::client::models::*;

/// 下载器统一接口
#[async_trait]
pub trait BitTorrentClient: Send + Sync {
    /// 获取客户端类型
    fn client_type(&self) -> ClientType;
    
    /// 测试连接
    async fn test_connection(&self) -> Result<bool>;
    
    /// 获取所有种子列表
    async fn get_torrents(&self) -> Result<Vec<TorrentInfo>>;
    
    /// 获取种子详情
    async fn get_torrent(&self, hash: &str) -> Result<Option<TorrentInfo>>;
    
    /// 添加种子（通过 .torrent 文件内容）
    async fn add_torrent(&self, torrent_bytes: &[u8], options: AddTorrentOptions) -> Result<String>;
    
    /// 删除种子
    async fn remove_torrent(&self, hash: &str, delete_files: bool) -> Result<()>;
    
    /// 暂停种子
    async fn pause_torrent(&self, hash: &str) -> Result<()>;
    
    /// 恢复种子
    async fn resume_torrent(&self, hash: &str) -> Result<()>;
    
    /// 强制重新校验
    async fn recheck_torrent(&self, hash: &str) -> Result<()>;
}

// src/client/models.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentInfo {
    pub hash: String,
    pub name: String,
    pub size: u64,
    pub progress: f64,
    pub state: TorrentState,
    pub save_path: String,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub tracker: Option<String>,        // 第一个 tracker URL
    pub trackers: Vec<String>,          // 所有 tracker URLs
    pub added_on: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddTorrentOptions {
    pub save_path: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub paused: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TorrentState {
    Downloading,
    Seeding,
    Paused,
    Checking,
    Error,
    Queued,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ClientType {
    QBittorrent,
    Transmission,
}
```

### 4.3 索引服务（核心功能）

```rust
// src/service/index.rs

use crate::client::traits::BitTorrentClient;
use crate::site::tracker::TrackerIdentifier;
use crate::db::repository::IndexRepository;

pub struct IndexService {
    index_repo: IndexRepository,
    tracker_identifier: TrackerIdentifier,
}

impl IndexService {
    /// 从下载器导入索引（核心功能）
    pub async fn import_from_client(
        &self,
        client: &dyn BitTorrentClient,
        client_id: &str,
    ) -> Result<ImportResult> {
        let torrents = client.get_torrents().await?;
        let mut result = ImportResult::default();
        
        for torrent in &torrents {
            result.total += 1;
            
            // 尝试从所有 tracker 中识别站点
            let site_info = self.identify_site_from_trackers(&torrent.trackers);
            
            match site_info {
                Some((site_id, torrent_id)) => {
                    // 检查是否已存在
                    if self.index_repo.exists(&torrent.hash, &site_id).await? {
                        result.skipped += 1;
                        continue;
                    }
                    
                    // 写入索引
                    self.index_repo.insert(IndexEntry {
                        info_hash: torrent.hash.clone(),
                        site_id: site_id.clone(),
                        torrent_id,
                        name: Some(torrent.name.clone()),
                        size: Some(torrent.size as i64),
                        source_client: Some(client_id.to_string()),
                    }).await?;
                    
                    result.imported += 1;
                }
                None => {
                    // 无法识别站点
                    result.unrecognized += 1;
                }
            }
        }
        
        Ok(result)
    }
    
    /// 从 tracker URLs 识别站点和种子 ID
    fn identify_site_from_trackers(&self, trackers: &[String]) -> Option<(String, String)> {
        for tracker in trackers {
            if let Some(result) = self.tracker_identifier.identify(tracker) {
                return Some(result);
            }
        }
        None
    }
    
    /// 获取索引统计
    pub async fn get_stats(&self) -> Result<IndexStats> {
        self.index_repo.get_stats().await
    }
    
    /// 清空索引
    pub async fn clear(&self) -> Result<()> {
        self.index_repo.clear().await
    }
    
    /// 按站点清空索引
    pub async fn clear_by_site(&self, site_id: &str) -> Result<()> {
        self.index_repo.clear_by_site(site_id).await
    }
}

#[derive(Debug, Default, Serialize)]
pub struct ImportResult {
    pub total: usize,
    pub imported: usize,
    pub skipped: usize,       // 已存在
    pub unrecognized: usize,  // 无法识别站点
}

#[derive(Debug, Serialize)]
pub struct IndexStats {
    pub total_entries: i64,
    pub sites: Vec<SiteIndexCount>,
}

#[derive(Debug, Serialize)]
pub struct SiteIndexCount {
    pub site_id: String,
    pub site_name: String,
    pub count: i64,
}
```

### 4.4 Tracker URL 识别器

```rust
// src/site/tracker.rs

use std::collections::HashMap;

pub struct TrackerIdentifier {
    /// domain -> site_id 映射
    domain_map: HashMap<String, String>,
    /// 站点的 tracker URL 模式
    patterns: HashMap<String, TrackerPattern>,
}

#[derive(Debug, Clone)]
pub struct TrackerPattern {
    pub site_id: String,
    /// 从 URL 中提取 torrent_id 的正则
    pub torrent_id_regex: Option<regex::Regex>,
}

impl TrackerIdentifier {
    pub fn new() -> Self {
        let mut identifier = Self {
            domain_map: HashMap::new(),
            patterns: HashMap::new(),
        };
        
        // 注册内置站点
        identifier.register_builtin_sites();
        
        identifier
    }
    
    fn register_builtin_sites(&mut self) {
        // NexusPHP 站点通常 tracker URL 格式：
        // https://domain/announce.php?passkey=xxx
        // 或 https://domain/tracker.php?passkey=xxx&torrent_id=123
        
        let sites = vec![
            ("m-team.cc", "mteam"),
            ("kp.m-team.cc", "mteam"),
            ("hdsky.me", "hdsky"),
            ("ourbits.club", "ourbits"),
            ("springsunday.net", "ssd"),
            ("pt.btschool.club", "btschool"),
            ("pterclub.com", "pter"),
            ("hdhome.org", "hdhome"),
            ("hdarea.club", "hdarea"),
            ("hdatmos.club", "hdatmos"),
            ("audiences.me", "aud"),
            ("hdfans.org", "hdfans"),
            ("hdtime.org", "hdtime"),
            ("1ptba.com", "1ptba"),
            ("hdzone.me", "hdzone"),
            ("pt.hdupt.com", "hdupt"),
            // Unit3D
            ("blutopia.cc", "blu"),
            ("aither.cc", "aither"),
            ("reelflix.xyz", "reelflix"),
            // Gazelle
            ("redacted.ch", "red"),
            ("orpheus.network", "ops"),
            // 更多站点...
        ];
        
        for (domain, site_id) in sites {
            self.domain_map.insert(domain.to_string(), site_id.to_string());
        }
    }
    
    /// 识别 tracker URL，返回 (site_id, torrent_id)
    pub fn identify(&self, tracker_url: &str) -> Option<(String, String)> {
        // 解析 URL
        let url = url::Url::parse(tracker_url).ok()?;
        let host = url.host_str()?;
        
        // 查找站点
        let site_id = self.find_site_by_host(host)?;
        
        // 尝试从 URL 提取 torrent_id
        let torrent_id = self.extract_torrent_id(tracker_url, &site_id)
            .unwrap_or_else(|| "unknown".to_string());
        
        Some((site_id, torrent_id))
    }
    
    fn find_site_by_host(&self, host: &str) -> Option<String> {
        // 直接匹配
        if let Some(site_id) = self.domain_map.get(host) {
            return Some(site_id.clone());
        }
        
        // 尝试去掉子域名匹配
        let parts: Vec<&str> = host.split('.').collect();
        if parts.len() >= 2 {
            let base_domain = parts[parts.len()-2..].join(".");
            if let Some(site_id) = self.domain_map.get(&base_domain) {
                return Some(site_id.clone());
            }
        }
        
        None
    }
    
    fn extract_torrent_id(&self, url: &str, _site_id: &str) -> Option<String> {
        // 常见模式：torrent_id=xxx 或 id=xxx
        let url = url::Url::parse(url).ok()?;
        
        for (key, value) in url.query_pairs() {
            if key == "torrent_id" || key == "id" {
                return Some(value.to_string());
            }
        }
        
        None
    }
    
    /// 动态添加站点识别规则
    pub fn register_site(&mut self, domain: &str, site_id: &str) {
        self.domain_map.insert(domain.to_string(), site_id.to_string());
    }
}
```

### 4.5 站点模板系统

```rust
// src/site/template.rs

use async_trait::async_trait;

/// 站点模板 - 定义站点的通用行为
#[async_trait]
pub trait SiteTemplate: Send + Sync {
    /// 获取站点基本信息
    fn info(&self) -> &SiteInfo;
    
    /// 构建种子下载链接
    fn build_download_url(&self, torrent_id: &str, passkey: &str) -> String;
    
    /// 下载种子文件
    async fn download_torrent(
        &self,
        http_client: &reqwest::Client,
        torrent_id: &str,
        passkey: &str,
    ) -> Result<Vec<u8>>;
}

#[derive(Debug, Clone, Deserialize)]
pub struct SiteInfo {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub template_type: TemplateType,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TemplateType {
    NexusPHP,
    Unit3D,
    Gazelle,
}

// src/site/templates/nexusphp.rs

pub struct NexusPHPTemplate {
    info: SiteInfo,
    download_pattern: String,  // 如 "/download.php?id={id}&passkey={passkey}"
}

#[async_trait]
impl SiteTemplate for NexusPHPTemplate {
    fn info(&self) -> &SiteInfo {
        &self.info
    }
    
    fn build_download_url(&self, torrent_id: &str, passkey: &str) -> String {
        let url = self.download_pattern
            .replace("{id}", torrent_id)
            .replace("{passkey}", passkey);
        format!("{}{}", self.info.base_url, url)
    }
    
    async fn download_torrent(
        &self,
        http_client: &reqwest::Client,
        torrent_id: &str,
        passkey: &str,
    ) -> Result<Vec<u8>> {
        let url = self.build_download_url(torrent_id, passkey);
        
        let response = http_client
            .get(&url)
            .send()
            .await?
            .error_for_status()?;
        
        let bytes = response.bytes().await?;
        Ok(bytes.to_vec())
    }
}
```

### 4.6 站点配置文件格式

```yaml
# 内置站点配置示例: src/site/builtin/mteam.yaml

id: mteam
name: M-Team
base_url: https://kp.m-team.cc
template: nexusphp

# Tracker 域名（用于识别）
tracker_domains:
  - m-team.cc
  - kp.m-team.cc
  - pt.m-team.cc

# 下载链接模式
download_pattern: "/download.php?id={id}&passkey={passkey}"

# 可选：请求速率限制
rate_limit:
  requests_per_minute: 10
```

```yaml
# 用户自定义站点配置示例

id: mysite
name: My Private Site
base_url: https://my.private.site
template: nexusphp

tracker_domains:
  - my.private.site

download_pattern: "/download.php?id={id}&passkey={passkey}"
```

### 4.7 辅种服务

```rust
// src/service/reseed.rs

pub struct ReseedService {
    client_manager: Arc<ClientManager>,
    site_manager: Arc<SiteManager>,
    index_repo: IndexRepository,
    history_repo: HistoryRepository,
    http_client: reqwest::Client,
}

impl ReseedService {
    /// 执行辅种
    pub async fn execute(&self, request: ReseedRequest) -> Result<ReseedResult> {
        let mut result = ReseedResult::default();
        
        // 1. 获取源下载器的种子列表
        let source_client = self.client_manager.get(&request.source_client)?;
        let torrents = source_client.get_torrents().await?;
        
        tracing::info!(
            "从 {} 获取到 {} 个种子",
            request.source_client,
            torrents.len()
        );
        
        // 2. 提取 info_hash 列表
        let hashes: Vec<String> = torrents.iter().map(|t| t.hash.clone()).collect();
        
        // 3. 本地匹配 - 查找在目标站点也存在的种子
        let matches = self.index_repo
            .find_matches(&hashes, &request.target_sites)
            .await?;
        
        tracing::info!("找到 {} 个可辅种匹配", matches.len());
        
        // 4. 获取目标下载器
        let target_client = self.client_manager.get(&request.target_client)?;
        
        // 5. 获取目标下载器已有的种子 hash（用于去重）
        let existing_hashes: HashSet<String> = target_client
            .get_torrents()
            .await?
            .into_iter()
            .map(|t| t.hash.to_lowercase())
            .collect();
        
        // 6. 处理每个匹配
        for match_entry in matches {
            result.total += 1;
            
            // 检查目标下载器是否已有此种子
            if existing_hashes.contains(&match_entry.info_hash.to_lowercase()) {
                result.skipped += 1;
                continue;
            }
            
            // 获取站点配置
            let site = match self.site_manager.get(&match_entry.site_id) {
                Some(s) => s,
                None => {
                    result.failed += 1;
                    continue;
                }
            };
            
            // 获取站点认证信息
            let site_auth = match self.site_manager.get_auth(&match_entry.site_id) {
                Some(a) => a,
                None => {
                    tracing::warn!("站点 {} 未配置认证信息", match_entry.site_id);
                    result.failed += 1;
                    continue;
                }
            };
            
            // 下载种子文件
            let torrent_bytes = match site.template
                .download_torrent(&self.http_client, &match_entry.torrent_id, &site_auth.passkey)
                .await
            {
                Ok(bytes) => bytes,
                Err(e) => {
                    tracing::warn!("下载种子失败: {} - {}", match_entry.name.as_deref().unwrap_or("unknown"), e);
                    result.failed += 1;
                    self.record_history(&match_entry, ReseedStatus::Failed, Some(e.to_string())).await;
                    continue;
                }
            };
            
            // 获取原种子的保存路径
            let original = torrents.iter().find(|t| t.hash == match_entry.info_hash);
            let save_path = original.map(|t| t.save_path.clone());
            
            // 添加到目标下载器
            match target_client.add_torrent(&torrent_bytes, AddTorrentOptions {
                save_path,
                paused: request.add_paused,
                ..Default::default()
            }).await {
                Ok(_) => {
                    result.success += 1;
                    tracing::info!(
                        "辅种成功: {} -> {}",
                        match_entry.name.as_deref().unwrap_or("unknown"),
                        match_entry.site_id
                    );
                    self.record_history(&match_entry, ReseedStatus::Success, None).await;
                }
                Err(e) => {
                    tracing::warn!("添加种子失败: {}", e);
                    result.failed += 1;
                    self.record_history(&match_entry, ReseedStatus::Failed, Some(e.to_string())).await;
                }
            }
            
            // 遵守速率限制
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        
        Ok(result)
    }
    
    /// 预览辅种（不实际执行）
    pub async fn preview(&self, request: ReseedRequest) -> Result<PreviewResult> {
        let source_client = self.client_manager.get(&request.source_client)?;
        let torrents = source_client.get_torrents().await?;
        
        let hashes: Vec<String> = torrents.iter().map(|t| t.hash.clone()).collect();
        let matches = self.index_repo
            .find_matches(&hashes, &request.target_sites)
            .await?;
        
        // 获取目标下载器已有的种子
        let target_client = self.client_manager.get(&request.target_client)?;
        let existing_hashes: HashSet<String> = target_client
            .get_torrents()
            .await?
            .into_iter()
            .map(|t| t.hash.to_lowercase())
            .collect();
        
        // 过滤已存在的
        let actionable: Vec<_> = matches
            .into_iter()
            .filter(|m| !existing_hashes.contains(&m.info_hash.to_lowercase()))
            .collect();
        
        let total_size: i64 = actionable.iter()
            .filter_map(|m| m.size)
            .sum();
        
        Ok(PreviewResult {
            matches: actionable,
            total_size,
        })
    }
    
    async fn record_history(&self, entry: &IndexEntry, status: ReseedStatus, message: Option<String>) {
        let _ = self.history_repo.insert(HistoryEntry {
            info_hash: entry.info_hash.clone(),
            target_site: entry.site_id.clone(),
            status,
            message,
            created_at: Utc::now(),
        }).await;
    }
}

#[derive(Debug, Deserialize)]
pub struct ReseedRequest {
    pub source_client: String,
    pub target_client: String,
    pub target_sites: Vec<String>,
    #[serde(default)]
    pub add_paused: bool,
}

#[derive(Debug, Default, Serialize)]
pub struct ReseedResult {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    pub skipped: usize,
}

#[derive(Debug, Serialize)]
pub struct PreviewResult {
    pub matches: Vec<IndexEntry>,
    pub total_size: i64,
}
```

### 4.8 定时任务调度

```rust
// src/service/scheduler.rs

use tokio_cron_scheduler::{Job, JobScheduler};

pub struct TaskScheduler {
    scheduler: JobScheduler,
    reseed_service: Arc<ReseedService>,
    index_service: Arc<IndexService>,
    client_manager: Arc<ClientManager>,
}

impl TaskScheduler {
    pub async fn new(
        reseed_service: Arc<ReseedService>,
        index_service: Arc<IndexService>,
        client_manager: Arc<ClientManager>,
    ) -> Result<Self> {
        let scheduler = JobScheduler::new().await?;
        Ok(Self {
            scheduler,
            reseed_service,
            index_service,
            client_manager,
        })
    }
    
    /// 添加定时辅种任务
    pub async fn add_reseed_job(&self, task: &ReseedTask) -> Result<Uuid> {
        let reseed_service = self.reseed_service.clone();
        let request = ReseedRequest {
            source_client: task.source_client.clone(),
            target_client: task.target_client.clone(),
            target_sites: task.target_sites.clone(),
            add_paused: task.add_paused,
        };
        
        let job = Job::new_async(task.cron_expression.as_str(), move |_uuid, _lock| {
            let service = reseed_service.clone();
            let req = request.clone();
            Box::pin(async move {
                tracing::info!("执行定时辅种任务");
                if let Err(e) = service.execute(req).await {
                    tracing::error!("定时辅种失败: {}", e);
                }
            })
        })?;
        
        let uuid = job.guid();
        self.scheduler.add(job).await?;
        Ok(uuid)
    }
    
    /// 添加定时索引更新任务
    pub async fn add_index_update_job(&self, client_id: String, cron: String) -> Result<Uuid> {
        let index_service = self.index_service.clone();
        let client_manager = self.client_manager.clone();
        
        let job = Job::new_async(cron.as_str(), move |_uuid, _lock| {
            let service = index_service.clone();
            let manager = client_manager.clone();
            let cid = client_id.clone();
            Box::pin(async move {
                tracing::info!("执行定时索引更新: {}", cid);
                if let Some(client) = manager.get(&cid) {
                    if let Err(e) = service.import_from_client(client.as_ref(), &cid).await {
                        tracing::error!("索引更新失败: {}", e);
                    }
                }
            })
        })?;
        
        let uuid = job.guid();
        self.scheduler.add(job).await?;
        Ok(uuid)
    }
    
    pub async fn start(&self) -> Result<()> {
        self.scheduler.start().await?;
        Ok(())
    }
    
    pub async fn shutdown(&self) -> Result<()> {
        self.scheduler.shutdown().await?;
        Ok(())
    }
}
```

---

## 5. 数据模型

### 5.1 数据库 Schema

```sql
-- migrations/001_initial.sql

-- 下载器配置
CREATE TABLE IF NOT EXISTS clients (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    client_type TEXT NOT NULL CHECK (client_type IN ('qbittorrent', 'transmission')),
    host TEXT NOT NULL,
    port INTEGER NOT NULL,
    username TEXT,
    password_encrypted TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 站点配置
CREATE TABLE IF NOT EXISTS sites (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    base_url TEXT NOT NULL,
    template_type TEXT NOT NULL,
    passkey TEXT,
    cookie_encrypted TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 种子索引（核心表）
CREATE TABLE IF NOT EXISTS torrent_index (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    info_hash TEXT NOT NULL,
    site_id TEXT NOT NULL,
    torrent_id TEXT NOT NULL,
    name TEXT,
    size INTEGER,
    source_client TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(info_hash, site_id),
    FOREIGN KEY (site_id) REFERENCES sites(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_torrent_hash ON torrent_index(info_hash);
CREATE INDEX IF NOT EXISTS idx_torrent_site ON torrent_index(site_id);

-- 辅种任务
CREATE TABLE IF NOT EXISTS reseed_tasks (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    source_client TEXT NOT NULL,
    target_client TEXT NOT NULL,
    target_sites TEXT NOT NULL,  -- JSON array
    cron_expression TEXT,
    add_paused INTEGER NOT NULL DEFAULT 0,
    enabled INTEGER NOT NULL DEFAULT 1,
    last_run_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (source_client) REFERENCES clients(id),
    FOREIGN KEY (target_client) REFERENCES clients(id)
);

-- 辅种历史记录
CREATE TABLE IF NOT EXISTS reseed_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id TEXT,
    info_hash TEXT NOT NULL,
    target_site TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('success', 'failed', 'skipped')),
    message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (task_id) REFERENCES reseed_tasks(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_history_hash ON reseed_history(info_hash);
CREATE INDEX IF NOT EXISTS idx_history_date ON reseed_history(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_history_status ON reseed_history(status);

-- 系统配置
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 全文搜索索引（用于搜索种子名称）
CREATE VIRTUAL TABLE IF NOT EXISTS torrent_fts USING fts5(
    name,
    content='torrent_index',
    content_rowid='id'
);

-- FTS 同步触发器
CREATE TRIGGER IF NOT EXISTS torrent_fts_insert AFTER INSERT ON torrent_index BEGIN
    INSERT INTO torrent_fts(rowid, name) VALUES (new.id, new.name);
END;

CREATE TRIGGER IF NOT EXISTS torrent_fts_delete AFTER DELETE ON torrent_index BEGIN
    INSERT INTO torrent_fts(torrent_fts, rowid, name) VALUES('delete', old.id, old.name);
END;

CREATE TRIGGER IF NOT EXISTS torrent_fts_update AFTER UPDATE ON torrent_index BEGIN
    INSERT INTO torrent_fts(torrent_fts, rowid, name) VALUES('delete', old.id, old.name);
    INSERT INTO torrent_fts(rowid, name) VALUES (new.id, new.name);
END;
```

---

## 6. API 设计

### 6.1 API 路由总览

```
GET    /api/health                    # 健康检查

# 下载器管理
GET    /api/clients                   # 获取所有下载器
POST   /api/clients                   # 添加下载器
GET    /api/clients/:id               # 获取下载器详情
PUT    /api/clients/:id               # 更新下载器
DELETE /api/clients/:id               # 删除下载器
POST   /api/clients/:id/test          # 测试连接
GET    /api/clients/:id/torrents      # 获取种子列表

# 站点管理
GET    /api/sites                     # 获取所有站点
GET    /api/sites/available           # 获取可用的内置站点模板
POST   /api/sites                     # 添加/配置站点
GET    /api/sites/:id                 # 获取站点详情
PUT    /api/sites/:id                 # 更新站点配置
DELETE /api/sites/:id                 # 删除站点

# 索引管理
GET    /api/index/stats               # 索引统计
POST   /api/index/import/:client_id   # 从指定下载器导入索引
DELETE /api/index                     # 清空所有索引
DELETE /api/index/:site_id            # 清空指定站点索引

# 辅种操作
POST   /api/reseed/preview            # 预览辅种结果（不执行）
POST   /api/reseed/execute            # 立即执行辅种
GET    /api/reseed/history            # 获取辅种历史

# 任务管理
GET    /api/tasks                     # 获取所有任务
POST   /api/tasks                     # 创建任务
GET    /api/tasks/:id                 # 获取任务详情
PUT    /api/tasks/:id                 # 更新任务
DELETE /api/tasks/:id                 # 删除任务
POST   /api/tasks/:id/run             # 立即运行任务

# 系统
GET    /api/stats                     # 系统统计（仪表盘数据）
GET    /api/settings                  # 获取设置
PUT    /api/settings                  # 更新设置

# WebSocket
GET    /api/ws/logs                   # 实时日志流
```

### 6.2 路由实现

```rust
// src/api/router.rs

use axum::{
    Router,
    routing::{get, post, put, delete},
};
use tower_http::{
    cors::CorsLayer,
    compression::CompressionLayer,
    trace::TraceLayer,
    services::{ServeDir, ServeFile},
};

pub fn create_router(state: AppState) -> Router {
    let api_routes = Router::new()
        // 健康检查
        .route("/health", get(handlers::health))
        
        // 下载器
        .route("/clients", get(handlers::client::list).post(handlers::client::create))
        .route("/clients/:id", get(handlers::client::get).put(handlers::client::update).delete(handlers::client::delete))
        .route("/clients/:id/test", post(handlers::client::test))
        .route("/clients/:id/torrents", get(handlers::client::torrents))
        
        // 站点
        .route("/sites", get(handlers::site::list).post(handlers::site::create))
        .route("/sites/available", get(handlers::site::available))
        .route("/sites/:id", get(handlers::site::get).put(handlers::site::update).delete(handlers::site::delete))
        
        // 索引
        .route("/index/stats", get(handlers::index::stats))
        .route("/index/import/:client_id", post(handlers::index::import))
        .route("/index", delete(handlers::index::clear_all))
        .route("/index/:site_id", delete(handlers::index::clear_site))
        
        // 辅种
        .route("/reseed/preview", post(handlers::reseed::preview))
        .route("/reseed/execute", post(handlers::reseed::execute))
        .route("/reseed/history", get(handlers::reseed::history))
        
        // 任务
        .route("/tasks", get(handlers::task::list).post(handlers::task::create))
        .route("/tasks/:id", get(handlers::task::get).put(handlers::task::update).delete(handlers::task::delete))
        .route("/tasks/:id/run", post(handlers::task::run))
        
        // 系统
        .route("/stats", get(handlers::stats))
        .route("/settings", get(handlers::settings::get).put(handlers::settings::update))
        
        // WebSocket
        .route("/ws/logs", get(handlers::ws::logs));
    
    Router::new()
        .nest("/api", api_routes)
        // 静态文件服务（前端）
        .fallback_service(
            ServeDir::new("web/dist")
                .fallback(ServeFile::new("web/dist/index.html"))
        )
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
}
```

---

## 7. 前端设计

### 7.1 技术栈

```json
{
  "name": "graft-web",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "preview": "vite preview"
  },
  "dependencies": {
    "solid-js": "^1.8.0",
    "@solidjs/router": "^0.13.0",
    "solid-icons": "^1.1.0"
  },
  "devDependencies": {
    "vite": "^5.0.0",
    "vite-plugin-solid": "^2.9.0",
    "typescript": "^5.3.0",
    "tailwindcss": "^3.4.0",
    "daisyui": "^4.4.0",
    "autoprefixer": "^10.4.0",
    "postcss": "^8.4.0"
  }
}
```

### 7.2 页面结构

```
web/src/
├── index.tsx               # 入口
├── App.tsx                 # 根组件 + 路由
├── api/                    # API 调用封装
│   ├── client.ts           # HTTP 客户端
│   ├── clients.ts          # 下载器 API
│   ├── sites.ts            # 站点 API
│   ├── index.ts            # 索引 API
│   ├── reseed.ts           # 辅种 API
│   └── tasks.ts            # 任务 API
├── stores/                 # 状态管理
│   ├── app.ts              # 全局状态
│   └── toast.ts            # 通知状态
├── components/             # 通用组件
│   ├── Layout.tsx          # 布局框架
│   ├── Sidebar.tsx         # 侧边栏
│   ├── Header.tsx          # 顶栏
│   ├── Modal.tsx           # 弹窗
│   ├── Table.tsx           # 表格
│   ├── Form.tsx            # 表单组件
│   ├── Toast.tsx           # 通知提示
│   └── Stats.tsx           # 统计卡片
├── pages/                  # 页面组件
│   ├── Dashboard.tsx       # 仪表盘
│   ├── Clients.tsx         # 下载器管理
│   ├── Sites.tsx           # 站点管理
│   ├── Index.tsx           # 索引管理
│   ├── Reseed.tsx          # 辅种操作
│   ├── Tasks.tsx           # 任务管理
│   ├── History.tsx         # 历史记录
│   └── Settings.tsx        # 系统设置
└── styles/
    └── main.css            # Tailwind 入口
```

### 7.3 UI 设计

```
┌────────────────────────────────────────────────────────────────┐
│  🌿 Graft                                          [⚙️ 设置]   │
├────────────┬───────────────────────────────────────────────────┤
│            │                                                   │
│  📊 仪表盘  │  ┌─────────────────────────────────────────────┐ │
│            │  │  概览                                        │ │
│  💻 下载器  │  │  ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐       │ │
│            │  │  │ 1234 │ │  89  │ │  12  │ │  3   │       │ │
│  🌐 站点   │  │  │索引总数│ │今日成功│ │ 失败 │ │活跃任务│       │ │
│            │  │  └──────┘ └──────┘ └──────┘ └──────┘       │ │
│  📦 索引   │  └─────────────────────────────────────────────┘ │
│            │                                                   │
│  🔄 辅种   │  ┌─────────────────────────────────────────────┐ │
│            │  │  快速辅种                                    │ │
│  📋 任务   │  │  ┌─────────────┐  ┌─────────────────────┐   │ │
│            │  │  │ 源下载器 ▼  │  │ 目标站点 (多选)      │   │ │
│  📜 历史   │  │  └─────────────┘  └─────────────────────┘   │ │
│            │  │                                              │ │
│            │  │  [预览]  [执行辅种]                          │ │
│            │  └─────────────────────────────────────────────┘ │
│            │                                                   │
│            │  ┌─────────────────────────────────────────────┐ │
│            │  │  最近辅种                                    │ │
│            │  │  ┌────────────────────────────────────────┐ │ │
│            │  │  │ 名称          站点    状态    时间     │ │ │
│            │  │  │ Ubuntu.iso   MT      ✅     10:23    │ │ │
│            │  │  │ Movie.mkv    HDH     ✅     10:22    │ │ │
│            │  │  │ Music.flac   OPS     ❌     10:21    │ │ │
│            │  │  └────────────────────────────────────────┘ │ │
│            │  └─────────────────────────────────────────────┘ │
└────────────┴───────────────────────────────────────────────────┘
```

### 7.4 索引管理页面

```
┌─────────────────────────────────────────────────────────────────┐
│  📦 索引管理                                                    │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  从下载器导入                                            │   │
│  │                                                          │   │
│  │  选择下载器: [  qBittorrent-Home  ▼  ]                  │   │
│  │                                                          │   │
│  │  [开始导入]                                              │   │
│  │                                                          │   │
│  │  ℹ️ 将扫描下载器中所有种子，自动识别站点并建立索引       │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  索引统计                                                │   │
│  │                                                          │   │
│  │  站点              数量        操作                      │   │
│  │  ─────────────────────────────────────                   │   │
│  │  M-Team            523         [清空]                    │   │
│  │  HDSky             412         [清空]                    │   │
│  │  OurBits           287         [清空]                    │   │
│  │  PTer              156         [清空]                    │   │
│  │  ─────────────────────────────────────                   │   │
│  │  总计              1378                                  │   │
│  │                                                          │   │
│  │  [清空所有索引]                                          │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## 8. 部署方案

### 8.1 部署方式

| 方式 | 适用场景 | 说明 |
|------|----------|------|
| **单二进制** | 大多数用户 | 下载即用，无需任何依赖 |
| **Docker** | 服务器/NAS | 容器化部署，易于管理 |

### 8.2 单二进制分发

```bash
# 构建脚本 build.sh

#!/bin/bash
set -e

VERSION=${1:-"dev"}
TARGETS=(
    "x86_64-unknown-linux-gnu"
    "x86_64-unknown-linux-musl"
    "x86_64-pc-windows-msvc"
    "x86_64-apple-darwin"
    "aarch64-unknown-linux-gnu"
    "aarch64-apple-darwin"
)

# 构建前端
echo "Building frontend..."
cd web
npm ci
npm run build
cd ..

# 构建后端（各平台）
for target in "${TARGETS[@]}"; do
    echo "Building for $target..."
    cross build --release --target "$target"
    
    mkdir -p dist
    
    if [[ "$target" == *"windows"* ]]; then
        zip -j "dist/graft-${VERSION}-${target}.zip" \
            "target/${target}/release/graft.exe"
    else
        tar -czvf "dist/graft-${VERSION}-${target}.tar.gz" \
            -C "target/${target}/release" graft
    fi
done

echo "Build complete! Artifacts in dist/"
```

### 8.3 Docker 部署

```dockerfile
# Dockerfile

# Stage 1: Build frontend
FROM node:20-alpine AS web-builder
WORKDIR /app/web
COPY web/package*.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

# Stage 2: Build backend
FROM rust:1.75-alpine AS rust-builder
RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static pkgconfig
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/
# 复制前端构建产物
COPY --from=web-builder /app/web/dist ./web/dist
RUN cargo build --release

# Stage 3: Final image
FROM alpine:3.19
RUN apk add --no-cache ca-certificates tzdata

WORKDIR /app

COPY --from=rust-builder /app/target/release/graft /app/graft
COPY --from=rust-builder /app/web/dist /app/web/dist

RUN mkdir -p /app/data

ENV GRAFT_DATA_DIR=/app/data
ENV GRAFT_WEB_DIR=/app/web/dist
ENV GRAFT_HOST=0.0.0.0
ENV GRAFT_PORT=3000

EXPOSE 3000

VOLUME ["/app/data"]

CMD ["/app/graft"]
```

```yaml
# docker-compose.yml

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
      - GRAFT_LOG_LEVEL=info
```

### 8.4 配置文件

```toml
# config.toml

[server]
host = "0.0.0.0"
port = 3000

[database]
path = "./data/graft.db"

[logging]
level = "info"  # trace, debug, info, warn, error

[reseed]
# 添加种子后是否暂停
default_paused = false
# 请求间隔（毫秒），避免被站点封禁
request_interval_ms = 500
# 单次辅种最大数量
max_per_run = 100
```

---

## 9. 开发路线图

### 9.1 版本规划

```
v0.1.0 - MVP（最小可行产品）
├── 基础框架搭建 (Rust + Axum + SQLite)
├── 下载器管理 (qBittorrent)
├── 站点管理 (NexusPHP 模板)
├── 从下载器导入索引（核心功能）
├── 手动辅种执行
└── 基础 Web UI

v0.2.0 - 核心功能完善
├── Transmission 支持
├── 定时任务调度
├── 辅种历史记录
├── WebSocket 实时日志
└── 更多内置站点

v0.3.0 - 体验优化
├── Unit3D 模板支持
├── Gazelle 模板支持
├── 仪表盘统计图表
├── 深色模式
└── PWA 支持

v0.4.0 - 社区功能
├── 社区站点配置仓库
├── 一键订阅站点配置
├── 站点配置自动更新
└── 站点配置贡献指南

v0.5.0 - 高级功能
├── 通知推送 (Bark/Telegram/Webhook)
├── 更多下载器支持 (Deluge, rTorrent)
├── 数据导入导出
└── API Token 认证

v1.0.0 - 正式发布
├── 全面测试与 Bug 修复
├── 性能优化
├── 完善文档
└── 多语言支持 (i18n)
```

### 9.2 里程碑时间线

| 里程碑 | 目标日期 | 主要内容 |
|--------|----------|----------|
| **M1** | +2周 | 项目初始化，下载器 API，数据库设计 |
| **M2** | +4周 | 索引导入，站点模板，手动辅种，基础 UI |
| **M3** | +6周 | 测试完善，**v0.1.0 MVP 发布** |
| **M4** | +8周 | Transmission，定时任务，历史记录 |
| **M5** | +10周 | WebSocket，更多站点，**v0.2.0 发布** |
| **M6** | +14周 | 体验优化，更多模板，**v0.3.0 发布** |
| **M7** | +18周 | 社区站点仓库，**v0.4.0 发布** |
| **M8** | +24周 | 高级功能，**v0.5.0 发布** |
| **M9** | +30周 | 稳定性优化，**v1.0.0 正式发布** |

### 9.3 MVP (v0.1.0) 详细任务

```
Week 1-2: 基础设施
├── [ ] 项目初始化 (Cargo, 目录结构)
├── [ ] 数据库设计与迁移
├── [ ] 配置管理模块
├── [ ] 日志系统
└── [ ] qBittorrent API 客户端

Week 3-4: 核心功能
├── [ ] Tracker URL 识别器
├── [ ] 从下载器导入索引
├── [ ] NexusPHP 站点模板
├── [ ] 内置 5-10 个常用站点
├── [ ] 辅种服务核心逻辑
└── [ ] REST API 实现

Week 5-6: 前端与测试
├── [ ] SolidJS 项目初始化
├── [ ] 下载器管理页面
├── [ ] 站点管理页面
├── [ ] 索引管理页面
├── [ ] 辅种操作页面
├── [ ] 集成测试
└── [ ] Docker 镜像构建
```

---

## 10. 与 IYUU 的对比

### 10.1 功能对比

| 功能 | IYUU Plus | Graft |
|------|-----------|-------|
| 辅种匹配 | ✅ 云端 API | ✅ 本地数据库 |
| 索引来源 | 云端 + RSS | **从下载器直接读取** |
| qBittorrent | ✅ | ✅ |
| Transmission | ✅ | ✅ (v0.2) |
| Deluge | ❌ | 🚧 (v0.5) |
| 定时任务 | ✅ | ✅ (v0.2) |
| Web UI | ✅ Layui | ✅ SolidJS |
| 用户认证 | ❌ 微信绑定 | ✅ **无需认证** |
| 云服务依赖 | ❌ 必须 | ✅ **完全本地** |
| 站点配置 | 云端维护 | 内置 + **社区仓库** |
| 转移功能 | ✅ | ❌ 不在范围 |
| RSS 订阅 | ✅ | ❌ 不在范围 |
| DDNS | ✅ | ❌ 不在范围 |

### 10.2 架构对比

| 维度 | IYUU Plus | Graft |
|------|-----------|-------|
| 语言 | PHP 8.3 | Rust |
| 框架 | Webman/Workerman | Axum/Tokio |
| 数据库 | MySQL + SQLite | **SQLite only** |
| 前端 | Layui + Vue | SolidJS + Tailwind |
| 部署 | Docker + PHP环境 | **单二进制** / Docker |
| 代码量 | ~50K+ 行（含 vendor） | 预估 ~8K 行 |
| 内存占用 | ~100-200MB | 预估 ~20-40MB |
| 启动时间 | 数秒 | <100ms |

### 10.3 用户体验对比

| 方面 | IYUU | Graft |
|------|------|-------|
| 首次配置 | 微信扫码 → 获取 Token | **直接使用，无需注册** |
| 索引构建 | 自动（云端） | **一键从下载器导入** |
| 站点添加 | 云端已有 | 内置 + 社区订阅 |
| 隐私 | Hash 上传云端 | **数据不出本地** |
| 离线使用 | ❌ 需要网络 | ✅ 完全离线可用 |

---

## 附录

### A. 开发环境搭建

```bash
# 1. 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. 安装 Node.js (推荐使用 fnm)
curl -fsSL https://fnm.vercel.app/install | bash
fnm install 20

# 3. 克隆项目
git clone https://github.com/lynthar/graft.git
cd graft

# 4. 安装前端依赖
cd web && npm install && cd ..

# 5. 运行开发服务器
cargo run

# 6. 前端开发模式（另一个终端）
cd web && npm run dev
```

### B. 参考资料

- [qBittorrent WebUI API](https://github.com/qbittorrent/qBittorrent/wiki/WebUI-API-(qBittorrent-4.1))
- [Transmission RPC](https://github.com/transmission/transmission/blob/main/docs/rpc-spec.md)
- [Axum Documentation](https://docs.rs/axum/latest/axum/)
- [SolidJS Documentation](https://www.solidjs.com/docs/latest)
- [Tailwind CSS](https://tailwindcss.com/docs)
- [DaisyUI Components](https://daisyui.com/components/)

### C. 术语表

| 术语 | 说明 |
|------|------|
| 辅种 | Cross-seeding，在多个站点做种同一资源 |
| info_hash | 种子文件的唯一标识，基于种子内容计算的 SHA1 |
| PT | Private Tracker，私有种子站点 |
| Tracker | 追踪器，协调 P2P 连接的服务器 |
| NexusPHP | 国内最常见的 PT 站点程序 |
| Unit3D | 另一个流行的 PT 站点程序 |
| Gazelle | 音乐站常用的 PT 站点程序 |
| Passkey | PT 站点用于认证下载的私钥 |

---

*文档结束*
