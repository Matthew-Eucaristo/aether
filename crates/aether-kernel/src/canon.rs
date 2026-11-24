use std::collections::BTreeMap;

use aether_world::{CapsuleId, Usage};
use blake3::Hasher;

use crate::blob::BlobHash;
use crate::capsule::{Budget, Cap, Capsule, Status};
use crate::input::Actor;
use crate::law::Law;
use crate::signal::{Sender, Signal};

pub(crate) trait Canon {
    fn feed(&self, h: &mut Hasher);
}

impl Canon for u64 {
    fn feed(&self, h: &mut Hasher) {
        h.update(&self.to_le_bytes());
    }
}

impl Canon for u32 {
    fn feed(&self, h: &mut Hasher) {
        h.update(&self.to_le_bytes());
    }
}

impl Canon for bool {
    fn feed(&self, h: &mut Hasher) {
        h.update(&[u8::from(*self)]);
    }
}

impl Canon for BlobHash {
    fn feed(&self, h: &mut Hasher) {
        h.update(&self.0);
    }
}

impl Canon for Usage {
    fn feed(&self, h: &mut Hasher) {
        let Usage { entities, bytes } = self;
        entities.feed(h);
        bytes.feed(h);
    }
}

impl Canon for Budget {
    fn feed(&self, h: &mut Hasher) {
        let Budget {
            fuel_per_tick,
            memory_bytes,
            writes_per_tick,
            spawns_per_tick,
        } = self;
        fuel_per_tick.feed(h);
        memory_bytes.feed(h);
        writes_per_tick.feed(h);
        spawns_per_tick.feed(h);
    }
}

impl Canon for Law {
    fn feed(&self, h: &mut Hasher) {
        let Law {
            max_entities,
            max_capsules,
            max_module_bytes,
            max_clist_len,
            max_consecutive_faults,
            allow_spawn,
            max_inputs_per_actor,
            region_fuel_per_tick,
            player_quota,
            ceiling,
            terrain,
        } = self;
        max_entities.feed(h);
        max_capsules.feed(h);
        max_module_bytes.feed(h);
        max_clist_len.feed(h);
        max_consecutive_faults.feed(h);
        allow_spawn.feed(h);
        max_inputs_per_actor.feed(h);
        region_fuel_per_tick.feed(h);
        player_quota.feed(h);
        ceiling.feed(h);
        terrain.feed(h);
    }
}

impl Canon for crate::terrain::Terrain {
    fn feed(&self, h: &mut Hasher) {
        let crate::terrain::Terrain {
            seed,
            relief,
            scale,
            plaza,
        } = self;
        seed.feed(h);
        relief.feed(h);
        scale.feed(h);
        plaza.feed(h);
    }
}

impl Canon for Actor {
    fn feed(&self, h: &mut Hasher) {
        match self {
            Actor::Admin => {
                h.update(&[0]);
            }
            Actor::Player(p) => {
                h.update(&[1]);
                (p.0.len() as u64).feed(h);
                h.update(p.0.as_bytes());
            }
        }
    }
}

impl Canon for Option<Cap> {
    fn feed(&self, h: &mut Hasher) {
        match self {
            None => {
                h.update(&[0]);
            }
            Some(Cap { entity, write }) => {
                h.update(&[1]);
                entity.0.feed(h);
                write.feed(h);
            }
        }
    }
}

impl Canon for Status {
    fn feed(&self, h: &mut Hasher) {
        h.update(&[match self {
            Status::Active => 0,
            Status::Suspended => 1,
        }]);
    }
}

impl Canon for Capsule {
    fn feed(&self, h: &mut Hasher) {
        let Capsule {
            installer,
            wasm,
            self_entity,
            clist,
            budget,
            status,
            consecutive_faults,
        } = self;
        installer.feed(h);
        wasm.feed(h);
        self_entity.0.feed(h);
        (clist.len() as u64).feed(h);
        for cap in clist {
            cap.feed(h);
        }
        budget.feed(h);
        status.feed(h);
        consecutive_faults.feed(h);
    }
}

impl Canon for BTreeMap<CapsuleId, Capsule> {
    fn feed(&self, h: &mut Hasher) {
        (self.len() as u64).feed(h);
        for (id, capsule) in self {
            id.0.feed(h);
            capsule.feed(h);
        }
    }
}

impl Canon for Vec<Signal> {
    fn feed(&self, h: &mut Hasher) {
        (self.len() as u64).feed(h);
        for Signal {
            from,
            entity,
            name,
            value,
        } in self
        {
            match from {
                Sender::Actor(a) => {
                    h.update(&[0]);
                    a.feed(h);
                }
                Sender::Capsule(c) => {
                    h.update(&[1]);
                    c.0.feed(h);
                }
            }
            entity.0.feed(h);
            (name.len() as u64).feed(h);
            h.update(name.as_bytes());
            value.to_bits().feed(h);
        }
    }
}
