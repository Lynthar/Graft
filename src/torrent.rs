//! Parsing of `.torrent` files from sites or uploaded by the user (untrusted input).

/// Largest `.torrent` accepted from a site.
pub const MAX_TORRENT_BYTES: usize = 10 * 1024 * 1024;

/// Bencode nesting allowed before a file is rejected; real metainfo needs about four.
const MAX_DEPTH: usize = 32;

/// What the reseed path needs from a `.torrent` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metainfo {
    /// SHA-1 of the raw `info` dictionary bytes, lowercase hex.
    pub info_hash: String,
    /// SHA-1 of `info.pieces`, lowercase hex; the value NexusPHP indexes.
    pub pieces_hash: String,
    /// `info.name`: the single file, or the folder the files land in.
    pub name: String,
    /// Files as they land under the save path: `name/...` for multi-file torrents.
    pub files: Vec<(String, u64)>,
    /// Host of the announce URL; the URL itself carries the passkey and is not kept.
    pub tracker_host: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum MetainfoError {
    #[error("文件超过 {MAX_TORRENT_BYTES} 字节")]
    TooLarge,
    #[error("不是有效的种子文件：{0}")]
    Malformed(&'static str),
}

use MetainfoError::Malformed;

enum Value<'a> {
    Int(i64),
    Bytes(&'a [u8]),
    List(Vec<Value<'a>>),
    Dict(Vec<(&'a [u8], Value<'a>)>),
}

impl<'a> Value<'a> {
    fn get(&self, key: &str) -> Option<&Value<'a>> {
        match self {
            Value::Dict(entries) => entries.iter().find(|(k, _)| *k == key.as_bytes()).map(|(_, v)| v),
            _ => None,
        }
    }
}

/// Parse a `.torrent` file.
///
/// # Errors
/// Anything but exactly one bencoded dictionary with a v1 `info` dictionary is
/// rejected, as is nesting deeper than real metainfo or a name that is not UTF-8.
/// Names follow libtorrent: `name.utf-8` / `path.utf-8` win over `name` / `path`.
pub fn parse(bytes: &[u8]) -> Result<Metainfo, MetainfoError> {
    if bytes.len() > MAX_TORRENT_BYTES {
        return Err(MetainfoError::TooLarge);
    }
    if bytes.first() != Some(&b'd') {
        return Err(Malformed("不是 bencode 字典"));
    }
    let mut info = None;
    let mut announce = None;
    let mut pos = 1;
    while *bytes.get(pos).ok_or(Malformed("数据被截断"))? != b'e' {
        let (key, key_end) = decode(bytes, pos, 1).ok_or(Malformed("键无法解析"))?;
        let (value, end) = decode(bytes, key_end, 1).ok_or(Malformed("值无法解析"))?;
        match key {
            Value::Bytes(b"info") => info = Some((value, key_end..end)),
            Value::Bytes(b"announce") => announce = Some(value),
            _ => {}
        }
        pos = end;
    }
    if pos + 1 != bytes.len() {
        return Err(Malformed("字典后面还有多余的数据"));
    }
    let (info, span) = info.ok_or(Malformed("缺少 info 字典"))?;

    let pieces = match info.get("pieces") {
        Some(Value::Bytes(p)) if !p.is_empty() && p.len() % 20 == 0 => *p,
        _ => return Err(Malformed("没有 v1 的 piece 哈希")),
    };
    let name = text(info.get("name.utf-8").or_else(|| info.get("name")))?;
    let files = match info.get("files") {
        None => vec![(name.clone(), length(info.get("length"))?)],
        Some(Value::List(entries)) => entries
            .iter()
            .map(|f| {
                let parts = match f.get("path.utf-8").or_else(|| f.get("path")) {
                    Some(Value::List(parts)) if !parts.is_empty() => parts,
                    _ => return Err(Malformed("有文件缺少路径")),
                };
                let mut path = name.clone();
                for part in parts {
                    path.push('/');
                    path.push_str(&text(Some(part))?);
                }
                Ok((path, length(f.get("length"))?))
            })
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(Malformed("files 不是列表")),
    };

    let tracker_host = match announce {
        Some(Value::Bytes(url)) => std::str::from_utf8(url)
            .ok()
            .and_then(|u| url::Url::parse(u).ok())
            .and_then(|u| u.host_str().map(str::to_ascii_lowercase)),
        _ => None,
    };
    Ok(Metainfo { info_hash: sha1_hex(&bytes[span]), pieces_hash: sha1_hex(pieces), name, files, tracker_host })
}

pub fn sha1_hex(bytes: &[u8]) -> String {
    sha1_smol::Sha1::from(bytes).digest().to_string()
}

fn text(value: Option<&Value>) -> Result<String, MetainfoError> {
    match value {
        Some(Value::Bytes(b)) if !b.is_empty() => {
            String::from_utf8(b.to_vec()).map_err(|_| Malformed("有名字不是 UTF-8"))
        }
        _ => Err(Malformed("名字缺失或为空")),
    }
}

fn length(value: Option<&Value>) -> Result<u64, MetainfoError> {
    match value {
        Some(Value::Int(n)) => u64::try_from(*n).map_err(|_| Malformed("长度为负")),
        _ => Err(Malformed("缺少长度")),
    }
}

/// Decode the value starting at `pos`; returns it with the offset just past it.
fn decode(b: &[u8], pos: usize, depth: usize) -> Option<(Value<'_>, usize)> {
    if depth > MAX_DEPTH {
        return None;
    }
    match *b.get(pos)? {
        b'i' => {
            let end = pos + b[pos..].iter().position(|&c| c == b'e')?;
            let n = std::str::from_utf8(&b[pos + 1..end]).ok()?.parse().ok()?;
            Some((Value::Int(n), end + 1))
        }
        b'l' => {
            let (mut items, mut p) = (Vec::new(), pos + 1);
            while *b.get(p)? != b'e' {
                let (v, next) = decode(b, p, depth + 1)?;
                items.push(v);
                p = next;
            }
            Some((Value::List(items), p + 1))
        }
        b'd' => {
            let (mut entries, mut p) = (Vec::new(), pos + 1);
            while *b.get(p)? != b'e' {
                let (Value::Bytes(k), next) = decode(b, p, depth + 1)? else { return None };
                let (v, next) = decode(b, next, depth + 1)?;
                entries.push((k, v));
                p = next;
            }
            Some((Value::Dict(entries), p + 1))
        }
        b'0'..=b'9' => {
            let colon = pos + b[pos..].iter().position(|&c| c == b':')?;
            let len: usize = std::str::from_utf8(&b[pos..colon]).ok()?.parse().ok()?;
            let end = (colon + 1).checked_add(len)?;
            Some((Value::Bytes(b.get(colon + 1..end)?), end))
        }
        _ => None,
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A bencoded torrent whose `info` dict holds `extra` verbatim, so tests can vary the
    /// info hash (e.g. a site's `source` tag) while keeping pieces and layout identical.
    pub fn torrent_bytes(name: &str, files: &[(&str, u64)], pieces: &[u8], extra: &str) -> Vec<u8> {
        let mut info = String::new();
        if files.len() == 1 && files[0].0.is_empty() {
            info.push_str(&format!("6:lengthi{}e", files[0].1));
        } else {
            info.push_str("5:filesl");
            for (path, len) in files {
                info.push_str(&format!("d6:lengthi{len}e4:pathl"));
                for part in path.split('/') {
                    info.push_str(&format!("{}:{}", part.len(), part));
                }
                info.push_str("ee");
            }
            info.push('e');
        }
        let mut out = format!("d8:announce3:x:y4:infod{info}4:name{}:{name}12:piece lengthi16384e6:pieces{}:", name.len(), pieces.len()).into_bytes();
        out.extend_from_slice(pieces);
        out.extend_from_slice(extra.as_bytes());
        out.extend_from_slice(b"ee");
        out
    }

    #[test]
    fn multi_file_layout_is_rooted_at_the_torrent_name() {
        let pieces = [7u8; 40];
        let bytes = torrent_bytes("Show", &[("a.mkv", 10), ("Subs/a.srt", 2)], &pieces, "");
        let meta = parse(&bytes).unwrap();
        assert_eq!(meta.files, vec![("Show/a.mkv".into(), 10), ("Show/Subs/a.srt".into(), 2)]);
        assert_eq!(meta.pieces_hash, sha1_hex(&pieces));
    }

    #[test]
    fn single_file_layout_is_the_name_itself() {
        let bytes = torrent_bytes("movie.mkv", &[("", 99)], &[1u8; 20], "");
        assert_eq!(parse(&bytes).unwrap().files, vec![("movie.mkv".into(), 99)]);
    }

    #[test]
    fn only_the_host_of_the_announce_url_is_kept() {
        let announce = "https://Tracker.Example.org/announce.php?passkey=SECRET";
        let plain = String::from_utf8(torrent_bytes("x", &[("", 1)], &[1u8; 20], "")).unwrap();
        let bytes = plain.replacen("d8:announce3:x:y", &format!("d8:announce{}:{announce}", announce.len()), 1);
        let meta = parse(bytes.as_bytes()).unwrap();
        assert_eq!(meta.tracker_host.as_deref(), Some("tracker.example.org"));
        assert!(!format!("{meta:?}").contains("SECRET"));
    }

    #[test]
    fn info_hash_covers_the_raw_info_bytes() {
        let a = parse(&torrent_bytes("x", &[("", 1)], &[1u8; 20], "")).unwrap();
        let b = parse(&torrent_bytes("x", &[("", 1)], &[1u8; 20], "6:source1:B")).unwrap();
        assert_ne!(a.info_hash, b.info_hash);
        assert_eq!(a.pieces_hash, b.pieces_hash);

        let bytes = torrent_bytes("x", &[("", 1)], &[1u8; 20], "");
        let start = bytes.windows(6).position(|w| w == b"4:info").unwrap() + 6;
        assert_eq!(a.info_hash, sha1_hex(&bytes[start..bytes.len() - 1]));
    }

    #[test]
    fn utf8_name_fields_win_and_raw_non_utf8_names_are_refused() {
        let mut gbk = torrent_bytes("x", &[("", 5)], &[3u8; 20], "");
        let at = gbk.windows(6).position(|w| w == b"1:x12:").unwrap();
        gbk.splice(at..at + 3, b"2:\xc4\xe3".iter().copied());
        assert!(parse(&gbk).is_err());

        let named = torrent_bytes("x", &[("", 5)], &[3u8; 20], "10:name.utf-87:你.mkv");
        assert_eq!(parse(&named).unwrap().files, vec![("你.mkv".into(), 5)]);
    }

    #[test]
    fn html_trailing_bytes_and_deep_nesting_are_rejected() {
        assert!(parse(b"<html>login</html>").is_err());
        let mut ok = torrent_bytes("x", &[("", 1)], &[1u8; 20], "");
        ok.push(b'x');
        assert!(parse(&ok).is_err());
        let deep = format!("d4:info{}e", "l".repeat(100_000));
        assert!(parse(deep.as_bytes()).is_err());
    }
}
