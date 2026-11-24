use std::collections::BTreeMap;
use std::sync::Arc;

use aether_world::{
    CapsuleId, Entity, EntityId, MAX_KEY_LEN, MAX_NUMS, MAX_TEXT_LEN, Owner, Txn, Value, World,
    WorldError,
};
use wasmtime::{Caller, Engine, Error, Linker, StoreLimits, Trap};

use crate::capsule::{Cap, claim_slot};
use crate::signal::{Sender, Signal};

const COST_READ: u64 = 20;
const COST_WRITE: u64 = 50;
const COST_SPAWN: u64 = 1_000;
const COST_DESPAWN: u64 = 200;
const COST_LOG: u64 = 10;
const COST_MATH: u64 = 20;

type Unary = fn(f64) -> f64;
type Binary = fn(f64, f64) -> f64;

pub(crate) struct HostState {
    pub world: World,
    pub txn: Txn,
    pub capsule: CapsuleId,
    pub clist: Vec<Option<Cap>>,
    pub max_clist_len: usize,
    pub max_entities: u64,
    pub writes_left: u32,
    pub spawns_left: u32,
    pub rng: u64,
    pub log: Vec<String>,
    pub log_bytes_left: usize,
    pub signals: Arc<[Signal]>,
    pub installers: Arc<BTreeMap<CapsuleId, String>>,
    pub emitted: Vec<Signal>,
    pub limits: StoreLimits,
}

