use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub const MAX_KEY_LEN: usize = 64;
pub const MAX_TEXT_LEN: usize = 4096;
pub const MAX_COMPONENTS: usize = 128;
pub const MAX_NUMS: usize = 1 << 20;
pub const ENTITY_OVERHEAD_BYTES: u64 = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub entities: u64,
    pub bytes: u64,
}

pub fn value_bytes(value: &Value) -> u64 {
    match value {
        Value::Num(_) => 8,
        Value::Text(t) => t.len() as u64,
        Value::Bool(_) => 1,
        Value::Nums(n) => 8 * n.len() as u64,
    }
}

pub fn entity_bytes(entity: &Entity) -> u64 {
    ENTITY_OVERHEAD_BYTES
        + entity
            .components
            .iter()
            .map(|(k, v)| k.len() as u64 + value_bytes(v))
            .sum::<u64>()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EntityId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CapsuleId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Owner {
    Region,
    Player(PlayerId),
    Capsule(CapsuleId),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Value {
    Num(f64),
    Text(String),
    Bool(bool),
    Nums(Arc<[f64]>),
}

/// Values are equal only when they are bit-identical, as the state hash sees them: 0.0 and -0.0 differ.
impl PartialEq for Value {
    fn eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Num(a), Value::Num(b)) => a.to_bits() == b.to_bits(),
            (Value::Text(a), Value::Text(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Nums(a), Value::Nums(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|(x, y)| x.to_bits() == y.to_bits())
            }
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub owner: Owner,
    pub components: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WorldError {
    #[error("entity {} does not exist", .0.0)]
    NoSuchEntity(EntityId),
    #[error("component key must be 1-{MAX_KEY_LEN} bytes of [A-Za-z0-9_.-]")]
    InvalidKey,
    #[error("numbers must be finite")]
    NonFinite,
    #[error("text is longer than {MAX_TEXT_LEN} bytes")]
    TextTooLong,
    #[error("a number array holds at most {MAX_NUMS} numbers")]
    TooManyNums,
    #[error("an entity holds at most {MAX_COMPONENTS} components")]
    TooManyComponents,
    #[error("entity ids must be below next_entity")]
    IdOutOfRange,
    #[error("transaction was opened against a different world version")]
    StaleTxn,
}

pub fn validate_key(key: &str) -> Result<(), WorldError> {
    let ok = !key.is_empty()
        && key.len() <= MAX_KEY_LEN
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'));
    if ok {
        Ok(())
    } else {
        Err(WorldError::InvalidKey)
    }
}

pub fn validate_value(value: &Value) -> Result<(), WorldError> {
    match value {
        Value::Num(n) if !n.is_finite() => Err(WorldError::NonFinite),
        Value::Text(t) if t.len() > MAX_TEXT_LEN => Err(WorldError::TextTooLong),
        Value::Nums(n) if n.len() > MAX_NUMS => Err(WorldError::TooManyNums),
        Value::Nums(n) if n.iter().any(|x| !x.is_finite()) => Err(WorldError::NonFinite),
        _ => Ok(()),
    }
}
