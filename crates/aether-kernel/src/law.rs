use aether_world::Usage;
use serde::{Deserialize, Serialize};

use crate::capsule::Budget;
use crate::terrain::Terrain;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Law {
    pub max_entities: u64,
    pub max_capsules: u32,
    pub max_module_bytes: u64,
    pub max_clist_len: u32,
    pub max_consecutive_faults: u32,
    pub allow_spawn: bool,
    pub max_inputs_per_actor: u32,
    pub region_fuel_per_tick: u64,
    pub player_quota: Usage,
    pub ceiling: Budget,
    pub terrain: Terrain,
}

impl Default for Law {
    fn default() -> Self {
        Self {
            max_entities: 10_000,
            max_capsules: 256,
            max_module_bytes: 1 << 20,
            max_clist_len: 4096,
            max_consecutive_faults: 3,
            allow_spawn: true,
            max_inputs_per_actor: 32,
            region_fuel_per_tick: 50_000_000,
            player_quota: Usage {
                entities: 500,
                bytes: 512 << 10,
            },
            ceiling: Budget {
                fuel_per_tick: 10_000_000,
                memory_bytes: 16 << 20,
                writes_per_tick: 1024,
                spawns_per_tick: 16,
            },
            terrain: Terrain::default(),
        }
    }
}

impl Law {
    pub fn attenuate(&self, requested: &Budget) -> Budget {
        let mut budget = requested.min(&self.ceiling);
        if !self.allow_spawn {
            budget.spawns_per_tick = 0;
        }
        budget
    }
}
