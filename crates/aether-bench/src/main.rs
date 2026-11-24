use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use aether_kernel::{Action, Actor, BlobHash, Grant, Input, Kernel, Law};
use aether_store::Store;
use aether_world::{PlayerId, Value};

const BOUNCE: &str = include_str!("../../../capsules/bounce.wat");
const BLACKHOLE: &str = include_str!("../../../capsules/blackhole.wat");
const REALM: &str = include_str!("../../../capsules/realm.wat");
const RIFLE: &str = include_str!("../../../capsules/rifle.wat");
const HOUSE: &str = include_str!("../../../capsules/house.wat");
const SCAN: &str = r#"(module
  (import "world" "find" (func $find (param i32 i32 i64) (result i64)))
  (import "world" "get_num" (func $get (param i64 i32 i32) (result f64)))
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "hp")
  (data (i32.const 8) "total")
  (func (export "tick") (local $id i64) (local $sum f64)
    (local.set $id (i64.const -1))
    (block $done
      (loop $next
        (local.set $id (call $find (i32.const 0) (i32.const 2) (local.get $id)))
        (br_if $done (i64.lt_s (local.get $id) (i64.const 0)))
        (local.set $sum (f64.add (local.get $sum) (call $get (local.get $id) (i32.const 0) (i32.const 2))))
        (br $next)))
    (call $set (i32.const 0) (i32.const 8) (i32.const 5) (local.get $sum))))"#;

fn law() -> Law {
    Law {
        max_entities: 100_000,
        max_inputs_per_actor: 100_000,
        ..Law::default()
    }
}

fn spawn(n: usize) -> Vec<Input> {
    (0..n)
        .map(|i| Input {
            actor: Actor::Admin,
            action: Action::Spawn {
                components: BTreeMap::from([
                    ("hp".to_owned(), Value::Num(i as f64)),
                    ("pos.x".to_owned(), Value::Num(0.0)),
                ]),
            },
        })
        .collect()
}

fn install(wasm: BlobHash, n: usize) -> Vec<Input> {
    (0..n)
        .map(|_| Input {
            actor: Actor::Admin,
            action: Action::Install {
                wasm,
                grants: Vec::new(),
                budget: Law::default().ceiling,
            },
        })
        .collect()
}

fn per_tick(mut tick: impl FnMut(), ticks: u32) -> Duration {
    tick();
    let start = Instant::now();
    for _ in 0..ticks {
        tick();
    }
    start.elapsed() / ticks
}

fn row(name: &str, d: Duration, note: String) {
    println!("{name:<44} {:>10.3} ms   {note}", d.as_secs_f64() * 1e3);
}

fn main() {
    println!("{:<44} {:>13}", "scenario", "per tick");
    for entities in [1_000, 10_000, 50_000] {
        let mut k = Kernel::new(law(), 1);
        k.step(spawn(entities));
        row(
            &format!("idle region, {entities} entities"),
            per_tick(|| drop(k.step(Vec::new())), 50),
            String::new(),
        );
        let start = Instant::now();
        let report = k.step(
            (0..entities as u64)
                .map(|i| Input {
                    actor: Actor::Admin,
                    action: Action::Set {
                        entity: aether_world::EntityId(i),
                        key: "pos.x".into(),
                        value: Value::Num(1.0),
                    },
                })
                .collect(),
        );
        row(
            &format!("one tick writing all {entities} entities"),
            start.elapsed(),
            format!("{} touched", report.touched.len()),
        );
    }

    for capsules in [16, 64, 256] {
        let mut k = Kernel::new(law(), 1);
        let wasm = k.add_blob(BOUNCE.as_bytes().to_vec());
        k.step(spawn(10_000));
        k.step(install(wasm, capsules));
        row(
            &format!("{capsules} bounce capsules, 10000 entities"),
            per_tick(|| drop(k.step(Vec::new())), 20),
            String::new(),
        );
    }

    for entities in [1_000, 10_000] {
        let mut k = Kernel::new(law(), 1);
        let wasm = k.add_blob(SCAN.as_bytes().to_vec());
        k.step(spawn(entities));
        k.step(install(wasm, 1));
        let mut fuel = 0;
        let d = per_tick(
            || {
                let r = k.step(Vec::new());
                fuel = r.runs[0].fuel_used;
                assert!(r.runs[0].fault.is_none(), "{:?}", r.runs[0].fault);
            },
            20,
        );
        row(
            &format!("one capsule summing hp over {entities}"),
            d,
            format!("{fuel} fuel"),
        );
    }

    {
        let mut law = law();
        law.ceiling.fuel_per_tick = 8_000_000;
        let mut k = Kernel::new(law.clone(), 1);
        let wasm = k.add_blob(BLACKHOLE.as_bytes().to_vec());
        k.step(vec![Input {
            actor: Actor::Admin,
            action: Action::Install {
                wasm,
                grants: Vec::new(),
                budget: law.ceiling,
            },
        }]);
        let mut fuel = 0;
        let mut json = 0;
        let d = per_tick(
            || {
                let r = k.step(Vec::new());
                assert!(r.runs[0].fault.is_none(), "{:?}", r.runs[0].fault);
                fuel = r.runs[0].fuel_used;
                json = r
                    .touched
                    .iter()
                    .map(|id| serde_json::to_vec(&k.world().entity(*id)).map_or(0, |v| v.len()))
                    .sum();
            },
            20,
        );
        row(
            "black hole, 50000 particles",
            d,
            format!("{fuel} fuel, {} KiB of JSON changes", json / 1024),
        );
    }

    for players in [8, 32] {
        battle(players);
    }

    let dir = std::env::temp_dir().join(format!("aether-bench-{}", std::process::id()));
    let mut store = Store::create(PathBuf::from(&dir), law(), 1).expect("store");
    store.step(spawn(10_000)).expect("spawn");
    row(
        "durable empty tick (journal fsync)",
        per_tick(|| drop(store.step(Vec::new()).expect("step")), 50),
        String::new(),
    );
    let start = Instant::now();
    store.checkpoint().expect("checkpoint");
    row(
        "checkpoint with 10000 entities",
        start.elapsed(),
        format!(
            "{} KiB snapshot",
            std::fs::metadata(dir.join("snapshot.json")).map_or(0, |m| m.len() / 1024)
        ),
    );
    let start = Instant::now();
    drop(store);
    Store::open(&dir).expect("reopen");
    row("reopen with 10000 entities", start.elapsed(), String::new());
    std::fs::remove_dir_all(&dir).expect("bench dir is removable");
}