impl Default for HostState {
    fn default() -> Self {
        let world = World::default();
        let txn = world.begin();
        HostState {
            world,
            txn,
            capsule: CapsuleId(0),
            clist: Vec::new(),
            max_clist_len: 0,
            max_entities: 0,
            writes_left: 0,
            spawns_left: 0,
            rng: 0,
            log: Vec::new(),
            log_bytes_left: 0,
            signals: Arc::from(Vec::new()),
            installers: Arc::new(BTreeMap::new()),
            emitted: Vec::new(),
            limits: wasmtime::StoreLimitsBuilder::new().build(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum HostError {
    #[error("handle {0} is not in this capsule's capability list")]
    BadHandle(i32),
    #[error("handle {0} does not grant write access")]
    NoWrite(i32),
    #[error("handle {0} refers to an entity this capsule did not create")]
    NotOwner(i32),
    #[error("a capsule cannot despawn its own state entity")]
    SelfDespawn,
    #[error("component {0:?} is missing or not a {1}")]
    WrongKind(String, &'static str),
    #[error("no spawns left this tick under the region law")]
    SpawnBudget,
    #[error("no writes left this tick")]
    WriteBudget,
    #[error("the region holds its maximum number of entities")]
    RegionFull,
    #[error("the capability list is full")]
    ClistFull,
    #[error("there is no signal {0} this tick")]
    BadSignal(i32),
    #[error("guest memory access out of bounds")]
    BadMemory,
    #[error("guest string is not valid UTF-8")]
    BadUtf8,
    #[error("guest string is too long")]
    TooLong,
    #[error(transparent)]
    World(#[from] WorldError),
}

pub(crate) fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn deny<T>(r: Result<T, HostError>) -> wasmtime::Result<T> {
    r.map_err(Error::new)
}

fn charge(caller: &mut Caller<'_, HostState>, cost: u64) -> wasmtime::Result<()> {
    let fuel = caller.get_fuel()?;
    match fuel.checked_sub(cost) {
        Some(left) => caller.set_fuel(left),
        None => {
            caller.set_fuel(0)?;
            Err(Error::new(Trap::OutOfFuel))
        }
    }
}

fn read_raw(
    caller: &mut Caller<'_, HostState>,
    ptr: i32,
    len: i32,
    max: usize,
) -> Result<Vec<u8>, HostError> {
    let len = usize::try_from(len).map_err(|_| HostError::BadMemory)?;
    if len > max {
        return Err(HostError::TooLong);
    }
    let ptr = usize::try_from(ptr).map_err(|_| HostError::BadMemory)?;
    let end = ptr.checked_add(len).ok_or(HostError::BadMemory)?;
    let memory = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or(HostError::BadMemory)?;
    Ok(memory
        .data(&*caller)
        .get(ptr..end)
        .ok_or(HostError::BadMemory)?
        .to_vec())
}

fn read_str(
    caller: &mut Caller<'_, HostState>,
    ptr: i32,
    len: i32,
    max: usize,
) -> Result<String, HostError> {
    String::from_utf8(read_raw(caller, ptr, len, max)?).map_err(|_| HostError::BadUtf8)
}

fn id_to_i64(id: EntityId) -> i64 {
    i64::try_from(id.0).unwrap_or(i64::MAX)
}

fn lookup(state: &HostState, id: i64) -> Option<&Entity> {
    let id = EntityId(u64::try_from(id).ok()?);
    state.txn.entity(&state.world, id)
}

fn component<'a>(state: &'a HostState, id: i64, key: &str) -> Option<&'a Value> {
    lookup(state, id)?.components.get(key)
}

fn write_out(
    caller: &mut Caller<'_, HostState>,
    ptr: i32,
    cap: i32,
    bytes: &[u8],
) -> Result<i32, HostError> {
    let len = i32::try_from(bytes.len()).map_err(|_| HostError::TooLong)?;
    if len > cap {
        return Ok(len);
    }
    let ptr = usize::try_from(ptr).map_err(|_| HostError::BadMemory)?;
    let end = ptr.checked_add(bytes.len()).ok_or(HostError::BadMemory)?;
    let memory = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or(HostError::BadMemory)?;
    memory
        .data_mut(&mut *caller)
        .get_mut(ptr..end)
        .ok_or(HostError::BadMemory)?
        .copy_from_slice(bytes);
    Ok(len)
}

fn signal(state: &HostState, i: i32) -> Result<&Signal, HostError> {
    usize::try_from(i)
        .ok()
        .and_then(|i| state.signals.get(i))
        .ok_or(HostError::BadSignal(i))
}

fn cap(state: &HostState, handle: i32) -> Result<Cap, HostError> {
    usize::try_from(handle)
        .ok()
        .and_then(|i| state.clist.get(i).copied().flatten())
        .ok_or(HostError::BadHandle(handle))
}

fn writable(state: &mut HostState, handle: i32) -> Result<Cap, HostError> {
    let cap = cap(state, handle)?;
    if !cap.write {
        return Err(HostError::NoWrite(handle));
    }
    state.writes_left = state
        .writes_left
        .checked_sub(1)
        .ok_or(HostError::WriteBudget)?;
    Ok(cap)
}

fn set_value(state: &mut HostState, handle: i32, key: &str, value: Value) -> Result<(), HostError> {
    let cap = writable(state, handle)?;
    let HostState { txn, world, .. } = state;
    Ok(txn.set(world, cap.entity, key, value)?)
}

pub(crate) fn linker(engine: &Engine) -> wasmtime::Result<Linker<HostState>> {
    let mut l = Linker::new(engine);

    l.func_wrap(
        "world",
        "signal_find",
        |mut c: Caller<'_, HostState>,
         kp: i32,
         kl: i32,
         entity: i64,
         after: i32|
         -> wasmtime::Result<i32> {
            charge(&mut c, COST_READ)?;
            let name = deny(read_str(&mut c, kp, kl, MAX_KEY_LEN))?;
            let start = usize::try_from(after.saturating_add(1)).unwrap_or(0);
            let found = c
                .data()
                .signals
                .iter()
                .enumerate()
                .skip(start)
                .find(|(_, sig)| {
                    (name.is_empty() || sig.name == name)
                        && (entity < 0 || i64::try_from(sig.entity.0).ok() == Some(entity))
                })
                .map(|(i, _)| i);
            let total = c.data().signals.len();
            charge(
                &mut c,
                (found.map_or(total, |i| i + 1).saturating_sub(start)) as u64,
            )?;
            Ok(found.map_or(-1, |i| i32::try_from(i).unwrap_or(i32::MAX)))
        },
    )?;

    l.func_wrap(
        "world",
        "signal_entity",
        |mut c: Caller<'_, HostState>, i: i32| -> wasmtime::Result<i64> {
            charge(&mut c, COST_READ)?;
            Ok(id_to_i64(deny(signal(c.data(), i))?.entity))
        },
    )?;

    l.func_wrap(
        "world",
        "signal_value",
        |mut c: Caller<'_, HostState>, i: i32| -> wasmtime::Result<f64> {
            charge(&mut c, COST_READ)?;
            Ok(deny(signal(c.data(), i))?.value)
        },
    )?;

    l.func_wrap(
        "world",
        "signal_name",
        |mut c: Caller<'_, HostState>, i: i32, bp: i32, bl: i32| -> wasmtime::Result<i32> {
            charge(&mut c, COST_READ)?;
            let name = deny(signal(c.data(), i))?.name.clone();
            deny(write_out(&mut c, bp, bl, name.as_bytes()))
        },
    )?;

    l.func_wrap(
        "world",
        "signal_from",
        |mut c: Caller<'_, HostState>, i: i32, bp: i32, bl: i32| -> wasmtime::Result<i32> {
            charge(&mut c, COST_READ)?;
            let from = deny(signal(c.data(), i))?.from.label();
            deny(write_out(&mut c, bp, bl, from.as_bytes()))
        },
    )?;

