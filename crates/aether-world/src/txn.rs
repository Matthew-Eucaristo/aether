use std::collections::BTreeMap;
use std::ops::Bound;

use crate::types::{
    Entity, EntityId, MAX_COMPONENTS, Owner, Value, WorldError, entity_bytes, validate_key,
    validate_value,
};
use crate::world::{MAX_RADIUS, Place, World};

#[derive(Debug)]
pub struct Txn {
    base_version: u64,
    next_entity: u64,
    overlay: BTreeMap<EntityId, Option<Entity>>,
}

impl Txn {
    pub(crate) fn new(world: &World) -> Self {
        Self {
            base_version: world.version(),
            next_entity: world.next_entity(),
            overlay: BTreeMap::new(),
        }
    }

    pub(crate) fn base_version(&self) -> u64 {
        self.base_version
    }

    pub(crate) fn into_parts(self) -> (u64, BTreeMap<EntityId, Option<Entity>>) {
        (self.next_entity, self.overlay)
    }

    pub fn is_empty(&self) -> bool {
        self.overlay.is_empty()
    }

    pub fn entity<'a>(&'a self, world: &'a World, id: EntityId) -> Option<&'a Entity> {
        match self.overlay.get(&id) {
            Some(slot) => slot.as_ref(),
            None => world.entity(id),
        }
    }

    pub fn get<'a>(
        &'a self,
        world: &'a World,
        id: EntityId,
        key: &str,
    ) -> Result<Option<&'a Value>, WorldError> {
        let entity = self.entity(world, id).ok_or(WorldError::NoSuchEntity(id))?;
        Ok(entity.components.get(key))
    }

    pub fn live_count(&self, world: &World) -> u64 {
        let mut count = world.len() as u64;
        for (id, slot) in &self.overlay {
            match (slot.is_some(), world.entity(*id).is_some()) {
                (true, false) => count += 1,
                (false, true) => count -= 1,
                _ => {}
            }
        }
        count
    }

    pub fn find(
        &self,
        world: &World,
        key: &str,
        after: Option<EntityId>,
    ) -> (Option<EntityId>, u64) {
        let range = (
            after.map_or(Bound::Unbounded, Bound::Excluded),
            Bound::Unbounded,
        );
        let mut examined = 0;
        let has_key = |slot: &Option<Entity>| {
            slot.as_ref()
                .is_some_and(|e| e.components.contains_key(key))
        };
        let committed = world.ids_with(key).and_then(|ids| {
            ids.range(range).copied().find(|id| {
                examined += 1;
                self.overlay.get(id).is_none_or(has_key)
            })
        });
        let pending = self.overlay.range(range).find_map(|(id, slot)| {
            examined += 1;
            has_key(slot).then_some(*id)
        });
        let found = match (committed, pending) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        (found, examined)
    }

    pub fn near(
        &self,
        world: &World,
        dim: i64,
        x: f64,
        y: f64,
        r: f64,
        after: Option<EntityId>,
    ) -> (Option<EntityId>, u64) {
        let r = r.clamp(0.0, MAX_RADIUS);
        let beyond = |id: &EntityId| after.is_none_or(|a| *id > a);
        let inside = |e: &Entity| Place::of(e).is_some_and(|p| p.within(dim, x, y, r));
        let mut examined = 0;
        let mut found: Option<EntityId> = None;
        for id in world.in_cells(dim, x, y, r) {
            examined += 1;
            if beyond(&id)
                && !self.overlay.contains_key(&id)
                && world.entity(id).is_some_and(inside)
                && found.is_none_or(|f| id < f)
            {
                found = Some(id);
            }
        }
        for (id, slot) in &self.overlay {
            examined += 1;
            if beyond(id) && slot.as_ref().is_some_and(inside) && found.is_none_or(|f| *id < f) {
                found = Some(*id);
            }
        }
        (found, examined)
    }

    pub fn usage_delta(&self, world: &World) -> BTreeMap<Owner, (i64, i64)> {
        let mut delta: BTreeMap<Owner, (i64, i64)> = BTreeMap::new();
        for (id, slot) in &self.overlay {
            if let Some(old) = world.entity(*id) {
                let d = delta.entry(old.owner.clone()).or_default();
                d.0 -= 1;
                d.1 -= entity_bytes(old) as i64;
            }
            if let Some(new) = slot {
                let d = delta.entry(new.owner.clone()).or_default();
                d.0 += 1;
                d.1 += entity_bytes(new) as i64;
            }
        }
        delta.retain(|_, d| *d != (0, 0));
        delta
    }

    pub fn spawn(&mut self, owner: Owner) -> EntityId {
        let id = EntityId(self.next_entity);
        self.next_entity += 1;
        self.overlay.insert(
            id,
            Some(Entity {
                owner,
                components: BTreeMap::new(),
            }),
        );
        id
    }

    pub fn set(
        &mut self,
        world: &World,
        id: EntityId,
        key: &str,
        value: Value,
    ) -> Result<(), WorldError> {
        validate_key(key)?;
        validate_value(&value)?;
        let entity = self.entity_mut(world, id)?;
        if !entity.components.contains_key(key) && entity.components.len() >= MAX_COMPONENTS {
            return Err(WorldError::TooManyComponents);
        }
        entity.components.insert(key.to_owned(), value);
        Ok(())
    }

    pub fn remove(&mut self, world: &World, id: EntityId, key: &str) -> Result<bool, WorldError> {
        if self.get(world, id, key)?.is_none() {
            return Ok(false);
        }
        Ok(self.entity_mut(world, id)?.components.remove(key).is_some())
    }

    pub fn despawn(&mut self, world: &World, id: EntityId) -> Result<(), WorldError> {
        self.entity(world, id).ok_or(WorldError::NoSuchEntity(id))?;
        if world.entity(id).is_some() {
            self.overlay.insert(id, None);
        } else {
            self.overlay.remove(&id);
        }
        Ok(())
    }

    fn entity_mut(&mut self, world: &World, id: EntityId) -> Result<&mut Entity, WorldError> {
        let slot = match self.overlay.entry(id) {
            std::collections::btree_map::Entry::Occupied(o) => o.into_mut(),
            std::collections::btree_map::Entry::Vacant(v) => {
                let entity = world.entity(id).ok_or(WorldError::NoSuchEntity(id))?;
                v.insert(Some(entity.clone()))
            }
        };
        slot.as_mut().ok_or(WorldError::NoSuchEntity(id))
    }
}
