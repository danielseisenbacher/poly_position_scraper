//! Append-only monthly archive of tracked trades: `<dir>/YYYY-MM.jsonl.gz`, one
//! `{"addr":…, …TrackedTrade}` object per line (multi-member gzip so appends are cheap).

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use flate2::read::MultiGzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use types::{Addr, TrackedTrade};

use super::{io_err, StoreError, StoreResult};

#[derive(Serialize)]
struct LineRef<'a> {
    addr: &'a Addr,
    #[serde(flatten)]
    trade: &'a TrackedTrade,
}

#[derive(Deserialize)]
struct Line {
    addr: Addr,
    #[serde(flatten)]
    trade: TrackedTrade,
}

/// Appends one gzip member per touched month and returns the number of lines written.
pub fn append(dir: &Path, trades: &[(Addr, TrackedTrade)]) -> StoreResult<usize> {
    if trades.is_empty() {
        return Ok(0);
    }
    let mut by_month: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (addr, trade) in trades {
        let buf = by_month.entry(month_of(trade.ts)).or_default();
        serde_json::to_writer(&mut *buf, &LineRef { addr, trade }).map_err(|source| {
            StoreError::Json {
                path: dir.display().to_string(),
                source,
            }
        })?;
        buf.push(b'\n');
    }
    std::fs::create_dir_all(dir).map_err(io_err(dir))?;
    for (month, lines) in by_month {
        let path = dir.join(format!("{month}.jsonl.gz"));
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(io_err(&path))?;
        let mut enc = GzEncoder::new(file, Compression::default());
        enc.write_all(&lines)
            .and_then(|_| enc.finish())
            .map_err(io_err(&path))?;
    }
    Ok(trades.len())
}

/// Reads every line of one archive file (all gzip members). A missing file is empty.
pub fn read(path: &Path) -> StoreResult<Vec<(Addr, TrackedTrade)>> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(io_err(path)(e)),
    };
    let mut out = Vec::new();
    for line in BufReader::new(MultiGzDecoder::new(file)).lines() {
        let line = line.map_err(io_err(path))?;
        if line.is_empty() {
            continue;
        }
        let l: Line = serde_json::from_str(&line).map_err(|source| StoreError::Json {
            path: path.display().to_string(),
            source,
        })?;
        out.push((l.addr, l.trade));
    }
    Ok(out)
}

/// `YYYY-MM` (UTC) of a unix timestamp.
fn month_of(ts: i64) -> String {
    let (y, m, _) = civil_from_days(ts.div_euclid(86_400));
    format!("{y:04}-{m:02}")
}

/// Howard Hinnant's civil_from_days: days since 1970-01-01 → (year, month, day).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::domain::parse_ymd;
    use types::TradeKind;

    fn trade(key: &str, ts: i64) -> TrackedTrade {
        TrackedTrade {
            key: key.into(),
            ts,
            kind: TradeKind::Buy,
            condition_id: "0xc0nd".into(),
            token_id: "123".into(),
            title: "Will it happen?".into(),
            outcome: "Yes".into(),
            price: 0.12,
            size: 100.0,
            usdc: 12.0,
            tx: "0xtx".into(),
        }
    }

    #[test]
    fn civil_dates() {
        for s in [
            "1970-01-01",
            "2000-02-29",
            "2026-10-21",
            "2026-12-31",
            "1969-12-31",
            "2100-03-01",
        ] {
            let (y, m, d) = civil_from_days(parse_ymd(s).unwrap() / 86_400);
            assert_eq!(format!("{y:04}-{m:02}-{d:02}"), s);
        }
        assert_eq!(month_of(parse_ymd("2026-11-01").unwrap() - 1), "2026-10");
        assert_eq!(month_of(-1), "1969-12");
    }

    #[test]
    fn multi_append_reads_as_one_stream() {
        let dir = tempfile::tempdir().unwrap();
        let arch = dir.path().join("trades_archive");
        let a = Addr::parse("0x00000000000000000000000000000000000000aa").unwrap();
        let oct = parse_ymd("2026-10-21").unwrap();
        let nov = parse_ymd("2026-11-02").unwrap();

        assert_eq!(append(&arch, &[]).unwrap(), 0);
        let first = vec![(a.clone(), trade("k1", oct)), (a.clone(), trade("k2", nov))];
        assert_eq!(append(&arch, &first).unwrap(), 2);
        assert_eq!(
            append(&arch, &[(a.clone(), trade("k3", oct + 60))]).unwrap(),
            1
        );

        let october = read(&arch.join("2026-10.jsonl.gz")).unwrap();
        assert_eq!(
            october,
            vec![
                (a.clone(), trade("k1", oct)),
                (a.clone(), trade("k3", oct + 60))
            ]
        );
        assert_eq!(
            read(&arch.join("2026-11.jsonl.gz")).unwrap(),
            vec![(a.clone(), trade("k2", nov))]
        );
        assert!(read(&arch.join("2026-09.jsonl.gz")).unwrap().is_empty());

        let mut text = String::new();
        std::io::Read::read_to_string(
            &mut MultiGzDecoder::new(std::fs::File::open(arch.join("2026-11.jsonl.gz")).unwrap()),
            &mut text,
        )
        .unwrap();
        assert!(text.starts_with(
            r#"{"addr":"0x00000000000000000000000000000000000000aa","key":"k2","ts":"#
        ));
    }
}
