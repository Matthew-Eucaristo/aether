use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use aether_world::{CapsuleId, EntityId, Owner, PlayerId, Txn, Usage, World};
use serde::{Deserialize, Serialize};
use wasmtime::{Store, StoreLimitsBuilder, Trap};

use crate::blob::{BlobHash, Blobs};
use crate::canon::Canon;
use crate::capsule::{Cap, Capsule, Status};
use crate::host::{HostError, HostState, splitmix64};
use crate::input::{Action, Actor, Input, InputOutcome};
use crate::law::Law;
use crate::meter::{DEPTH_EXPORT, DEPTH_LIMIT};
use crate::runtime::{Runtime, THREAD_STACK};

const LOG_BYTES_PER_TICK: usize = 1024;
pub const RULES: &str = "aether-rules/4";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub world: World,
    pub law: Law,
    pub seed: u64,
    pub next_capsule: u64,
    pub capsules: BTreeMap<CapsuleId, Capsule>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Journal {
    pub start: State,
    pub ticks: Vec<Vec<Input>>,
}

#[derive(Debug, thiserror::Error)]
pub enum KernelError {
    #[error("blob {0} is not in the blob store")]
    MissingBlob(BlobHash),
    #[error("invalid capsule module: {0}")]
    InvalidModule(String),
    #[error("invalid snapshot: {0}")]
    InvalidSnapshot(String),
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Fault {
    OutOfFuel,
    DepthLimit,
    Denied(String),
    Trap(String),
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CapsuleRun {
    pub capsule: CapsuleId,
    pub fuel_used: u64,
    pub fault: Option<Fault>,
    pub suspended: bool,
    pub log: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TickReport {
    pub tick: u64,
    pub inputs: Vec<InputOutcome>,
    pub runs: Vec<CapsuleRun>,
    pub touched: BTreeSet<EntityId>,
    pub state_hash: BlobHash,
}

pub struct Kernel {
    state: State,
    blobs: Blobs,
    runtime: Runtime,
    journal: Journal,
}

impl Kernel {
    pub fn new(law: Law, seed: u64) -> Self {
        let state = State {
            world: World::new(),
            law,
            seed,
            next_capsule: 0,
            capsules: BTreeMap::new(),
        };
        Self::from_state(state, Blobs::new()).expect("an empty state is valid")
    }

    pub fn from_state(state: State, blobs: Blobs) -> Result<Self, KernelError> {
        state
            .world
            .validate()
            .map_err(|e| KernelError::InvalidSnapshot(e.to_string()))?;
        check_consistency(&state).map_err(KernelError::InvalidSnapshot)?;
        let mut runtime = Runtime::new();
        for capsule in state.capsules.values() {
            runtime.prepare(&blobs, &capsule.wasm)?;
        }
        let journal = Journal {
            start: state.clone(),
            ticks: Vec::new(),
        };
        Ok(Self {
            state,
            blobs,
            runtime,
            journal,
        })
    }

    pub fn replay(journal: &Journal, blobs: Blobs) -> Result<(Kernel, Vec<BlobHash>), KernelError> {
        let mut kernel = Kernel::from_state(journal.start.clone(), blobs)?;
        let hashes = journal
            .ticks
            .iter()
            .map(|inputs| kernel.step(inputs.clone()).state_hash)
            .collect();
        Ok((kernel, hashes))
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn world(&self) -> &World {
        &self.state.world
    }

    pub fn blobs(&self) -> &Blobs {
        &self.blobs
    }

    pub fn journal(&self) -> &Journal {
        &self.journal
    }

    pub fn add_blob(&mut self, bytes: impl Into<Arc<[u8]>>) -> BlobHash {
        self.blobs.insert(bytes)
    }

    pub fn checkpoint(&mut self) -> Journal {
        let fresh = Journal {
            start: self.state.clone(),
            ticks: Vec::new(),
        };
        std::mem::replace(&mut self.journal, fresh)
    }

    pub fn prune_blobs(&mut self) -> Vec<BlobHash> {
        let used: BTreeSet<BlobHash> = self.state.capsules.values().map(|c| c.wasm).collect();
        let dropped = self.blobs.retain(|hash| used.contains(hash));
        for hash in &dropped {
            self.runtime.forget(hash);
        }
        dropped
    }

    pub fn state_hash(&self) -> BlobHash {
        let State {
            world,
            law,
            seed,
            next_capsule,
            capsules,
        } = &self.state;
        let mut h = blake3::Hasher::new();
        h.update(b"aether-kernel/v2");
        h.update(&world.hash());
        law.feed(&mut h);
        seed.feed(&mut h);
        next_capsule.feed(&mut h);
        capsules.feed(&mut h);
        BlobHash(*h.finalize().as_bytes())
    }

    pub fn step(&mut self, inputs: Vec<Input>) -> TickReport {
        let tick = self.state.world.advance_tick();
        let mut touched = BTreeSet::new();
        let mut sent: BTreeMap<&Actor, u32> = BTreeMap::new();
        let outcomes = inputs
            .iter()
            .map(|input| {
                let count = sent.entry(&input.actor).or_default();
                *count += 1;
                let limit = self.state.law.max_inputs_per_actor;
                if *count > limit {
                    return InputOutcome::Rejected(format!(
                        "actor sent more than {limit} inputs this tick"
                    ));
                }
                self.apply_input(input, &mut touched)
                    .unwrap_or_else(InputOutcome::Rejected)
            })
            .collect();
        let active: Vec<CapsuleId> = self
            .state
            .capsules
            .iter()
            .filter(|(_, c)| c.status == Status::Active)
            .map(|(id, _)| *id)
            .collect();
        let runs = std::thread::scope(|s| {
            std::thread::Builder::new()
                .name("aether-capsules".into())
                .stack_size(THREAD_STACK)
                .spawn_scoped(s, || {
                    active
                        .into_iter()
                        .map(|id| self.run_capsule(id, &mut touched))
                        .collect()
                })
                .expect("the OS provides a capsule thread")
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
        });
        self.journal.ticks.push(inputs);
        TickReport {
            tick,
            inputs: outcomes,
            runs,
            touched,
            state_hash: self.state_hash(),
        }
    }

    fn commit(
        &mut self,
        txn: Txn,
        touched: &mut BTreeSet<EntityId>,
        pending: Option<(CapsuleId, &Actor)>,
    ) -> Result<(), String> {
        self.check_quota(&txn, pending)?;
        touched.extend(
            self.state
                .world
                .commit(txn)
                .expect("transaction was opened against the current world"),
        );
        Ok(())
    }

    fn account_of(&self, owner: &Owner, pending: Option<(CapsuleId, &Actor)>) -> Option<PlayerId> {
        let installer = match owner {
            Owner::Region => return None,
            Owner::Player(p) => return Some(p.clone()),
            Owner::Capsule(c) => match pending {
                Some((id, actor)) if id == *c => actor,
                _ => &self.state.capsules.get(c)?.installer,
            },
        };
        match installer {
            Actor::Admin => None,
            Actor::Player(p) => Some(p.clone()),
        }
    }

    pub fn player_usage(&self, player: &PlayerId) -> Usage {
        let world = &self.state.world;
        let me = Actor::Player(player.clone());
        let own = world.usage(&Owner::Player(player.clone()));
        self.state
            .capsules
            .iter()
            .filter(|(_, c)| c.installer == me)
            .map(|(id, _)| world.usage(&Owner::Capsule(*id)))
            .fold(own, |a, b| Usage {
                entities: a.entities + b.entities,
                bytes: a.bytes + b.bytes,
            })
    }

    fn check_quota(&self, txn: &Txn, pending: Option<(CapsuleId, &Actor)>) -> Result<(), String> {
        let quota = self.state.law.player_quota;
        let mut per: BTreeMap<PlayerId, (i64, i64)> = BTreeMap::new();
        for (owner, (entities, bytes)) in txn.usage_delta(&self.state.world) {
            if let Some(player) = self.account_of(&owner, pending) {
                let d = per.entry(player).or_default();
                d.0 += entities;
                d.1 += bytes;
            }
        }
        for (player, (entities, bytes)) in per {
            let used = self.player_usage(&player);
            if entities > 0 && used.entities as i64 + entities > quota.entities as i64 {
                return Err(format!(
                    "player {} would hold {} entities; the quota is {}",
                    player.0,
                    used.entities as i64 + entities,
                    quota.entities
                ));
            }
            if bytes > 0 && used.bytes as i64 + bytes > quota.bytes as i64 {
                return Err(format!(
                    "player {} would hold {} bytes; the quota is {}",
                    player.0,
                    used.bytes as i64 + bytes,
                    quota.bytes
                ));
            }
        }
        Ok(())
    }

    fn may_modify(&self, actor: &Actor, owner: &Owner) -> bool {
        match (actor, owner) {
            (Actor::Admin, _) => true,
            (Actor::Player(p), Owner::Player(o)) => p == o,
            (Actor::Player(_), Owner::Capsule(c)) => self
                .state
                .capsules
                .get(c)
                .is_some_and(|capsule| capsule.installer == *actor),
            (Actor::Player(_), Owner::Region) => false,
        }
    }

    fn authorize(&self, actor: &Actor, entity: EntityId) -> Result<(), String> {
        let e = self
            .state
            .world
            .entity(entity)
            .ok_or_else(|| format!("entity {} does not exist", entity.0))?;
        if self.may_modify(actor, &e.owner) {
            Ok(())
        } else {
            Err(format!("actor may not modify entity {}", entity.0))
        }
    }

    fn authorize_capsule(&self, actor: &Actor, id: CapsuleId) -> Result<(), String> {
        let capsule = self
            .state
            .capsules
            .get(&id)
            .ok_or_else(|| format!("capsule {} does not exist", id.0))?;
        if *actor == Actor::Admin || *actor == capsule.installer {
            Ok(())
        } else {
            Err(format!("actor may not manage capsule {}", id.0))
        }
    }

    fn apply_input(
        &mut self,
        input: &Input,
        touched: &mut BTreeSet<EntityId>,
    ) -> Result<InputOutcome, String> {
        let law = &self.state.law;
        let world = &self.state.world;
        let actor = &input.actor;
        match &input.action {
            Action::Spawn { components } => {
                if world.len() as u64 >= law.max_entities {
                    return Err("the region holds its maximum number of entities".into());
                }
                let mut txn = world.begin();
                let id = txn.spawn(actor.owner());
                for (key, value) in components {
                    txn.set(world, id, key, value.clone())
                        .map_err(|e| e.to_string())?;
                }
                self.commit(txn, touched, None)?;
                Ok(InputOutcome::Spawned(id))
            }
            Action::Set { entity, key, value } => {
                self.authorize(actor, *entity)?;
                let mut txn = world.begin();
                txn.set(world, *entity, key, value.clone())
                    .map_err(|e| e.to_string())?;
                self.commit(txn, touched, None)?;
                Ok(InputOutcome::Applied)
            }
            Action::Remove { entity, key } => {
                self.authorize(actor, *entity)?;
                let mut txn = world.begin();
                txn.remove(world, *entity, key).map_err(|e| e.to_string())?;
                self.commit(txn, touched, None)?;
                Ok(InputOutcome::Applied)
            }
            Action::Despawn { entity } => {
                self.authorize(actor, *entity)?;
                if let Some((id, _)) = self
                    .state
                    .capsules
                    .iter()
                    .find(|(_, c)| c.self_entity == *entity)
                {
                    return Err(format!(
                        "entity {} is capsule {}'s own entity; uninstall the capsule instead",
                        entity.0, id.0
                    ));
                }
                let mut txn = world.begin();
                txn.despawn(world, *entity).map_err(|e| e.to_string())?;
                self.commit(txn, touched, None)?;
                Ok(InputOutcome::Applied)
            }
            Action::Install {
                wasm,
                grants,
                budget,
            } => {
                if self.state.capsules.len() >= law.max_capsules as usize {
                    return Err("the region holds its maximum number of capsules".into());
                }
                check_module(&mut self.runtime, &self.blobs, law, wasm)?;
                if grants.len() + 1 > law.max_clist_len as usize {
                    return Err("too many grants for the region's capability list limit".into());
                }
                for grant in grants {
                    let e = world
                        .entity(grant.entity)
                        .ok_or_else(|| format!("entity {} does not exist", grant.entity.0))?;
                    if grant.write && !self.may_modify(actor, &e.owner) {
                        return Err(format!(
                            "actor may not grant write access to entity {}",
                            grant.entity.0
                        ));
                    }
                }
                if world.len() as u64 >= law.max_entities {
                    return Err("the region holds its maximum number of entities".into());
                }
                let id = CapsuleId(self.state.next_capsule);
                let mut txn = world.begin();
                let self_entity = txn.spawn(Owner::Capsule(id));
                let mut clist = vec![Some(Cap {
                    entity: self_entity,
                    write: true,
                })];
                clist.extend(grants.iter().map(|g| {
                    Some(Cap {
                        entity: g.entity,
                        write: g.write,
                    })
                }));
                let capsule = Capsule {
                    installer: actor.clone(),
                    wasm: *wasm,
                    self_entity,
                    clist,
                    budget: *budget,
                    status: Status::Active,
                    consecutive_faults: 0,
                };
                self.commit(txn, touched, Some((id, actor)))?;
                self.state.next_capsule += 1;
                self.state.capsules.insert(id, capsule);
                Ok(InputOutcome::Installed(id))
            }
            Action::Uninstall { capsule } => {
                self.authorize_capsule(actor, *capsule)?;
                let owner = Owner::Capsule(*capsule);
                let mut txn = world.begin();
                for (id, _) in world.entities().filter(|(_, e)| e.owner == owner) {
                    txn.despawn(world, id).map_err(|e| e.to_string())?;
                }
                self.commit(txn, touched, None)?;
                self.state.capsules.remove(capsule);
                Ok(InputOutcome::Applied)
            }
            Action::Upgrade { capsule, wasm } => {
                self.authorize_capsule(actor, *capsule)?;
                check_module(&mut self.runtime, &self.blobs, law, wasm)?;
                let c = self
                    .state
                    .capsules
                    .get_mut(capsule)
                    .expect("authorize_capsule checked existence");
                c.wasm = *wasm;
                c.status = Status::Active;
                c.consecutive_faults = 0;
                Ok(InputOutcome::Applied)
            }
            Action::Grant { capsule, grant } => {
                self.authorize_capsule(actor, *capsule)?;
                let e = world
                    .entity(grant.entity)
                    .ok_or_else(|| format!("entity {} does not exist", grant.entity.0))?;
                if grant.write && !self.may_modify(actor, &e.owner) {
                    return Err(format!(
                        "actor may not grant write access to entity {}",
                        grant.entity.0
                    ));
                }
                let max = law.max_clist_len as usize;
                let c = self
                    .state
                    .capsules
                    .get_mut(capsule)
                    .expect("authorize_capsule checked existence");
                if c.clist.len() >= max {
                    return Err("the capsule's capability list is full".into());
                }
                c.clist.push(Some(Cap {
                    entity: grant.entity,
                    write: grant.write,
                }));
                let handle = u32::try_from(c.clist.len() - 1)
                    .expect("clist length fits the law's u32 limit");
                Ok(InputOutcome::Granted(handle))
            }
            Action::Revoke { capsule, handle } => {
                self.authorize_capsule(actor, *capsule)?;
                if *handle == 0 {
                    return Err("handle 0 is the capsule's own entity and cannot be revoked".into());
                }
                let c = self
                    .state
                    .capsules
                    .get_mut(capsule)
                    .expect("authorize_capsule checked existence");
                match c.clist.get_mut(*handle as usize) {
                    Some(slot @ Some(_)) => {
                        *slot = None;
                        Ok(InputOutcome::Applied)
                    }
                    _ => Err(format!("capsule {} holds no handle {handle}", capsule.0)),
                }
            }
            Action::Resume { capsule } => {
                self.authorize_capsule(actor, *capsule)?;
                let c = self
                    .state
                    .capsules
                    .get_mut(capsule)
                    .expect("authorize_capsule checked existence");
                c.status = Status::Active;
                c.consecutive_faults = 0;
                Ok(InputOutcome::Applied)
            }
            Action::SetLaw { law } => {
                if *actor != Actor::Admin {
                    return Err("only the region admin may change the law".into());
                }
                self.state.law = law.clone();
                Ok(InputOutcome::Applied)
            }
        }
    }

    fn run_capsule(&mut self, id: CapsuleId, touched: &mut BTreeSet<EntityId>) -> CapsuleRun {
        let law = self.state.law.clone();
        let capsule = self.state.capsules[&id].clone();
        let budget = law.attenuate(&capsule.budget);
        let prepared = self
            .runtime
            .prepare(&self.blobs, &capsule.wasm)
            .expect("installed capsules were validated and their blobs are retained");

        let mut tick_seed = self.state.world.tick();
        let mut capsule_seed = id.0;
        let rng = self.state.seed
            ^ splitmix64(&mut tick_seed)
            ^ splitmix64(&mut capsule_seed).rotate_left(17);

        let world = std::mem::take(&mut self.state.world);
        let txn = world.begin();
        let host = HostState {
            world,
            txn,
            capsule: id,
            clist: capsule.clist.clone(),
            max_clist_len: law.max_clist_len as usize,
            max_entities: law.max_entities,
            writes_left: budget.writes_per_tick,
            spawns_left: budget.spawns_per_tick,
            rng,
            log: Vec::new(),
            log_bytes_left: LOG_BYTES_PER_TICK,
            limits: StoreLimitsBuilder::new()
                .memory_size(usize::try_from(budget.memory_bytes).unwrap_or(usize::MAX))
                .instances(1)
                .memories(1)
                .tables(1)
                .table_elements(10_000)
                .build(),
        };
        let mut store = Store::new(self.runtime.engine(), host);
        store.limiter(|s| &mut s.limits);
        store
            .set_fuel(u64::MAX)
            .expect("fuel metering is enabled in the engine config");

        let mut fuel_used = 0;
        let result = match prepared.pre.instantiate(&mut store) {
            Err(err) => Err(classify(&err)),
            Ok(_) if prepared.init_fuel > budget.fuel_per_tick => {
                fuel_used = budget.fuel_per_tick;
                Err(Fault::OutOfFuel)
            }
            Ok(instance) => {
                store
                    .set_fuel(budget.fuel_per_tick - prepared.init_fuel)
                    .expect("fuel metering is enabled in the engine config");
                let called = instance
                    .get_typed_func::<(), ()>(&mut store, "tick")
                    .and_then(|tick| tick.call(&mut store, ()));
                fuel_used = budget.fuel_per_tick - store.get_fuel().unwrap_or(0);
                called.map_err(|err| {
                    let depth = instance
                        .get_global(&mut store, DEPTH_EXPORT)
                        .and_then(|g| g.get(&mut store).i32());
                    match depth {
                        Some(d) if d.unsigned_abs() > DEPTH_LIMIT => Fault::DepthLimit,
                        _ => classify(&err),
                    }
                })
            }
        };
        let host = store.into_data();
        self.state.world = host.world;
        let result = result.and_then(|()| self.check_quota(&host.txn, None).map_err(Fault::Denied));

        let record = self
            .state
            .capsules
            .get_mut(&id)
            .expect("capsule is installed for the whole tick");
        match result {
            Ok(()) => {
                record.clist = host.clist;
                record.consecutive_faults = 0;
                touched.extend(
                    self.state
                        .world
                        .commit(host.txn)
                        .expect("transaction was opened against the current world"),
                );
                CapsuleRun {
                    capsule: id,
                    fuel_used,
                    fault: None,
                    suspended: false,
                    log: host.log,
                }
            }
            Err(fault) => {
                record.consecutive_faults += 1;
                let suspended = record.consecutive_faults >= law.max_consecutive_faults;
                if suspended {
                    record.status = Status::Suspended;
                }
                CapsuleRun {
                    capsule: id,
                    fuel_used,
                    fault: Some(fault),
                    suspended,
                    log: host.log,
                }
            }
        }
    }
}

fn check_consistency(state: &State) -> Result<(), String> {
    if let Some(max) = state.capsules.keys().max()
        && max.0 >= state.next_capsule
    {
        return Err("capsule ids must be below next_capsule".into());
    }
    for (id, capsule) in &state.capsules {
        let own = state
            .world
            .entity(capsule.self_entity)
            .ok_or_else(|| format!("capsule {}'s own entity is missing", id.0))?;
        if own.owner != Owner::Capsule(*id) {
            return Err(format!(
                "capsule {}'s own entity belongs to someone else",
                id.0
            ));
        }
        let first = Some(Cap {
            entity: capsule.self_entity,
            write: true,
        });
        if capsule.clist.first() != Some(&first) {
            return Err(format!(
                "capsule {}'s handle 0 must be its own entity with write access",
                id.0
            ));
        }
    }
    for (entity_id, entity) in state.world.entities() {
        if let Owner::Capsule(c) = entity.owner
            && !state.capsules.contains_key(&c)
        {
            return Err(format!(
                "entity {} belongs to capsule {}, which is not installed",
                entity_id.0, c.0
            ));
        }
    }
    Ok(())
}

fn check_module(
    runtime: &mut Runtime,
    blobs: &Blobs,
    law: &Law,
    wasm: &BlobHash,
) -> Result<(), String> {
    let size = blobs
        .get(wasm)
        .ok_or_else(|| format!("blob {wasm} is not in the blob store"))?
        .len();
    let max = law.max_module_bytes;
    if size as u64 > max {
        return Err(format!(
            "module is {size} bytes; the region law allows {max}"
        ));
    }
    runtime
        .prepare(blobs, wasm)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn classify(err: &wasmtime::Error) -> Fault {
    if let Some(host) = err.downcast_ref::<HostError>() {
        return Fault::Denied(host.to_string());
    }
    match err.downcast_ref::<Trap>() {
        Some(Trap::OutOfFuel) => Fault::OutOfFuel,
        Some(trap) => Fault::Trap(trap.to_string()),
        None => Fault::Trap(err.to_string()),
    }
}
