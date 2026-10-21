use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};

/// A Polygon address, always stored lowercase as `0x` + 40 hex chars.
///
/// Deserializing accepts any case (the RTDS feed and Gamma return checksummed
/// addresses) and normalises to lowercase, so two `Addr`s compare equal exactly
/// when they refer to the same account.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Addr(String);

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("invalid address {0:?}: expected 0x followed by 40 hex characters")]
pub struct AddrError(pub String);

impl Addr {
    pub fn parse(s: &str) -> Result<Self, AddrError> {
        let t = s.trim();
        let ok = t.len() == 42
            && (t.starts_with("0x") || t.starts_with("0X"))
            && t[2..].bytes().all(|b| b.is_ascii_hexdigit());
        if ok {
            Ok(Addr(format!("0x{}", t[2..].to_ascii_lowercase())))
        } else {
            Err(AddrError(s.to_string()))
        }
    }

    /// Decode the address stored in the low 20 bytes of a 32-byte log topic
    /// (`0x` + 64 hex chars), as emitted for indexed `address` event params.
    pub fn from_topic(topic: &str) -> Result<Self, AddrError> {
        let t = topic.trim();
        if t.len() != 66 || !t.starts_with("0x") {
            return Err(AddrError(topic.to_string()));
        }
        Addr::parse(&format!("0x{}", &t[26..]))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// `0xabcd…1234`, for issue titles and tables.
    pub fn short(&self) -> String {
        format!("{}…{}", &self.0[..6], &self.0[38..])
    }
}

impl FromStr for Addr {
    type Err = AddrError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Addr::parse(s)
    }
}

impl<'de> Deserialize<'de> for Addr {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Addr::parse(&s).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Addr({})", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_case() {
        let a = Addr::parse("0xF3238CC8D3EF945C39B36712040886E3847EB0D7").unwrap();
        assert_eq!(a.as_str(), "0xf3238cc8d3ef945c39b36712040886e3847eb0d7");
        assert_eq!(a.short(), "0xf323…b0d7");
    }

    #[test]
    fn rejects_garbage() {
        assert!(Addr::parse("0x123").is_err());
        assert!(Addr::parse("f3238cc8d3ef945c39b36712040886e3847eb0d7aa").is_err());
        assert!(Addr::parse("0xg3238cc8d3ef945c39b36712040886e3847eb0d7").is_err());
    }

    #[test]
    fn decodes_topic() {
        let a = Addr::from_topic(
            "0x000000000000000000000000c3964d24c189ef737578715b47aa67b0546c3cfe",
        )
        .unwrap();
        assert_eq!(a.as_str(), "0xc3964d24c189ef737578715b47aa67b0546c3cfe");
    }

    #[test]
    fn serde_roundtrip_lowercases() {
        let a: Addr = serde_json::from_str("\"0xC3964D24c189ef737578715b47aa67b0546c3cfe\"").unwrap();
        assert_eq!(serde_json::to_string(&a).unwrap(), "\"0xc3964d24c189ef737578715b47aa67b0546c3cfe\"");
    }
}
