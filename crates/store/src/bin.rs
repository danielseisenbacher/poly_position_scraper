//! Binary encodings for release-asset state. Files are gzip-compressed; a missing
//! file loads as empty. Formats are versioned with a 4-byte magic + u32 version.
//!
//! Layout (all integers little-endian), entries sorted by address:
//! - `evaluated.bin`: `PWEV`, u32 version 1, u64 count, then per entry
//!   address (20 raw bytes) + i64 `last_eval_ts` + u8 `last_score`.
//! - `proxy_owner.bin`: `PWPO`, u32 version 1, u64 count, then per entry
//!   proxy (20) + owner (20) + u64 block.

use std::io::{Read, Write};
use std::path::Path;

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use types::state::{Evaluated, EvaluatedSet, ProxyOwnerMap};
use types::Addr;

use super::{atomic_write, read_optional, StoreError, StoreResult};

const VERSION: u32 = 1;
const EVALUATED_MAGIC: &[u8; 4] = b"PWEV";
const EVALUATED_ENTRY: usize = 20 + 8 + 1;
const PROXY_OWNER_MAGIC: &[u8; 4] = b"PWPO";
const PROXY_OWNER_ENTRY: usize = 20 + 20 + 8;
const HEADER: usize = 4 + 4 + 8;

pub fn load_evaluated(path: &Path) -> StoreResult<EvaluatedSet> {
    let mut set = EvaluatedSet::default();
    let Some(body) = load(path, EVALUATED_MAGIC, EVALUATED_ENTRY)? else {
        return Ok(set);
    };
    set.entries.reserve(body.len() / EVALUATED_ENTRY);
    for rec in body.as_chunks::<EVALUATED_ENTRY>().0 {
        let last_eval_ts = i64::from_le_bytes(rec[20..28].try_into().expect("8 bytes"));
        set.entries.insert(
            addr_from_bytes(&rec[..20]),
            Evaluated {
                last_eval_ts,
                last_score: rec[28],
            },
        );
    }
    Ok(set)
}

pub fn save_evaluated(path: &Path, set: &EvaluatedSet) -> StoreResult<()> {
    let mut entries: Vec<_> = set.entries.iter().collect();
    entries.sort_unstable_by(|a, b| a.0.cmp(b.0));
    let mut buf = header(EVALUATED_MAGIC, entries.len(), EVALUATED_ENTRY);
    for (addr, e) in entries {
        buf.extend_from_slice(&addr_to_bytes(addr));
        buf.extend_from_slice(&e.last_eval_ts.to_le_bytes());
        buf.push(e.last_score);
    }
    save(path, &buf)
}

pub fn load_proxy_owner(path: &Path) -> StoreResult<ProxyOwnerMap> {
    let mut map = ProxyOwnerMap::default();
    let Some(body) = load(path, PROXY_OWNER_MAGIC, PROXY_OWNER_ENTRY)? else {
        return Ok(map);
    };
    map.entries.reserve(body.len() / PROXY_OWNER_ENTRY);
    for rec in body.as_chunks::<PROXY_OWNER_ENTRY>().0 {
        let block = u64::from_le_bytes(rec[40..48].try_into().expect("8 bytes"));
        map.entries.insert(
            addr_from_bytes(&rec[..20]),
            (addr_from_bytes(&rec[20..40]), block),
        );
    }
    Ok(map)
}

pub fn save_proxy_owner(path: &Path, map: &ProxyOwnerMap) -> StoreResult<()> {
    let mut entries: Vec<_> = map.entries.iter().collect();
    entries.sort_unstable_by(|a, b| a.0.cmp(b.0));
    let mut buf = header(PROXY_OWNER_MAGIC, entries.len(), PROXY_OWNER_ENTRY);
    for (proxy, (owner, block)) in entries {
        buf.extend_from_slice(&addr_to_bytes(proxy));
        buf.extend_from_slice(&addr_to_bytes(owner));
        buf.extend_from_slice(&block.to_le_bytes());
    }
    save(path, &buf)
}

fn header(magic: &[u8; 4], count: usize, entry: usize) -> Vec<u8> {
    let mut buf = Vec::with_capacity(HEADER + count * entry);
    buf.extend_from_slice(magic);
    buf.extend_from_slice(&VERSION.to_le_bytes());
    buf.extend_from_slice(&(count as u64).to_le_bytes());
    buf
}

fn save(path: &Path, raw: &[u8]) -> StoreResult<()> {
    let mut enc = GzEncoder::new(Vec::with_capacity(raw.len() / 2), Compression::default());
    let gz = enc
        .write_all(raw)
        .and_then(|_| enc.finish())
        .map_err(super::io_err(path))?;
    atomic_write(path, &gz)
}

/// Decompresses and validates the header; returns the entry bytes (a whole number of
/// `entry`-sized records), or `None` if the file does not exist.
fn load(path: &Path, magic: &[u8; 4], entry: usize) -> StoreResult<Option<Vec<u8>>> {
    let Some(gz) = read_optional(path)? else {
        return Ok(None);
    };
    let corrupt = |msg: String| StoreError::Corrupt {
        path: path.display().to_string(),
        msg,
    };
    let mut raw = Vec::with_capacity(gz.len() * 2);
    GzDecoder::new(gz.as_slice())
        .read_to_end(&mut raw)
        .map_err(|e| corrupt(format!("gzip: {e}")))?;
    if raw.len() < HEADER {
        return Err(corrupt(format!("truncated header ({} bytes)", raw.len())));
    }
    if &raw[..4] != magic {
        return Err(corrupt(format!(
            "bad magic {:?}",
            String::from_utf8_lossy(&raw[..4])
        )));
    }
    let version = u32::from_le_bytes(raw[4..8].try_into().expect("4 bytes"));
    if version != VERSION {
        return Err(corrupt(format!("unsupported version {version}")));
    }
    let count = u64::from_le_bytes(raw[8..16].try_into().expect("8 bytes"));
    let body_len = raw.len() - HEADER;
    if count.checked_mul(entry as u64) != Some(body_len as u64) {
        return Err(corrupt(format!(
            "header says {count} entries but body has {body_len} bytes"
        )));
    }
    raw.drain(..HEADER);
    Ok(Some(raw))
}

