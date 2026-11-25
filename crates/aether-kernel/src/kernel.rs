use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use aether_world::{CapsuleId, EntityId, Owner, PlayerId, Txn, Usage, World};
use serde::{Deserialize, Serialize};
use wasmtime::{Instance, Store, StoreLimitsBuilder, Trap, Val};

use crate::blob::{BlobHash, Blobs};
use crate::canon::Canon;
use crate::capsule::{Cap, Capsule, Status, claim_slot};
use crate::host::{HostError, HostState, splitmix64};
use crate::input::{Action, Actor, Input, InputOutcome};
use crate::law::Law;
use crate::meter::{DEPTH_EXPORT, DEPTH_LIMIT, Init, MEMORY_EXPORT, Reset};
use crate::pledge::{self, Pledge, Stake};
use crate::runtime::{Runtime, TABLE_ELEMENTS, THREAD_STACK};
use crate::signal::{Sender, Signal};

const LOG_BYTES_PER_TICK: usize = 1024;
pub const RULES: &str = "aether-rules/18";
pub const GUARANTEED_FUEL: u64 = 250_000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub world: World,
    pub law: Law,
    pub seed: u64,
    pub next_capsule: u64,
    pub capsules: BTreeMap<CapsuleId, Capsule>,
    pub signals: Vec<Signal>,
    pub pledges: Vec<Pledge>,
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
    pub signals: Vec<Signal>,
    pub state_hash: BlobHash,
}

pub struct Kernel {
    state: State,
    blobs: Blobs,
    runtime: Runtime,
    journal: Journal,
    delivering: Vec<Signal>,
    live: HashMap<CapsuleId, Live>,
}

struct Live {
    wasm: BlobHash,
    memory_bytes: u64,
    store: Store<HostState>,
    instance: Instance,
}

fn reset(store: &mut Store<HostState>, instance: &Instance, r: &Reset) -> Option<()> {
    if let Some(pages) = r.pages {
        let memory = instance.get_memory(&mut *store, MEMORY_EXPORT)?;
        if memory.size(&*store) != pages {
            return None;
        }
        memory.data_mut(&mut *store).fill(0);
        for (at, bytes) in &r.data {
            memory.write(&mut *store, *at as usize, bytes).ok()?;
        }
    }
    for (name, init) in &r.globals {
        let value = match *init {
            Init::I32(v) => Val::I32(v),
            Init::I64(v) => Val::I64(v),
            Init::F32(v) => Val::F32(v),
            Init::F64(v) => Val::F64(v),
            Init::V128(v) => Val::V128(v.into()),
        };
        instance
            .get_global(&mut *store, name)?
            .set(&mut *store, value)
            .ok()?;
    }
    Some(())
}

impl Kernel {
    pub fn new(law: Law, seed: u64) -> Self {
        let state = State {
            world: World::new(),
            law,
            seed,
            next_capsule: 0,
            capsules: BTreeMap::new(),
            signals: Vec::new(),
            pledges: Vec::new(),
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
            delivering: Vec::new(),
            live: HashMap::new(),
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
            signals,
            pledges,
        } = &self.state;
        let mut h = blake3::Hasher::new();
        h.update(b"aether-kernel/v2");
        h.update(&world.hash());
        law.feed(&mut h);
        seed.feed(&mut h);
        next_capsule.feed(&mut h);
        capsules.feed(&mut h);
        signals.feed(&mut h);
        pledges.feed(&mut h);
        BlobHash(*h.finalize().as_bytes())
    }