    l.func_wrap(
        "world",
        "emit",
        |mut c: Caller<'_, HostState>,
         entity: i64,
         kp: i32,
         kl: i32,
         value: f64|
         -> wasmtime::Result<()> {
            charge(&mut c, COST_WRITE)?;
            let name = deny(read_str(&mut c, kp, kl, MAX_KEY_LEN))?;
            deny(aether_world::validate_key(&name).map_err(HostError::from))?;
            if !value.is_finite() {
                return deny(Err(HostError::World(WorldError::NonFinite)));
            }
            let s = c.data_mut();
            if lookup(s, entity).is_none() {
                return deny(Err(HostError::World(WorldError::NoSuchEntity(EntityId(
                    u64::try_from(entity).unwrap_or(u64::MAX),
                )))));
            }
            s.writes_left = deny(s.writes_left.checked_sub(1).ok_or(HostError::WriteBudget))?;
            let from = Sender::Capsule(s.capsule);
            s.emitted.push(Signal {
                from,
                entity: EntityId(u64::try_from(entity).expect("lookup accepted the id")),
                name,
                value,
            });
            Ok(())
        },
    )?;

    l.func_wrap("world", "self", |c: Caller<'_, HostState>| -> i64 {
        c.data().clist[0].map_or(-1, |cap| id_to_i64(cap.entity))
    })?;

    l.func_wrap(
        "world",
        "entity",
        |c: Caller<'_, HostState>, h: i32| -> wasmtime::Result<i64> {
            Ok(id_to_i64(deny(cap(c.data(), h))?.entity))
        },
    )?;

    l.func_wrap(
        "world",
        "exists",
        |mut c: Caller<'_, HostState>, id: i64| -> wasmtime::Result<i32> {
            charge(&mut c, COST_READ)?;
            Ok(i32::from(lookup(c.data(), id).is_some()))
        },
    )?;

    l.func_wrap(
        "world",
        "kind",
        |mut c: Caller<'_, HostState>, id: i64, kp: i32, kl: i32| -> wasmtime::Result<i32> {
            charge(&mut c, COST_READ)?;
            let key = deny(read_str(&mut c, kp, kl, MAX_KEY_LEN))?;
            Ok(match component(c.data(), id, &key) {
                None => 0,
                Some(Value::Num(_)) => 1,
                Some(Value::Text(_)) => 2,
                Some(Value::Bool(_)) => 3,
                Some(Value::Nums(_)) => 4,
            })
        },
    )?;

    l.func_wrap(
        "world",
        "get_num",
        |mut c: Caller<'_, HostState>, id: i64, kp: i32, kl: i32| -> wasmtime::Result<f64> {
            charge(&mut c, COST_READ)?;
            let key = deny(read_str(&mut c, kp, kl, MAX_KEY_LEN))?;
            match component(c.data(), id, &key) {
                Some(Value::Num(n)) => Ok(*n),
                _ => deny(Err(HostError::WrongKind(key, "number"))),
            }
        },
    )?;

    l.func_wrap(
        "world",
        "get_bool",
        |mut c: Caller<'_, HostState>, id: i64, kp: i32, kl: i32| -> wasmtime::Result<i32> {
            charge(&mut c, COST_READ)?;
            let key = deny(read_str(&mut c, kp, kl, MAX_KEY_LEN))?;
            match component(c.data(), id, &key) {
                Some(Value::Bool(b)) => Ok(i32::from(*b)),
                _ => deny(Err(HostError::WrongKind(key, "bool"))),
            }
        },
    )?;

    l.func_wrap(
        "world",
        "get_nums",
        |mut c: Caller<'_, HostState>,
         id: i64,
         kp: i32,
         kl: i32,
         dst: i32,
         max: i32|
         -> wasmtime::Result<i32> {
            charge(&mut c, COST_READ)?;
            let key = deny(read_str(&mut c, kp, kl, MAX_KEY_LEN))?;
            let nums = match component(c.data(), id, &key) {
                Some(Value::Nums(n)) => n.clone(),
                _ => return deny(Err(HostError::WrongKind(key, "number array"))),
            };
            charge(&mut c, nums.len() as u64)?;
            let count = i32::try_from(nums.len()).map_err(|_| Error::new(HostError::TooLong))?;
            if count > max {
                return Ok(count);
            }
            let bytes: Vec<u8> = nums.iter().flat_map(|n| n.to_le_bytes()).collect();
            deny(write_out(&mut c, dst, max.saturating_mul(8), &bytes))?;
            Ok(count)
        },
    )?;

    l.func_wrap(
        "world",
        "get_text",
        |mut c: Caller<'_, HostState>,
         id: i64,
         kp: i32,
         kl: i32,
         bp: i32,
         bl: i32|
         -> wasmtime::Result<i32> {
            charge(&mut c, COST_READ)?;
            let key = deny(read_str(&mut c, kp, kl, MAX_KEY_LEN))?;
            let text = match component(c.data(), id, &key) {
                Some(Value::Text(t)) => t.clone(),
                _ => return deny(Err(HostError::WrongKind(key, "text"))),
            };
            charge(&mut c, text.len() as u64)?;
            deny(write_out(&mut c, bp, bl, text.as_bytes()))
        },
    )?;

    l.func_wrap(
        "world",
        "owner",
        |mut c: Caller<'_, HostState>, id: i64, bp: i32, bl: i32| -> wasmtime::Result<i32> {
            charge(&mut c, COST_READ)?;
            let Some(entity) = lookup(c.data(), id) else {
                return Ok(-1);
            };
            let owner = match &entity.owner {
                Owner::Region => "region".to_owned(),
                Owner::Player(p) => format!("player:{}", p.0),
                Owner::Capsule(id) => format!("capsule:{}", id.0),
            };
            deny(write_out(&mut c, bp, bl, owner.as_bytes()))
        },
    )?;

    l.func_wrap(
        "world",
        "installer",
        |mut c: Caller<'_, HostState>, capsule: i64, bp: i32, bl: i32| -> wasmtime::Result<i32> {
            charge(&mut c, COST_READ)?;
            let Some(label) = u64::try_from(capsule)
                .ok()
                .and_then(|id| c.data().installers.get(&CapsuleId(id)).cloned())
            else {
                return Ok(-1);
            };
            deny(write_out(&mut c, bp, bl, label.as_bytes()))
        },
    )?;

    let unary: [(&str, Unary); 4] = [
        ("sin", libm::sin),
        ("cos", libm::cos),
        ("exp", libm::exp),
        ("ln", libm::log),
    ];
    for (name, f) in unary {
        l.func_wrap(
            "world",
            name,
            move |mut c: Caller<'_, HostState>, x: f64| -> wasmtime::Result<f64> {
                charge(&mut c, COST_MATH)?;
                Ok(f(x))
            },
        )?;
    }
    let binary: [(&str, Binary); 2] = [("atan2", libm::atan2), ("pow", libm::pow)];
    for (name, f) in binary {
        l.func_wrap(
            "world",
            name,
            move |mut c: Caller<'_, HostState>, a: f64, b: f64| -> wasmtime::Result<f64> {
                charge(&mut c, COST_MATH)?;
                Ok(f(a, b))
            },
        )?;
    }

    l.func_wrap(
        "world",
        "find",
        |mut c: Caller<'_, HostState>, kp: i32, kl: i32, after: i64| -> wasmtime::Result<i64> {
            charge(&mut c, COST_READ)?;
            let key = deny(read_str(&mut c, kp, kl, MAX_KEY_LEN))?;
            let after = u64::try_from(after).ok().map(EntityId);
            let s = c.data();
            let (found, examined) = s.txn.find(&s.world, &key, after);
            charge(&mut c, examined)?;
            Ok(found.map_or(-1, id_to_i64))
        },
    )?;

    l.func_wrap(
        "world",
        "near",
        |mut c: Caller<'_, HostState>,
         dim: i64,
         x: f64,
         y: f64,
         r: f64,
         after: i64|
         -> wasmtime::Result<i64> {
            charge(&mut c, COST_READ)?;
            let after = u64::try_from(after).ok().map(EntityId);
            let s = c.data();
            let (found, examined) = s.txn.near(&s.world, dim, x, y, r, after);
            charge(&mut c, examined)?;
            Ok(found.map_or(-1, id_to_i64))
        },
    )?;

    l.func_wrap(
        "world",
        "set_num",
        |mut c: Caller<'_, HostState>, h: i32, kp: i32, kl: i32, v: f64| -> wasmtime::Result<()> {
            charge(&mut c, COST_WRITE)?;
            let key = deny(read_str(&mut c, kp, kl, MAX_KEY_LEN))?;
            deny(set_value(c.data_mut(), h, &key, Value::Num(v)))
        },
    )?;

    l.func_wrap(
        "world",
        "set_text",
        |mut c: Caller<'_, HostState>,
         h: i32,
         kp: i32,
         kl: i32,
         vp: i32,
         vl: i32|
         -> wasmtime::Result<()> {
            charge(&mut c, COST_WRITE + u64::try_from(vl.max(0)).unwrap_or(0))?;
            let key = deny(read_str(&mut c, kp, kl, MAX_KEY_LEN))?;
            let text = deny(read_str(&mut c, vp, vl, MAX_TEXT_LEN))?;
            deny(set_value(c.data_mut(), h, &key, Value::Text(text)))
        },
    )?;

    l.func_wrap(
        "world",
        "set_nums",
        |mut c: Caller<'_, HostState>,
         h: i32,
         kp: i32,
         kl: i32,
         src: i32,
         count: i32|
         -> wasmtime::Result<()> {
            charge(
                &mut c,
                COST_WRITE + u64::try_from(count.max(0)).unwrap_or(0),
            )?;
            let key = deny(read_str(&mut c, kp, kl, MAX_KEY_LEN))?;
            let len = count.checked_mul(8).ok_or(Error::new(HostError::TooLong))?;
            let bytes = deny(read_raw(&mut c, src, len, MAX_NUMS * 8))?;
            let nums: Arc<[f64]> = bytes
                .as_chunks::<8>()
                .0
                .iter()
                .map(|b| f64::from_le_bytes(*b))
                .collect();
            deny(set_value(c.data_mut(), h, &key, Value::Nums(nums)))
        },
    )?;

    l.func_wrap(
        "world",
        "set_bool",
        |mut c: Caller<'_, HostState>, h: i32, kp: i32, kl: i32, v: i32| -> wasmtime::Result<()> {
            charge(&mut c, COST_WRITE)?;
            let key = deny(read_str(&mut c, kp, kl, MAX_KEY_LEN))?;
            deny(set_value(c.data_mut(), h, &key, Value::Bool(v != 0)))
        },
    )?;

    l.func_wrap(
        "world",
        "remove",
        |mut c: Caller<'_, HostState>, h: i32, kp: i32, kl: i32| -> wasmtime::Result<i32> {
            charge(&mut c, COST_WRITE)?;
            let key = deny(read_str(&mut c, kp, kl, MAX_KEY_LEN))?;
            let s = c.data_mut();
            let cap = deny(writable(s, h))?;
            let HostState { txn, world, .. } = s;
            let removed = deny(txn.remove(world, cap.entity, &key).map_err(HostError::from))?;
            Ok(i32::from(removed))
        },
    )?;

    l.func_wrap(
        "world",
        "spawn",
        |mut c: Caller<'_, HostState>| -> wasmtime::Result<i32> {
            charge(&mut c, COST_SPAWN)?;
            let s = c.data_mut();
            deny(spawn(s))
        },
    )?;

    l.func_wrap(
        "world",
        "despawn",
        |mut c: Caller<'_, HostState>, h: i32| -> wasmtime::Result<()> {
            charge(&mut c, COST_DESPAWN)?;
            deny(despawn(c.data_mut(), h))
        },
    )?;

    l.func_wrap("world", "tick", |c: Caller<'_, HostState>| -> i64 {
        i64::try_from(c.data().world.tick()).unwrap_or(i64::MAX)
    })?;

    l.func_wrap("world", "random", |mut c: Caller<'_, HostState>| -> i64 {
        splitmix64(&mut c.data_mut().rng) as i64
    })?;

    l.func_wrap(
        "world",
        "log",
        |mut c: Caller<'_, HostState>, p: i32, n: i32| -> wasmtime::Result<()> {
            charge(&mut c, COST_LOG + u64::try_from(n.max(0)).unwrap_or(0))?;
            let max = c.data().log_bytes_left;
            let line = deny(read_str(&mut c, p, n, max))?;
            let s = c.data_mut();
            s.log_bytes_left -= line.len();
            s.log.push(line);
            Ok(())
        },
    )?;

    Ok(l)
}

