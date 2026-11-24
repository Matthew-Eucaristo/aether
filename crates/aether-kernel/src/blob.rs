use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlobHash(pub [u8; 32]);

impl BlobHash {
    pub fn of(bytes: &[u8]) -> Self {
        Self(*blake3::hash(bytes).as_bytes())
    }

    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        if s.len() != 64 {
            return None;
        }
        let mut out = [0u8; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok()?;
        }
        Some(Self(out))
    }
}

impl fmt::Debug for BlobHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BlobHash({})", &self.to_hex()[..12])
    }
}

impl fmt::Display for BlobHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl Serialize for BlobHash {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for BlobHash {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::from_hex(&s).ok_or_else(|| serde::de::Error::custom("expected 64 hex characters"))
    }
}

#[derive(Clone, Debug, Default)]
pub struct Blobs {
    map: BTreeMap<BlobHash, Arc<[u8]>>,
}

impl Blobs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, bytes: impl Into<Arc<[u8]>>) -> BlobHash {
        let bytes = bytes.into();
        let hash = BlobHash::of(&bytes);
        self.map.entry(hash).or_insert(bytes);
        hash
    }

    pub fn get(&self, hash: &BlobHash) -> Option<&Arc<[u8]>> {
        self.map.get(hash)
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&BlobHash) -> bool) -> Vec<BlobHash> {
        let mut dropped = Vec::new();
        self.map.retain(|hash, _| {
            let kept = keep(hash);
            if !kept {
                dropped.push(*hash);
            }
            kept
        });
        dropped
    }

    pub fn iter(&self) -> impl Iterator<Item = (&BlobHash, &Arc<[u8]>)> {
        self.map.iter()
    }
}