fn addr_to_bytes(addr: &Addr) -> [u8; 20] {
    let hex = &addr.as_str().as_bytes()[2..];
    let mut out = [0u8; 20];
    for (i, b) in out.iter_mut().enumerate() {
        *b = nibble(hex[2 * i]) << 4 | nibble(hex[2 * i + 1]);
    }
    out
}

fn nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        _ => unreachable!("Addr is lowercase hex"),
    }
}

fn addr_from_bytes(bytes: &[u8]) -> Addr {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(42);
    s.push_str("0x");
    for &b in bytes {
        s.push(HEX[usize::from(b >> 4)] as char);
        s.push(HEX[usize::from(b & 0xf)] as char);
    }
    Addr::parse(&s).expect("40 hex chars")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(n: u64) -> Addr {
        Addr::parse(&format!("0x{n:040x}")).unwrap()
    }

    fn sample_evaluated() -> EvaluatedSet {
        let mut set = EvaluatedSet::default();
        set.entries.insert(
            addr(0xff),
            Evaluated {
                last_eval_ts: 1_790_000_000,
                last_score: 77,
            },
        );
        set.entries.insert(
            addr(0x1),
            Evaluated {
                last_eval_ts: -5,
                last_score: 0,
            },
        );
        set.entries.insert(
            Addr::parse("0xF3238CC8D3EF945C39B36712040886E3847EB0D7").unwrap(),
            Evaluated {
                last_eval_ts: i64::MAX,
                last_score: 255,
            },
        );
        set
    }

    fn raw(path: &Path) -> Vec<u8> {
        let mut out = Vec::new();
        GzDecoder::new(std::fs::File::open(path).unwrap())
            .read_to_end(&mut out)
            .unwrap();
        out
    }

    fn write_gz(path: &Path, raw: &[u8]) {
        let mut enc = GzEncoder::new(Vec::new(), Compression::default());
        enc.write_all(raw).unwrap();
        std::fs::write(path, enc.finish().unwrap()).unwrap();
    }

    #[test]
    fn missing_files_are_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_evaluated(&dir.path().join("nope.bin"))
            .unwrap()
            .entries
            .is_empty());
        assert!(load_proxy_owner(&dir.path().join("nope.bin"))
            .unwrap()
            .entries
            .is_empty());
    }

    #[test]
    fn evaluated_roundtrip_is_deterministic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/evaluated.bin");
        let set = sample_evaluated();
        save_evaluated(&path, &set).unwrap();
        assert_eq!(load_evaluated(&path).unwrap(), set);

        let bytes = raw(&path);
        assert_eq!(&bytes[..4], b"PWEV");
        assert_eq!(bytes.len(), HEADER + 3 * EVALUATED_ENTRY);
        assert_eq!(&bytes[HEADER..HEADER + 20], &addr_to_bytes(&addr(0x1)));
        let first = std::fs::read(&path).unwrap();
        save_evaluated(&path, &set).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), first);
        assert!(!dir.path().join("sub/.evaluated.bin.tmp").exists());
    }

    #[test]
    fn proxy_owner_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("proxy_owner.bin");
        let mut map = ProxyOwnerMap::default();
        map.entries.insert(addr(7), (addr(8), 78_000_000));
        map.entries.insert(addr(3), (addr(8), u64::MAX));
        save_proxy_owner(&path, &map).unwrap();
        assert_eq!(load_proxy_owner(&path).unwrap(), map);
        assert_eq!(&raw(&path)[..4], b"PWPO");
    }

    #[test]
    fn detects_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evaluated.bin");
        save_evaluated(&path, &sample_evaluated()).unwrap();
        let good = raw(&path);
        let is_corrupt = |p: &Path| matches!(load_evaluated(p), Err(StoreError::Corrupt { .. }));

        write_gz(&path, &good[..good.len() - 1]);
        assert!(is_corrupt(&path), "truncated body");
        write_gz(&path, &good[..10]);
        assert!(is_corrupt(&path), "truncated header");
        let mut bad = good.clone();
        bad[..4].copy_from_slice(b"PWPO");
        write_gz(&path, &bad);
        assert!(is_corrupt(&path), "wrong magic");
        let mut bad = good.clone();
        bad[4] = 2;
        write_gz(&path, &bad);
        assert!(is_corrupt(&path), "wrong version");
        let mut bad = good.clone();
        bad[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
        write_gz(&path, &bad);
        assert!(is_corrupt(&path), "absurd count");
        std::fs::write(&path, b"not gzip at all").unwrap();
        assert!(is_corrupt(&path), "not gzip");
        save_evaluated(&path, &sample_evaluated()).unwrap();
        let gz = std::fs::read(&path).unwrap();
        std::fs::write(&path, &gz[..gz.len() / 2]).unwrap();
        assert!(is_corrupt(&path), "truncated gzip");

        save_evaluated(&path, &sample_evaluated()).unwrap();
        assert!(
            matches!(load_proxy_owner(&path), Err(StoreError::Corrupt { .. })),
            "other format"
        );
    }
}
