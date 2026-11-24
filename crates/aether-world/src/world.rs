use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize, Serializer};

use crate::txn::Txn;
use crate::types::{
    Entity, EntityId, MAX_COMPONENTS, Owner, Usage, Value, WorldError, entity_bytes, validate_key,
    validate_value,
};

const LANES: usize = 1024;

#[derive(Clone, PartialEq, Eq)]
struct Digest(Box<[u16; LANES]>);

impl Default for Digest {
    fn default() -> Self {
        Digest(Box::new([0; LANES]))
    }
}

impl std::fmt::Debug for Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Digest({:04x}{:04x}..)", self.0[0], self.0[1])
    }
}

impl Digest {
    fn apply(&mut self, id: EntityId, entity: &Entity, add: bool) {
        let mut bytes = [0u8; LANES * 2];
        entity_hasher(id, entity).finalize_xof().fill(&mut bytes);
        for (lane, pair) in self.0.iter_mut().zip(bytes.as_chunks::<2>().0) {
            let v = u16::from_le_bytes(*pair);
            *lane = if add {
                lane.wrapping_add(v)
            } else {
                lane.wrapping_sub(v)
            };
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Derived {
    digest: Digest,
    by_key: BTreeMap<String, BTreeSet<EntityId>>,
    usage: BTreeMap<Owner, Usage>,
}

impl Derived {
    fn add(&mut self, id: EntityId, entity: &Entity) {
        self.digest.apply(id, entity, true);
        for key in entity.components.keys() {
            self.by_key.entry(key.clone()).or_default().insert(id);
        }
        let usage = self.usage.entry(entity.owner.clone()).or_default();
        usage.entities += 1;
        usage.bytes += entity_bytes(entity);
    }

    fn remove(&mut self, id: EntityId, entity: &Entity) {
        self.digest.apply(id, entity, false);
        for key in entity.components.keys() {
            if let Some(ids) = self.by_key.get_mut(key) {
                ids.remove(&id);
                if ids.is_empty() {
                    self.by_key.remove(key);
                }
            }
        }
        if let Some(usage) = self.usage.get_mut(&entity.owner) {
            usage.entities -= 1;
            usage.bytes -= entity_bytes(entity);
            if usage.entities == 0 {
                self.usage.remove(&entity.owner);
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(from = "WorldData<BTreeMap<EntityId, Entity>>")]
pub struct World {
    tick: u64,
    version: u64,
    next_entity: u64,
    entities: BTreeMap<EntityId, Entity>,
    derived: Derived,
}

#[derive(Serialize, Deserialize)]
struct WorldData<E> {
    tick: u64,
    version: u64,
    next_entity: u64,
    entities: E,
}

impl From<WorldData<BTreeMap<EntityId, Entity>>> for World {
    fn from(d: WorldData<BTreeMap<EntityId, Entity>>) -> Self {
        let mut derived = Derived::default();
        for (id, entity) in &d.entities {
            derived.add(*id, entity);
        }
        World {
            tick: d.tick,
            version: d.version,
            next_entity: d.next_entity,
            entities: d.entities,
            derived,
        }
    }
}

impl Serialize for World {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WorldData {
            tick: self.tick,
            version: self.version,
            next_entity: self.next_entity,
            entities: &self.entities,
        }
        .serialize(s)
    }
}

impl World {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn advance_tick(&mut self) -> u64 {
        self.tick += 1;
        self.tick
    }

    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    pub fn entity(&self, id: EntityId) -> Option<&Entity> {
        self.entities.get(&id)
    }

    pub fn entities(&self) -> impl Iterator<Item = (EntityId, &Entity)> {
        self.entities.iter().map(|(id, e)| (*id, e))
    }

    pub fn usage(&self, owner: &Owner) -> Usage {
        self.derived.usage.get(owner).copied().unwrap_or_default()
    }

    pub(crate) fn ids_with(&self, key: &str) -> Option<&BTreeSet<EntityId>> {
        self.derived.by_key.get(key)
    }

    pub(crate) fn version(&self) -> u64 {
        self.version
    }

    pub(crate) fn next_entity(&self) -> u64 {
        self.next_entity
    }

    pub fn begin(&self) -> Txn {
        Txn::new(self)
    }

    pub fn commit(&mut self, txn: Txn) -> Result<BTreeSet<EntityId>, WorldError> {
        if txn.base_version() != self.version {
            return Err(WorldError::StaleTxn);
        }
        let (next_entity, overlay) = txn.into_parts();
        if overlay.is_empty() {
            return Ok(BTreeSet::new());
        }
        let touched = overlay.keys().copied().collect();
        for (id, slot) in overlay {
            if let Some(old) = self.entities.get(&id) {
                self.derived.remove(id, old);
            }
            match slot {
                Some(entity) => {
                    self.derived.add(id, &entity);
                    self.entities.insert(id, entity);
                }
                None => {
                    self.entities.remove(&id);
                }
            }
        }
        self.next_entity = next_entity;
        self.version += 1;
        Ok(touched)
    }

    pub fn validate(&self) -> Result<(), WorldError> {
        for (id, entity) in &self.entities {
            if id.0 >= self.next_entity {
                return Err(WorldError::IdOutOfRange);
            }
            if entity.components.len() > MAX_COMPONENTS {
                return Err(WorldError::TooManyComponents);
            }
            for (key, value) in &entity.components {
                validate_key(key)?;
                validate_value(value)?;
            }
        }
        Ok(())
    }

    pub fn hash(&self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(b"aether-world/v2");
        h.update(&self.tick.to_le_bytes());
        h.update(&self.next_entity.to_le_bytes());
        h.update(&(self.entities.len() as u64).to_le_bytes());
        for lane in self.derived.digest.0.iter() {
            h.update(&lane.to_le_bytes());
        }
        *h.finalize().as_bytes()
    }
}

fn entity_hasher(id: EntityId, entity: &Entity) -> blake3::Hasher {
    let mut h = blake3::Hasher::new();
    h.update(b"aether-world/entity/v1");
    h.update(&id.0.to_le_bytes());
    hash_owner(&mut h, &entity.owner);
    h.update(&(entity.components.len() as u64).to_le_bytes());
    for (key, value) in &entity.components {
        hash_bytes(&mut h, key.as_bytes());
        hash_value(&mut h, value);
    }
    h
}

fn hash_bytes(h: &mut blake3::Hasher, bytes: &[u8]) {
    h.update(&(bytes.len() as u64).to_le_bytes());
    h.update(bytes);
}

fn hash_owner(h: &mut blake3::Hasher, owner: &Owner) {
    match owner {
        Owner::Region => {
            h.update(&[0]);
        }
        Owner::Player(p) => {
            h.update(&[1]);
            hash_bytes(h, p.0.as_bytes());
        }
        Owner::Capsule(c) => {
            h.update(&[2]);
            h.update(&c.0.to_le_bytes());
        }
    }
}

fn hash_value(h: &mut blake3::Hasher, value: &Value) {
    match value {
        Value::Num(n) => {
            h.update(&[0]);
            h.update(&n.to_bits().to_le_bytes());
        }
        Value::Text(t) => {
            h.update(&[1]);
            hash_bytes(h, t.as_bytes());
        }
        Value::Bool(b) => {
            h.update(&[2, u8::from(*b)]);
        }
        Value::Nums(nums) => {
            h.update(&[3]);
            h.update(&(nums.len() as u64).to_le_bytes());
            let mut buf = [0u8; 4096];
            for chunk in nums.chunks(buf.len() / 8) {
                for (slot, n) in buf.as_chunks_mut::<8>().0.iter_mut().zip(chunk) {
                    slot.copy_from_slice(&n.to_bits().to_le_bytes());
                }
                h.update(&buf[..chunk.len() * 8]);
            }
        }
    }
}