fn battle(players: usize) {
    let law = law();
    let mut k = Kernel::new(law.clone(), 1);
    let realm = k.add_blob(REALM.as_bytes().to_vec());
    let rifle = k.add_blob(RIFLE.as_bytes().to_vec());
    let house = k.add_blob(HOUSE.as_bytes().to_vec());
    k.step(vec![Input {
        actor: Actor::Admin,
        action: Action::Install {
            wasm: realm,
            grants: Vec::new(),
            budget: law.ceiling,
        },
    }]);
    k.step(install(house, 1));
    let hub = k
        .state()
        .capsules
        .values()
        .next()
        .expect("realm")
        .self_entity;
    let who = |i: usize| Actor::Player(PlayerId(format!("p{i}")));
    let signal = |i: usize, entity, name: &str, value: f64| Input {
        actor: who(i),
        action: Action::Signal {
            entity,
            name: name.into(),
            value,
        },
    };
    for batch in (0..players).collect::<Vec<_>>().chunks(6) {
        k.step(batch.iter().map(|&i| signal(i, hub, "join", 0.0)).collect());
    }
    let mut avatars = Vec::new();
    for i in 0..players {
        let label = Value::Text(format!("player:p{i}"));
        let id = k
            .world()
            .entities()
            .find(|(_, e)| e.components.get("player") == Some(&label))
            .map(|(id, _)| id)
            .expect("the realm made a body");
        let a = i as f64 / players as f64 * std::f64::consts::TAU;
        k.step(vec![
            Input {
                actor: Actor::Admin,
                action: Action::Set {
                    entity: id,
                    key: "pos.x".into(),
                    value: Value::Num(12.0 * a.cos()),
                },
            },
            Input {
                actor: Actor::Admin,
                action: Action::Set {
                    entity: id,
                    key: "pos.y".into(),
                    value: Value::Num(12.0 * a.sin()),
                },
            },
            signal(i, id, "ax", -a.cos()),
            signal(i, id, "ay", -a.sin()),
            Input {
                actor: who(i),
                action: Action::Install {
                    wasm: rifle,
                    grants: vec![Grant {
                        entity: id,
                        write: false,
                    }],
                    budget: Law::default().ceiling,
                },
            },
        ]);
        avatars.push(id);
    }
    let mut t = 0u64;
    let mut fuel = 0;
    let mut realm_fuel = 0;
    let mut bullets = 0;
    let d = per_tick(
        || {
            t += 1;
            let inputs = avatars
                .iter()
                .enumerate()
                .flat_map(|(i, id)| {
                    let strafe = if (t / 5 + i as u64).is_multiple_of(2) {
                        1.0
                    } else {
                        -1.0
                    };
                    [
                        signal(i, *id, "fire", 1.0),
                        signal(i, *id, "mx", strafe * 0.5),
                        signal(i, *id, "my", strafe * 0.5),
                    ]
                })
                .collect();
            let r = k.step(inputs);
            for run in &r.runs {
                assert!(
                    run.fault.is_none(),
                    "capsule {:?}: {:?}",
                    run.capsule,
                    run.fault
                );
            }
            fuel = r.runs.iter().map(|x| x.fuel_used).sum();
            realm_fuel = r.runs[0].fuel_used;
            bullets = k
                .world()
                .entities()
                .filter(|(_, e)| e.components.contains_key("projectile"))
                .count();
        },
        50,
    );
    row(
        &format!("battle: {players} players moving and firing + realm"),
        d,
        format!("{fuel} fuel ({realm_fuel} realm), {bullets} bullets in flight"),
    );
}
