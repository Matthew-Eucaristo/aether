use std::collections::BTreeSet;
use std::fmt::Write;

use aether_kernel::{Action, Actor, Fault, Grant, Input, InputOutcome, Kernel};
use aether_world::{CapsuleId, Entity, EntityId, Owner, Value};
use serde::Deserialize;
use serde_json::{Value as Json, json};

use crate::model::ToolSpec;

const MAX_TRIAL_TICKS: u32 = 100;
const LISTED_ENTITIES: usize = 60;

fn grants_schema() -> Json {
    json!({
        "type": "array",
        "description": "Capabilities for the capsule. Each becomes the next handle after 0, in order. Only write grants matter; reading is public.",
        "items": {
            "type": "object",
            "additionalProperties": false,
            "required": ["entity", "write"],
            "properties": {
                "entity": {"type": "integer", "description": "Entity id"},
                "write": {"type": "boolean"}
            }
        }
    })
}

pub fn specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "look",
            description: "List entities in the real world with their owner and components. Filter by a component key and/or an owner; empty strings mean no filter. Owner is written as region, player:<name> or capsule:<id>.",
            schema: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["key", "owner"],
                "properties": {
                    "key": {"type": "string"},
                    "owner": {"type": "string"}
                }
            }),
        },
        ToolSpec {
            name: "read_capsule",
            description: "Show an installed capsule: who installed it, its status, its capability list and its source code.",
            schema: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["capsule"],
                "properties": {"capsule": {"type": "integer"}}
            }),
        },
        ToolSpec {
            name: "trial",
            description: "Run a module on a private copy of the world without changing the real one. With capsule = -1 it installs the module as a new capsule with the given grants; with an existing capsule id it upgrades that capsule (grants are ignored). Then it runs the given number of ticks (at most 100) and reports install errors, faults, fuel, logs, and the entities the capsule created or changed with their components and motion. install and upgrade accept only a module that passed a clean trial.",
            schema: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["wat", "capsule", "grants", "ticks"],
                "properties": {
                    "wat": {"type": "string", "description": "Complete WAT module"},
                    "capsule": {"type": "integer"},
                    "grants": grants_schema(),
                    "ticks": {"type": "integer"}
                }
            }),
        },
        ToolSpec {
            name: "install",
            description: "Install a module that passed a clean trial as a new capsule in the real world, acting for the person. Returns the new capsule id.",
            schema: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["wat", "grants"],
                "properties": {
                    "wat": {"type": "string"},
                    "grants": grants_schema()
                }
            }),
        },
        ToolSpec {
            name: "upgrade",
            description: "Replace the code of an existing capsule in the real world with a module that passed a clean upgrade trial for that capsule. The capsule keeps its entities and capabilities.",
            schema: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["capsule", "wat"],
                "properties": {
                    "capsule": {"type": "integer"},
                    "wat": {"type": "string"}
                }
            }),
        },
    ]
}

#[derive(Deserialize)]
pub struct GrantIn {
    entity: u64,
    write: bool,
}

pub fn grants(input: &[GrantIn]) -> Vec<Grant> {
    input
        .iter()
        .map(|g| Grant {
            entity: EntityId(g.entity),
            write: g.write,
        })
        .collect()
}

#[derive(Deserialize)]
pub struct LookIn {
    pub key: String,
    pub owner: String,
}

#[derive(Deserialize)]
pub struct ReadIn {
    pub capsule: u64,
}

#[derive(Deserialize)]
pub struct TrialIn {
    pub wat: String,
    pub capsule: i64,
    pub grants: Vec<GrantIn>,
    pub ticks: u32,
}

#[derive(Deserialize)]
pub struct InstallIn {
    pub wat: String,
    pub grants: Vec<GrantIn>,
}

#[derive(Deserialize)]
pub struct UpgradeIn {
    pub capsule: u64,
    pub wat: String,
}

pub fn owner_label(owner: &Owner) -> String {
    match owner {
        Owner::Region => "region".into(),
        Owner::Player(p) => format!("player:{}", p.0),
        Owner::Capsule(c) => format!("capsule:{}", c.0),
    }
}

pub fn actor_label(actor: &Actor) -> String {
    match actor {
        Actor::Admin => "admin".into(),
        Actor::Player(p) => format!("player:{}", p.0),
    }
}

fn value(v: &Value) -> String {
    match v {
        Value::Num(n) => format!("{}", (n * 1000.0).round() / 1000.0),
        Value::Text(t) => format!("{t:?}"),
        Value::Bool(b) => b.to_string(),
        Value::Nums(n) => format!(
            "[{} numbers: {}{}]",
            n.len(),
            n.iter()
                .take(6)
                .map(|x| format!("{}", (x * 1000.0).round() / 1000.0))
                .collect::<Vec<_>>()
                .join(", "),
            if n.len() > 6 { ", …" } else { "" }
        ),
    }
}

