use aether_world::EntityId;
use serde::{Deserialize, Serialize};

use crate::blob::BlobHash;
use crate::input::Actor;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    pub fuel_per_tick: u64,
    pub memory_bytes: u64,
    pub writes_per_tick: u32,
    pub spawns_per_tick: u32,
}

impl Budget {
    pub fn min(&self, other: &Budget) -> Budget {
        Budget {
            fuel_per_tick: self.fuel_per_tick.min(other.fuel_per_tick),
            memory_bytes: self.memory_bytes.min(other.memory_bytes),
            writes_per_tick: self.writes_per_tick.min(other.writes_per_tick),
            spawns_per_tick: self.spawns_per_tick.min(other.spawns_per_tick),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    pub entity: EntityId,
    pub write: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cap {
    pub entity: EntityId,
    pub write: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Active,
    Suspended,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Capsule {
    pub installer: Actor,
    pub wasm: BlobHash,
    pub self_entity: EntityId,
    pub clist: Vec<Option<Cap>>,
    pub budget: Budget,
    pub status: Status,
    pub consecutive_faults: u32,
}