fn spawn(s: &mut HostState) -> Result<i32, HostError> {
    s.spawns_left = s.spawns_left.checked_sub(1).ok_or(HostError::SpawnBudget)?;
    if s.clist.len() >= s.max_clist_len && !s.clist.iter().skip(1).any(Option::is_none) {
        return Err(HostError::ClistFull);
    }
    if s.txn.live_count(&s.world) >= s.max_entities {
        return Err(HostError::RegionFull);
    }
    let entity = s.txn.spawn(Owner::Capsule(s.capsule));
    let cap = Cap {
        entity,
        write: true,
    };
    let handle = claim_slot(&mut s.clist, cap, s.max_clist_len).ok_or(HostError::ClistFull)?;
    i32::try_from(handle).map_err(|_| HostError::ClistFull)
}

fn despawn(s: &mut HostState, h: i32) -> Result<(), HostError> {
    if h == 0 {
        return Err(HostError::SelfDespawn);
    }
    let cap = writable(s, h)?;
    let owner = &s
        .txn
        .entity(&s.world, cap.entity)
        .ok_or(WorldError::NoSuchEntity(cap.entity))?
        .owner;
    if *owner != Owner::Capsule(s.capsule) {
        return Err(HostError::NotOwner(h));
    }
    let HostState { txn, world, .. } = s;
    txn.despawn(world, cap.entity)?;
    if let Some(slot) = usize::try_from(h).ok().and_then(|i| s.clist.get_mut(i)) {
        *slot = None;
    }
    Ok(())
}