fn entity_line(id: EntityId, e: &Entity) -> String {
    let mut line = format!("#{} owner={}", id.0, owner_label(&e.owner));
    for (k, v) in &e.components {
        let _ = write!(line, " {k}={}", value(v));
    }
    line
}

fn position(e: &Entity) -> Option<[f64; 3]> {
    let get = |k: &str| match e.components.get(k) {
        Some(Value::Num(n)) => Some(*n),
        _ => None,
    };
    Some([
        get("pos.x")?,
        get("pos.y").unwrap_or(0.0),
        get("pos.z").unwrap_or(0.0),
    ])
}

pub fn context(k: &Kernel, actor: &Actor) -> String {
    let state = k.state();
    let law = &state.law;
    let c = &law.ceiling;
    let mut out = String::new();
    let _ = writeln!(out, "# Context");
    let _ = writeln!(out, "You act for {}.", actor_label(actor));
    let _ = writeln!(
        out,
        "Tick {}. Per capsule per tick the law allows {} fuel, {} writes, {} spawns and {} bytes of memory{}.",
        state.world.tick(),
        c.fuel_per_tick,
        c.writes_per_tick,
        c.spawns_per_tick,
        c.memory_bytes,
        if law.allow_spawn {
            ""
        } else {
            " (spawning is disabled)"
        }
    );
    if let Actor::Player(p) = actor {
        let used = k.player_usage(p);
        let _ = writeln!(
            out,
            "This player holds {} of {} entities and {} of {} bytes, counting their capsules.",
            used.entities, law.player_quota.entities, used.bytes, law.player_quota.bytes
        );
    }
    let _ = writeln!(out, "\n## Capsules");
    if state.capsules.is_empty() {
        let _ = writeln!(out, "none");
    }
    for (id, cap) in &state.capsules {
        let _ = writeln!(
            out,
            "capsule {} installed by {}, {:?}, own entity #{}, {} handles",
            id.0,
            actor_label(&cap.installer),
            cap.status,
            cap.self_entity.0,
            cap.clist.len()
        );
    }
    let _ = writeln!(out, "\n## Entities ({} in total)", state.world.len());
    for (id, e) in state.world.entities().take(LISTED_ENTITIES) {
        let _ = writeln!(out, "{}", entity_line(id, e));
    }
    if state.world.len() > LISTED_ENTITIES {
        let _ = writeln!(out, "… use look to see the rest");
    }
    out
}

pub fn look(k: &Kernel, input: &LookIn) -> String {
    let mut out = String::new();
    let mut shown = 0;
    let mut matched = 0;
    for (id, e) in k.world().entities() {
        if !input.key.is_empty() && !e.components.contains_key(&input.key) {
            continue;
        }
        if !input.owner.is_empty() && owner_label(&e.owner) != input.owner {
            continue;
        }
        matched += 1;
        if shown < LISTED_ENTITIES {
            let _ = writeln!(out, "{}", entity_line(id, e));
            shown += 1;
        }
    }
    let _ = writeln!(out, "{matched} matching entities, {shown} shown");
    out
}

pub fn read_capsule(k: &Kernel, id: CapsuleId) -> Result<String, String> {
    let cap = k
        .state()
        .capsules
        .get(&id)
        .ok_or_else(|| format!("capsule {} does not exist", id.0))?;
    let mut out = format!(
        "capsule {} installed by {}, {:?}, {} faults in a row, budget {} fuel per tick\n",
        id.0,
        actor_label(&cap.installer),
        cap.status,
        cap.consecutive_faults,
        cap.budget.fuel_per_tick
    );
    for (h, slot) in cap.clist.iter().enumerate() {
        match slot {
            Some(c) => {
                let _ = writeln!(
                    out,
                    "handle {h}: entity #{} {}",
                    c.entity.0,
                    if h == 0 {
                        "(own)"
                    } else if c.write {
                        "write"
                    } else {
                        "read"
                    }
                );
            }
            None => {
                let _ = writeln!(out, "handle {h}: revoked");
            }
        }
    }
    let source = k
        .blobs()
        .get(&cap.wasm)
        .and_then(|b| std::str::from_utf8(b).ok().map(str::to_owned))
        .unwrap_or_else(|| "<binary module>".into());
    let _ = write!(out, "source:\n{source}");
    Ok(out)
}

pub struct Trial {
    pub report: String,
    pub clean: bool,
}

