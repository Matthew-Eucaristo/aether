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

    fn merge(&mut self, other: &Digest) {
        for (lane, v) in self.0.iter_mut().zip(other.0.iter()) {
            *lane = lane.wrapping_add(*v);
        }
    }

    fn of_changes(changes: &[(EntityId, Option<&Entity>, Option<&Entity>)]) -> Digest {
        let mut d = Digest::default();
        for (id, old, new) in changes {
            if old == new {
                continue;
            }
            if let Some(old) = old {
                d.apply(*id, old, false);
            }
            if let Some(new) = new {
                d.apply(*id, new, true);
            }
        }
        d
    }
}

const PARALLEL_FROM: usize = 64;

pub const CELL: f64 = 16.0;
pub const MAX_RADIUS: f64 = 512.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    Cell(Cell),
    Wide(i64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Cell {
    dim: i64,
    x: i64,
    y: i64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub dim: i64,
    pub x: f64,
    pub y: f64,
    pub reach: f64,
}

impl Place {
    pub fn of(entity: &Entity) -> Option<Place> {
        let num = |key: &str| match entity.components.get(key) {
            Some(Value::Num(n)) => Some(*n),
            _ => None,
        };
        let (sx, sy) = (
            num("scale.x").unwrap_or(0.0).abs(),
            num("scale.y").unwrap_or(0.0).abs(),
        );
        Some(Place {
            dim: num("dim").map_or(0, |d| d as i64),
            x: num("pos.x")?,
            y: num("pos.y").unwrap_or(0.0),
            reach: (sx * sx + sy * sy).sqrt(),
        })
    }

    fn slot(self) -> Slot {
        if self.reach > CELL {
            Slot::Wide(self.dim)
        } else {
            Slot::Cell(self.cell())
        }
    }

    fn cell(self) -> Cell {
        Cell {
            dim: self.dim,
            x: (self.x / CELL).floor() as i64,
            y: (self.y / CELL).floor() as i64,
        }
    }

    pub fn within(self, dim: i64, x: f64, y: f64, r: f64) -> bool {
        let (dx, dy) = (self.x - x, self.y - y);
        let reach = r + self.reach;
        self.dim == dim && dx * dx + dy * dy <= reach * reach
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Derived {
    digest: Digest,
    stale: BTreeMap<EntityId, Option<Entity>>,
    by_key: BTreeMap<String, BTreeSet<EntityId>>,
    usage: BTreeMap<Owner, Usage>,
    cells: BTreeMap<Cell, BTreeSet<EntityId>>,
    wide: BTreeMap<i64, BTreeSet<EntityId>>,
}

impl Derived {
    fn index(&mut self, key: &str, id: EntityId, present: bool) {
        if present {
            match self.by_key.get_mut(key) {
                Some(ids) => {
                    ids.insert(id);
                }
                None => {
                    self.by_key.insert(key.to_owned(), BTreeSet::from([id]));
                }
            }
        } else if let Some(ids) = self.by_key.get_mut(key) {
            ids.remove(&id);
            if ids.is_empty() {
                self.by_key.remove(key);
            }
        }
    }

    fn count(&mut self, owner: &Owner, entities: i64, bytes: i64) {
        if !self.usage.contains_key(owner) {
            self.usage.insert(owner.clone(), Usage::default());
        }
        let usage = self.usage.get_mut(owner).expect("inserted above");
        usage.entities = usage.entities.wrapping_add_signed(entities);
        usage.bytes = usage.bytes.wrapping_add_signed(bytes);
        if usage.entities == 0 {
            self.usage.remove(owner);
        }
    }

    fn locate(&mut self, id: EntityId, old: Option<&Entity>, new: Option<&Entity>) {
        let before = old.and_then(Place::of).map(Place::slot);
        let after = new.and_then(Place::of).map(Place::slot);
        if before == after {
            return;
        }
        match before {
            Some(Slot::Cell(cell)) => {
                if let Some(ids) = self.cells.get_mut(&cell) {
                    ids.remove(&id);
                    if ids.is_empty() {
                        self.cells.remove(&cell);
                    }
                }
            }
            Some(Slot::Wide(dim)) => {
                if let Some(ids) = self.wide.get_mut(&dim) {
                    ids.remove(&id);
                    if ids.is_empty() {
                        self.wide.remove(&dim);
                    }
                }
            }
            None => {}
        }
        match after {
            Some(Slot::Cell(cell)) => {
                self.cells.entry(cell).or_default().insert(id);
            }
            Some(Slot::Wide(dim)) => {
                self.wide.entry(dim).or_default().insert(id);
            }
            None => {}
        }
    }

    fn change(&mut self, id: EntityId, old: Option<&Entity>, new: Option<&Entity>) {
        self.locate(id, old, new);
        let empty = BTreeMap::new();
        let before = old.map_or(&empty, |e| &e.components);
        let after = new.map_or(&empty, |e| &e.components);
        let (mut a, mut b) = (before.keys().peekable(), after.keys().peekable());
        loop {
            match (a.peek(), b.peek()) {
                (None, None) => break,
                (Some(x), Some(y)) if x == y => {
                    a.next();
                    b.next();
                }
                (Some(x), Some(y)) if x < y => {
                    let x = (*x).clone();
                    a.next();
                    self.index(&x, id, false);
                }
                (Some(_), None) => {
                    let x = a.next().expect("peeked").clone();
                    self.index(&x, id, false);
                }
                _ => {
                    let y = b.next().expect("peeked").clone();
                    self.index(&y, id, true);
                }
            }
        }
        match (old, new) {
            (Some(o), Some(n)) if o.owner == n.owner => {
                self.count(&n.owner, 0, entity_bytes(n) as i64 - entity_bytes(o) as i64);
            }
            _ => {
                if let Some(o) = old {
                    self.count(&o.owner, -1, -(entity_bytes(o) as i64));
                }
                if let Some(n) = new {
                    self.count(&n.owner, 1, entity_bytes(n) as i64);
                }
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
            derived.change(*id, None, Some(entity));
            derived.stale.insert(*id, None);
        }
        let mut world = World {
            tick: d.tick,
            version: d.version,
            next_entity: d.next_entity,
            entities: d.entities,
            derived,
        };
        world.settle();
        world
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

    pub fn near(&self, dim: i64, x: f64, y: f64, r: f64) -> Vec<EntityId> {
        let r = r.clamp(0.0, MAX_RADIUS);
        let mut found: Vec<EntityId> = self
            .in_cells(dim, x, y, r)
            .into_iter()
            .filter(|id| {
                self.entities
                    .get(id)
                    .and_then(Place::of)
                    .is_some_and(|p| p.within(dim, x, y, r))
            })
            .collect();
        found.sort_unstable();
        found
    }

    pub(crate) fn in_cells(&self, dim: i64, x: f64, y: f64, r: f64) -> Vec<EntityId> {
        if !(x.is_finite() && y.is_finite()) {
            return Vec::new();
        }
        let r = r.clamp(0.0, MAX_RADIUS) + CELL;
        let lo = Place {
            dim,
            x: x - r,
            y: y - r,
            reach: 0.0,
        }
        .cell();
        let hi = Place {
            dim,
            x: x + r,
            y: y + r,
            reach: 0.0,
        }
        .cell();
        let mut ids = Vec::new();
        for cx in lo.x..=hi.x {
            let from = Cell {
                dim,
                x: cx,
                y: lo.y,
            };
            let to = Cell {
                dim,
                x: cx,
                y: hi.y,
            };
            for (_, cell) in self.derived.cells.range(from..=to) {
                ids.extend(cell.iter().copied());
            }
        }
        ids.extend(self.derived.wide.get(&dim).into_iter().flatten().copied());
        ids
    }

    pub fn having(&self, key: &str) -> impl Iterator<Item = EntityId> + '_ {
        self.derived.by_key.get(key).into_iter().flatten().copied()
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
            if next_entity != self.next_entity {
                self.next_entity = next_entity;
                self.version += 1;
            }
            return Ok(BTreeSet::new());
        }
        let touched = overlay.keys().copied().collect();
        for (id, slot) in overlay {
            self.derived
                .change(id, self.entities.get(&id), slot.as_ref());
            let old = match slot {
                Some(entity) => self.entities.insert(id, entity),
                None => self.entities.remove(&id),
            };
            self.derived.stale.entry(id).or_insert(old);
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

    pub fn settle(&mut self) {
        if self.derived.stale.is_empty() {
            return;
        }
        let stale = std::mem::take(&mut self.derived.stale);
        let changes: Vec<(EntityId, Option<&Entity>, Option<&Entity>)> = stale
            .iter()
            .map(|(id, old)| (*id, old.as_ref(), self.entities.get(id)))
            .collect();
        let threads = std::thread::available_parallelism()
            .map_or(1, |n| n.get())
            .min(changes.len() / PARALLEL_FROM)
            .max(1);
        let delta = if threads == 1 {
            Digest::of_changes(&changes)
        } else {
            let per = changes.len().div_ceil(threads);
            std::thread::scope(|s| {
                let parts: Vec<_> = changes
                    .chunks(per)
                    .map(|part| s.spawn(move || Digest::of_changes(part)))
                    .collect();
                let mut total = Digest::default();
                for part in parts {
                    total.merge(&part.join().expect("hashing does not panic"));
                }
                total
            })
        };
        self.derived.digest.merge(&delta);
    }

    pub fn hash(&self) -> [u8; 32] {
        assert!(
            self.derived.stale.is_empty(),
            "settle the world before hashing it"
        );
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