    pub fn step(&mut self, inputs: Vec<Input>) -> TickReport {
        let tick = self.state.world.advance_tick();
        let mut touched = BTreeSet::new();
        self.delivering = std::mem::take(&mut self.state.signals);
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
        let signals: Arc<[Signal]> = std::mem::take(&mut self.delivering).into();
        let installers: Arc<BTreeMap<CapsuleId, String>> = Arc::new(
            self.state
                .capsules
                .iter()
                .map(|(id, c)| (*id, Sender::Actor(c.installer.clone()).label()))
                .collect(),
        );
        let runs = std::thread::scope(|s| {
            std::thread::Builder::new()
                .name("aether-capsules".into())
                .stack_size(THREAD_STACK)
                .spawn_scoped(s, || {
                    let mut remaining = self.state.law.region_fuel_per_tick;
                    let floor = (remaining / active.len().max(1) as u64).min(GUARANTEED_FUEL);
                    let count = active.len() as u64;
                    active
                        .into_iter()
                        .enumerate()
                        .map(|(i, id)| {
                            let reserve = floor * (count - 1 - i as u64);
                            let cap = remaining.saturating_sub(reserve);
                            let run =
                                self.run_capsule(id, cap, &signals, &installers, &mut touched);
                            remaining = remaining.saturating_sub(run.fuel_used);
                            run
                        })
                        .collect()
                })
                .expect("the OS provides a capsule thread")
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
        });
        crate::physics::step(&mut self.state.world, &self.state.law, &mut touched);
        self.prune_pledges();
        self.journal.ticks.push(inputs);
        self.live.retain(|id, l| {
            self.state
                .capsules
                .get(id)
                .is_some_and(|c| c.wasm == l.wasm && c.status == Status::Active)
        });
        self.state.world.settle();
        TickReport {
            tick,
            inputs: outcomes,
            runs,
            touched,
            signals: signals.to_vec(),
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

    fn frozen_entity(&self, actor: &Actor, entity: EntityId) -> Option<String> {
        if *actor == Actor::Admin {
            return None;
        }
        if let Some(p) = pledge::of(&self.state.pledges, Stake::Entity(entity)) {
            return Some(format!(
                "entity {} is pledged to covenant {}",
                entity.0, p.covenant.0
            ));
        }
        match self.state.world.entity(entity).map(|e| &e.owner) {
            Some(Owner::Capsule(c)) => pledge::of(&self.state.pledges, Stake::Capsule(*c))
                .map(|p| format!("capsule {} is pledged to covenant {}", c.0, p.covenant.0)),
            _ => None,
        }
    }

    fn frozen_capsule(&self, actor: &Actor, id: CapsuleId) -> Option<String> {
        if *actor == Actor::Admin {
            return None;
        }
        if let Some(p) = pledge::of(&self.state.pledges, Stake::Capsule(id)) {
            return Some(format!(
                "capsule {} is pledged to covenant {}",
                id.0, p.covenant.0
            ));
        }
        let holds_others = self
            .state
            .pledges
            .iter()
            .any(|p| p.covenant == id && Actor::Player(p.from.clone()) != *actor);
        holds_others.then(|| format!("covenant {} holds other players' pledges", id.0))
    }

    fn prune_pledges(&mut self) {
        let State {
            world,
            capsules,
            pledges,
            ..
        } = &mut self.state;
        pledges.retain(|p| {
            let covenant = capsules
                .get(&p.covenant)
                .is_some_and(|c| c.status == Status::Active);
            let stake = match p.stake {
                Stake::Entity(e) => world.entity(e).is_some(),
                Stake::Capsule(c) => capsules.contains_key(&c),
            };
            covenant && stake
        });
    }

    fn authorize(&self, actor: &Actor, entity: EntityId) -> Result<(), String> {
        if let Some(why) = self.frozen_entity(actor, entity) {
            return Err(why);
        }
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
        if let Some(why) = self.frozen_capsule(actor, id) {
            return Err(why);
        }
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
                    if grant.write
                        && let Some(why) = self.frozen_entity(actor, grant.entity)
                    {
                        return Err(why);
                    }
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
                if grant.write
                    && let Some(why) = self.frozen_entity(actor, grant.entity)
                {
                    return Err(why);
                }
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
                let cap = Cap {
                    entity: grant.entity,
                    write: grant.write,
                };
                let handle = claim_slot(&mut c.clist, cap, max)
                    .ok_or("the capsule's capability list is full")?;
                let handle = u32::try_from(handle).expect("clist length fits the law's u32 limit");
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
            Action::Signal {
                entity,
                name,
                value,
            } => {
                aether_world::validate_key(name).map_err(|e| e.to_string())?;
                if !value.is_finite() {
                    return Err("signal values must be finite".into());
                }
                if world.entity(*entity).is_none() {
                    return Err(format!("entity {} does not exist", entity.0));
                }
                self.delivering.push(Signal {
                    from: Sender::Actor(actor.clone()),
                    entity: *entity,
                    name: name.clone(),
                    value: *value,
                });
                Ok(InputOutcome::Applied)
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
            Action::Pledge { covenant, stake } => {
                let Actor::Player(player) = actor else {
                    return Err("only players pledge stakes".into());
                };
                let c = self
                    .state
                    .capsules
                    .get(covenant)
                    .filter(|c| c.status == Status::Active)
                    .ok_or_else(|| format!("covenant {} is not an active capsule", covenant.0))?;
                let target = c.self_entity;
                if pledge::of(&self.state.pledges, *stake).is_some() {
                    return Err("that stake is already pledged".into());
                }
                let name = match stake {
                    Stake::Entity(e) => {
                        let owner = &world
                            .entity(*e)
                            .ok_or_else(|| format!("entity {} does not exist", e.0))?
                            .owner;
                        if *owner != Owner::Player(player.clone()) {
                            return Err(format!("{} does not own entity {}", player.0, e.0));
                        }
                        ("pledge", e.0)
                    }
                    Stake::Capsule(id) => {
                        if id == covenant {
                            return Err("a covenant cannot be its own stake".into());
                        }
                        let staked = self
                            .state
                            .capsules
                            .get(id)
                            .ok_or_else(|| format!("capsule {} does not exist", id.0))?;
                        if staked.installer != *actor {
                            return Err(format!("{} did not install capsule {}", player.0, id.0));
                        }
                        ("pledge_capsule", id.0)
                    }
                };
                let at = self.state.pledges.partition_point(|p| p.stake < *stake);
                self.state.pledges.insert(
                    at,
                    Pledge {
                        stake: *stake,
                        covenant: *covenant,
                        from: player.clone(),
                    },
                );
                self.delivering.push(Signal {
                    from: Sender::Actor(actor.clone()),
                    entity: target,
                    name: name.0.into(),
                    value: name.1 as f64,
                });
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

    fn run_capsule(
        &mut self,
        id: CapsuleId,
        region_cap: u64,
        signals: &Arc<[Signal]>,
        installers: &Arc<BTreeMap<CapsuleId, String>>,
        touched: &mut BTreeSet<EntityId>,
    ) -> CapsuleRun {
        let law = self.state.law.clone();
        let capsule = self.state.capsules[&id].clone();
        let mut budget = law.attenuate(&capsule.budget);
        budget.fuel_per_tick = budget.fuel_per_tick.min(region_cap);
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
            signals: signals.clone(),
            installers: installers.clone(),
            emitted: Vec::new(),
            limits: StoreLimitsBuilder::new()
                .memory_size(usize::try_from(budget.memory_bytes).unwrap_or(usize::MAX))
                .instances(1)
                .memories(1)
                .tables(1)
                .table_elements(TABLE_ELEMENTS)
                .build(),
            terrain: law.terrain,
            near: None,
            pledges: Arc::new(self.state.pledges.clone()),
            settled: Vec::new(),
        };
        let cached = self.live.remove(&id).filter(|l| {
            l.wasm == capsule.wasm
                && l.memory_bytes == budget.memory_bytes
                && prepared.reset.is_some()
        });
        let (mut store, instance) = match cached {
            Some(mut live) => {
                *live.store.data_mut() = host;
                let reset_ok = reset(
                    &mut live.store,
                    &live.instance,
                    prepared.reset.as_ref().expect("filtered above"),
                );
                match reset_ok {
                    Some(()) => (live.store, Ok(live.instance)),
                    None => {
                        let host = std::mem::take(live.store.data_mut());
                        let mut store = Store::new(self.runtime.engine(), host);
                        store.limiter(|s| &mut s.limits);
                        store
                            .set_fuel(u64::MAX)
                            .expect("fuel metering is enabled in the engine config");
                        let instance = prepared.pre.instantiate(&mut store);
                        (store, instance)
                    }
                }
            }
            None => {
                let mut store = Store::new(self.runtime.engine(), host);
                store.limiter(|s| &mut s.limits);
                store
                    .set_fuel(u64::MAX)
                    .expect("fuel metering is enabled in the engine config");
                let instance = prepared.pre.instantiate(&mut store);
                (store, instance)
            }
        };

        let mut fuel_used = 0;
        let mut keep = None;
        let result = match instance {
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
                if let Some(r) = &prepared.reset {
                    let grown = r.pages.is_some_and(|pages| {
                        instance
                            .get_memory(&mut store, MEMORY_EXPORT)
                            .is_none_or(|m| m.size(&store) != pages)
                    });
                    if !grown {
                        keep = Some(instance);
                    }
                }
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
        let host = std::mem::take(store.data_mut());
        if let Some(instance) = keep {
            self.live.insert(
                id,
                Live {
                    wasm: capsule.wasm,
                    memory_bytes: budget.memory_bytes,
                    store,
                    instance,
                },
            );
        }
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
                self.state.signals.extend(host.emitted);
                for (stake, winner) in host.settled {
                    self.state.pledges.retain(|p| p.stake != stake);
                    let Some(winner) = winner else { continue };
                    let winner = Actor::Player(winner);
                    let (won, kept_by) = match stake {
                        Stake::Entity(e) => (BTreeSet::from([e]), None),
                        Stake::Capsule(c) => {
                            let owned = Owner::Capsule(c);
                            let won = self
                                .state
                                .world
                                .entities()
                                .filter(|(_, e)| e.owner == owned)
                                .map(|(id, _)| id)
                                .collect();
                            if let Some(staked) = self.state.capsules.get_mut(&c) {
                                staked.installer = winner.clone();
                            }
                            (won, Some(c))
                        }
                    };
                    for (cid, other) in self.state.capsules.iter_mut() {
                        if Some(*cid) == kept_by || other.installer == winner {
                            continue;
                        }
                        for slot in other.clist.iter_mut().skip(1) {
                            if slot.is_some_and(|cap| cap.write && won.contains(&cap.entity)) {
                                *slot = None;
                            }
                        }
                    }
                }
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

#[cfg(test)]
mod reuse {
    use super::*;

    const DIRTY: &str = r#"(module
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (memory (export "memory") 1)
  (global $g (mut i32) (i32.const 7))
  (global $f (mut f64) (f64.const 2.5))
  (data (i32.const 0) "seen")
  (data (i32.const 8) "glob")
  (data (i32.const 16) "size")
  (data (i32.const 24) "word")
  (data (i32.const 32) "flt")
  (func (export "tick")
    (call $set (i32.const 0) (i32.const 0) (i32.const 4) (f64.convert_i32_u (i32.load (i32.const 100))))
    (call $set (i32.const 0) (i32.const 8) (i32.const 4) (f64.convert_i32_s (global.get $g)))
    (call $set (i32.const 0) (i32.const 16) (i32.const 4) (f64.convert_i32_s (memory.size)))
    (call $set (i32.const 0) (i32.const 24) (i32.const 4) (f64.convert_i32_u (i32.load8_u (i32.const 0))))
    (call $set (i32.const 0) (i32.const 32) (i32.const 3) (global.get $f))
    (i32.store (i32.const 100) (i32.const 999))
    (i32.store8 (i32.const 0) (i32.const 0))
    (global.set $g (i32.const 1234))
    (global.set $f (f64.const -1))
    GROW))"#;

    fn run(src: &str) -> (Vec<Vec<f64>>, usize) {
        let mut k = Kernel::new(Law::default(), 1);
        let wasm = k.add_blob(src.as_bytes().to_vec());
        let mut seen = Vec::new();
        let mut cached = 0;
        let report = k.step(vec![Input {
            actor: Actor::Admin,
            action: Action::Install {
                wasm,
                grants: Vec::new(),
                budget: Law::default().ceiling,
            },
        }]);
        assert!(
            report.runs.iter().all(|r| r.fault.is_none()),
            "{:?}",
            report.runs
        );
        for _ in 0..5 {
            let report = k.step(Vec::new());
            assert!(
                report.runs.iter().all(|r| r.fault.is_none()),
                "{:?}",
                report.runs
            );
            let me = k.state().capsules[&CapsuleId(0)].self_entity;
            let e = k.world().entity(me).expect("own entity");
            seen.push(
                ["seen", "glob", "size", "word", "flt"]
                    .iter()
                    .map(|key| match e.components.get(*key) {
                        Some(aether_world::Value::Num(n)) => *n,
                        other => panic!("{key}: {other:?}"),
                    })
                    .collect(),
            );
            cached = cached.max(k.live.len());
        }
        (seen, cached)
    }

    #[test]
    fn reused_instances_start_every_tick_exactly_like_fresh_ones() {
        let fresh = vec![0.0, 7.0, 1.0, 115.0, 2.5];
        let (seen, cached) = run(&DIRTY.replace("GROW", ""));
        assert_eq!(cached, 1, "the instance is kept between ticks");
        for tick in &seen {
            assert_eq!(tick, &fresh);
        }
        let (seen, cached) = run(&DIRTY.replace("GROW", "(drop (memory.grow (i32.const 1)))"));
        assert_eq!(cached, 0, "an instance whose memory grew is not kept");
        for tick in &seen {
            assert_eq!(tick, &fresh);
        }
    }
}