pub fn trial(k: &Kernel, actor: &Actor, input: &TrialIn) -> Trial {
    let mut fork = match Kernel::from_state(k.state().clone(), k.blobs().clone()) {
        Ok(fork) => fork,
        Err(e) => {
            return Trial {
                report: format!("could not copy the world: {e}"),
                clean: false,
            };
        }
    };
    let wasm = fork.add_blob(input.wat.clone().into_bytes());
    let action = if input.capsule < 0 {
        Action::Install {
            wasm,
            grants: grants(&input.grants),
            budget: fork.state().law.ceiling,
        }
    } else {
        Action::Upgrade {
            capsule: CapsuleId(input.capsule as u64),
            wasm,
        }
    };
    let first = fork.step(vec![Input {
        actor: actor.clone(),
        action,
    }]);
    let id = match &first.inputs[0] {
        InputOutcome::Installed(id) => *id,
        InputOutcome::Applied => CapsuleId(input.capsule as u64),
        InputOutcome::Rejected(reason) => {
            return Trial {
                report: format!("rejected before running: {reason}"),
                clean: false,
            };
        }
        other => {
            return Trial {
                report: format!("unexpected outcome {other:?}"),
                clean: false,
            };
        }
    };
    let owned = |k: &Kernel| -> Vec<(EntityId, Entity)> {
        k.world()
            .entities()
            .filter(|(_, e)| e.owner == Owner::Capsule(id))
            .map(|(i, e)| (i, e.clone()))
            .collect()
    };
    let start = owned(&fork);
    let mut reports = vec![first];
    let ticks = input.ticks.clamp(1, MAX_TRIAL_TICKS);
    for _ in 1..ticks {
        let r = fork.step(Vec::new());
        let stop = r.runs.iter().any(|run| run.capsule == id && run.suspended);
        reports.push(r);
        if stop {
            break;
        }
    }
    let end = owned(&fork);

    let mut out = String::new();
    let mut faults = Vec::new();
    let mut fuel = Vec::new();
    let mut logs = Vec::new();
    let mut touched = BTreeSet::new();
    for r in &reports {
        touched.extend(r.touched.iter().copied());
        if let Some(run) = r.runs.iter().find(|run| run.capsule == id) {
            fuel.push(run.fuel_used);
            logs.extend(run.log.iter().map(|l| format!("tick {}: {l}", r.tick)));
            if let Some(f) = &run.fault {
                let text = match f {
                    Fault::OutOfFuel => "out of fuel".to_owned(),
                    Fault::DepthLimit => "call depth limit".to_owned(),
                    Fault::Denied(d) => format!("denied: {d}"),
                    Fault::Trap(t) => format!("trap: {t}"),
                };
                faults.push(format!(
                    "tick {}: {text}{}",
                    r.tick,
                    if run.suspended { " -> suspended" } else { "" }
                ));
            }
        }
    }
    let _ = writeln!(
        out,
        "{} capsule {} on a private copy of the world and ran {} ticks.",
        if input.capsule < 0 {
            "Installed"
        } else {
            "Upgraded"
        },
        id.0,
        reports.len()
    );
    if faults.is_empty() {
        let _ = writeln!(out, "No faults.");
    } else {
        let _ = writeln!(
            out,
            "{} faulted ticks; their writes were discarded:",
            faults.len()
        );
        for f in faults.iter().take(8) {
            let _ = writeln!(out, "  {f}");
        }
    }
    if let (Some(min), Some(max)) = (fuel.iter().min(), fuel.iter().max()) {
        let _ = writeln!(
            out,
            "Fuel per tick: {min} to {max} of {}.",
            k.state().law.ceiling.fuel_per_tick
        );
    }
    if !logs.is_empty() {
        let _ = writeln!(out, "Log:");
        for l in logs.iter().take(20) {
            let _ = writeln!(out, "  {l}");
        }
    }
    let _ = writeln!(out, "Entities the capsule owns at the end ({}):", end.len());
    for (eid, e) in end.iter().take(30) {
        let before = start
            .iter()
            .find(|(s, _)| s == eid)
            .and_then(|(_, e)| position(e));
        let motion = match (before, position(e)) {
            (Some(a), Some(b)) => format!(
                "   moved ({:.2}, {:.2}, {:.2}) -> ({:.2}, {:.2}, {:.2})",
                a[0], a[1], a[2], b[0], b[1], b[2]
            ),
            _ => String::new(),
        };
        let _ = writeln!(out, "  {}{motion}", entity_line(*eid, e));
    }
    let others: Vec<String> = touched
        .iter()
        .filter(|t| !end.iter().any(|(e, _)| e == *t) && !start.iter().any(|(e, _)| e == *t))
        .filter_map(|t| fork.world().entity(*t).map(|e| entity_line(*t, e)))
        .take(20)
        .collect();
    if !others.is_empty() {
        let _ = writeln!(out, "Other entities changed during the trial:");
        for o in others {
            let _ = writeln!(out, "  {o}");
        }
    }
    Trial {
        report: out,
        clean: faults.is_empty(),
    }
}
