use std::collections::BTreeMap;

use aether_kernel::{Action, Actor, Grant, Input, InputOutcome, Kernel, Law, TickReport};
use aether_world::{CapsuleId, EntityId, PlayerId, Value};

const REALM: &str = include_str!("../../../capsules/realm.wat");
const RIFLE: &str = include_str!("../../../capsules/rifle.wat");
const LAUNCHER: &str = include_str!("../../../capsules/launcher.wat");

const HEALTH: usize = 3;
const KILLS: usize = 4;
const DEATHS: usize = 5;
const DEAD: usize = 6;
const X: usize = 7;

fn player(name: &str) -> Actor {
    Actor::Player(PlayerId(name.into()))
}

fn step(k: &mut Kernel, inputs: Vec<(Actor, Action)>) -> TickReport {
    let r = k.step(
        inputs
            .into_iter()
            .map(|(actor, action)| Input { actor, action })
            .collect(),
    );
    for run in &r.runs {
        assert_eq!(run.fault, None, "tick {} capsule {:?}", r.tick, run.capsule);
    }
    r
}

fn raw(k: &mut Kernel, inputs: Vec<(Actor, Action)>) -> TickReport {
    k.step(
        inputs
            .into_iter()
            .map(|(actor, action)| Input { actor, action })
            .collect(),
    )
}

fn install(k: &mut Kernel, actor: Actor, src: &str, grants: Vec<Grant>) -> CapsuleId {
    let wasm = k.add_blob(src.as_bytes().to_vec());
    let budget = Law::default().ceiling;
    match &step(
        k,
        vec![(
            actor,
            Action::Install {
                wasm,
                grants,
                budget,
            },
        )],
    )
    .inputs[0]
    {
        InputOutcome::Installed(id) => *id,
        other => panic!("install failed: {other:?}"),
    }
}

fn spawn(k: &mut Kernel, components: &[(&str, Value)]) -> EntityId {
    let components: BTreeMap<String, Value> = components
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect();
    match &step(k, vec![(Actor::Admin, Action::Spawn { components })]).inputs[0] {
        InputOutcome::Spawned(id) => *id,
        other => panic!("spawn failed: {other:?}"),
    }
}

fn signal(entity: EntityId, name: &str, value: f64) -> Action {
    Action::Signal {
        entity,
        name: name.into(),
        value,
    }
}

fn set(entity: EntityId, key: &str, value: f64) -> Action {
    Action::Set {
        entity,
        key: key.into(),
        value: Value::Num(value),
    }
}

fn roster(k: &Kernel, realm: CapsuleId) -> Vec<[f64; 20]> {
    let me = k.state().capsules[&realm].self_entity;
    match k.world().entity(me).unwrap().components.get("roster") {
        Some(Value::Nums(n)) => n
            .chunks(20)
            .map(|c| c.try_into().expect("twenty numbers per player"))
            .collect(),
        _ => Vec::new(),
    }
}

fn entry(k: &Kernel, realm: CapsuleId, avatar: EntityId) -> [f64; 20] {
    roster(k, realm)
        .into_iter()
        .find(|e| e[0] == avatar.0 as f64)
        .expect("avatar is in the realm")
}

fn body_of(k: &Kernel, name: &str) -> Option<EntityId> {
    let label = Value::Text(format!("player:{name}"));
    k.world()
        .entities()
        .find(|(_, e)| e.components.get("player") == Some(&label))
        .map(|(id, _)| id)
}

fn avatar_of(k: &Kernel, name: &str) -> EntityId {
    body_of(k, name).expect("the realm made a body")
}

fn num(k: &Kernel, id: EntityId, key: &str) -> f64 {
    match k.world().entity(id).unwrap().components.get(key) {
        Some(Value::Num(n)) => *n,
        other => panic!("{key}: {other:?}"),
    }
}

fn projectiles(k: &Kernel) -> usize {
    k.world()
        .entities()
        .filter(|(_, e)| e.components.contains_key("projectile"))
        .count()
}

struct Duel {
    k: Kernel,
    realm: CapsuleId,
    ana: EntityId,
    budi: EntityId,
}

fn duel() -> Duel {
    let mut k = Kernel::new(Law::default(), 9);
    let realm = install(&mut k, Actor::Admin, REALM, vec![]);
    let hub = k.state().capsules[&realm].self_entity;
    step(
        &mut k,
        vec![
            (player("ana"), signal(hub, "join", 0.0)),
            (player("budi"), signal(hub, "join", 0.0)),
        ],
    );
    let ana = avatar_of(&k, "ana");
    let budi = avatar_of(&k, "budi");
    step(
        &mut k,
        vec![
            (Actor::Admin, set(ana, "pos.x", 0.0)),
            (Actor::Admin, set(ana, "pos.y", 0.0)),
            (Actor::Admin, set(budi, "pos.x", 6.0)),
            (Actor::Admin, set(budi, "pos.y", 0.0)),
        ],
    );
    step(&mut k, vec![]);
    Duel {
        k,
        realm,
        ana,
        budi,
    }
}

fn read(avatar: EntityId) -> Vec<Grant> {
    vec![Grant {
        entity: avatar,
        write: false,
    }]
}

fn fire(k: &mut Kernel, who: &str, avatar: EntityId, ticks: usize) {
    for _ in 0..ticks {
        step(k, vec![(player(who), signal(avatar, "fire", 1.0))]);
    }
}

const SABOTEUR: &str = r#"(module
  (import "world" "find" (func $find (param i32 i32 i64) (result i64)))
  (import "world" "emit" (func $emit (param i64 i32 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "projectile")
  (data (i32.const 16) "hit")
  (data (i32.const 24) "hx")
  (data (i32.const 32) "hy")
  (data (i32.const 40) "hz")
  (func (export "tick") (local $p i64)
    (local.set $p (i64.const -1))
    (loop $next
      (local.set $p (call $find (i32.const 0) (i32.const 10) (local.get $p)))
      (if (i64.lt_s (local.get $p) (i64.const 0)) (then (return)))
      (call $emit (local.get $p) (i32.const 24) (i32.const 2) (f64.const 40))
      (call $emit (local.get $p) (i32.const 32) (i32.const 2) (f64.const 40))
      (call $emit (local.get $p) (i32.const 40) (i32.const 2) (f64.const 0.9))
      (call $emit (local.get $p) (i32.const 16) (i32.const 3) (f64.const -1))
      (br $next))))"#;

const MINE: &str = r#"(module
  (import "world" "spawn" (func $spawn (result i32)))
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (import "world" "kind" (func $kind (param i64 i32 i32) (result i32)))
  (import "world" "self" (func $self (result i64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "projectile")
  (data (i32.const 16) "pos.x")
  (data (i32.const 24) "pos.y")
  (data (i32.const 32) "pos.z")
  (data (i32.const 48) "boom")
  (data (i32.const 56) "blast")
  (data (i32.const 64) "done")
  (func (export "tick") (local $h i32)
    (if (call $kind (call $self) (i32.const 64) (i32.const 4)) (then (return)))
    (call $set (i32.const 0) (i32.const 64) (i32.const 4) (f64.const 1))
    (local.set $h (call $spawn))
    (call $set (local.get $h) (i32.const 0) (i32.const 10) (f64.const 25))
    (call $set (local.get $h) (i32.const 56) (i32.const 5) (f64.const 6))
    (call $set (local.get $h) (i32.const 16) (i32.const 5) (f64.const 12))
    (call $set (local.get $h) (i32.const 24) (i32.const 5) (f64.const 0))
    (call $set (local.get $h) (i32.const 32) (i32.const 5) (f64.const 0.9))
    (call $set (local.get $h) (i32.const 48) (i32.const 4) (f64.const 1))))"#;

fn hits_in(r: &TickReport) -> usize {
    r.signals.iter().filter(|s| s.name == "hit").count()
}

#[test]
fn own_blast_never_hurts_the_shooter() {
    let Duel {
        mut k,
        realm,
        ana,
        budi,
    } = duel();
    step(
        &mut k,
        vec![
            (Actor::Admin, set(budi, "pos.x", 2.0)),
            (Actor::Admin, set(budi, "pos.y", 0.0)),
        ],
    );
    install(&mut k, player("ana"), LAUNCHER, read(ana));
    step(&mut k, vec![(player("ana"), signal(ana, "weapon", 2.0))]);
    step(&mut k, vec![(player("ana"), signal(ana, "fire", 2.0))]);
    for _ in 0..15 {
        step(&mut k, vec![]);
    }
    let b = entry(&k, realm, budi);
    let a = entry(&k, realm, ana);
    assert!(b[HEALTH] < 100.0, "sanity: the missile reached budi: {b:?}");
    assert_eq!(
        a[HEALTH], 100.0,
        "guide: 'that body is never hit by its own projectile'; ana entry: {a:?}"
    );
    assert_eq!(a[DEATHS], 0.0, "ana must not die to her own blast: {a:?}");
}

#[test]
fn dead_players_cannot_fire_or_kill() {
    let Duel {
        mut k,
        realm,
        ana,
        budi,
    } = duel();
    install(&mut k, player("ana"), RIFLE, read(ana));
    install(&mut k, player("budi"), RIFLE, read(budi));
    step(&mut k, vec![(player("budi"), signal(budi, "ax", -1.0))]);
    fire(&mut k, "budi", budi, 40);
    let a = entry(&k, realm, ana);
    assert_eq!(a[DEATHS], 1.0, "budi killed ana: {a:?}");
    assert!(a[DEAD] > 0.0, "ana is in her respawn window: {a:?}");
    let window = (a[DEAD] as usize).saturating_sub(1).min(6);
    assert!(window >= 2, "ana must stay dead while she fires: {a:?}");
    fire(&mut k, "ana", ana, window);
    let anas = k
        .world()
        .entities()
        .filter(|(_, e)| {
            e.components.contains_key("projectile")
                && e.components.get("from") == Some(&Value::Num(ana.0 as f64))
        })
        .count();
    fire(&mut k, "budi", budi, 10);
    for _ in 0..6 {
        step(&mut k, vec![]);
    }
    let b = entry(&k, realm, budi);
    let a = entry(&k, realm, ana);
    println!(
        "dead-fire probe: ana bullets in flight {anas}, budi health {}, ana kills {}, ana deaths {}, ana dead {}",
        b[HEALTH], a[KILLS], a[DEATHS], a[DEAD]
    );
    assert_eq!(
        anas, 0,
        "a dead player's trigger must not spawn bullets, found {anas}"
    );
    assert_eq!(
        b[HEALTH], 100.0,
        "a dead player must not deal damage: {b:?}"
    );
    assert_eq!(a[KILLS], 0.0, "no kills while dead: {a:?}");
    assert_eq!(
        a[DEATHS], 1.0,
        "a corpse cannot be killed again during the respawn window: {a:?}"
    );
    for _ in 0..40 {
        step(&mut k, vec![]);
    }
    assert_eq!(entry(&k, realm, ana)[DEAD], 0.0, "ana respawned");
    assert_eq!(
        k.world().entity(ana).unwrap().components.get("weapon"),
        Some(&Value::Num(1.0)),
        "a respawned body holds weapon 1 again"
    );
    fire(&mut k, "ana", ana, 3);
    let after = k
        .world()
        .entities()
        .filter(|(_, e)| e.components.get("from") == Some(&Value::Num(ana.0 as f64)))
        .count();
    assert!(after > 0, "a respawned player can fire again");
}

#[test]
fn forged_hit_signals_cannot_kill_projectiles_in_flight() {
    let Duel {
        mut k,
        realm,
        ana,
        budi,
        ..
    } = duel();
    install(&mut k, player("ana"), LAUNCHER, read(ana));
    install(&mut k, player("budi"), SABOTEUR, vec![]);
    step(&mut k, vec![(player("ana"), signal(ana, "weapon", 2.0))]);
    step(&mut k, vec![(player("ana"), signal(ana, "fire", 2.0))]);
    for _ in 0..25 {
        step(&mut k, vec![]);
    }
    let b = entry(&k, realm, budi);
    assert!(
        b[HEALTH] < 100.0,
        "budi's capsule shot down ana's missile with forged hit signals; budi untouched: {b:?}"
    );
}

#[test]
fn boom_false_does_not_detonate() {
    let mut k = Kernel::new(Law::default(), 9);
    install(&mut k, Actor::Admin, REALM, vec![]);
    spawn(
        &mut k,
        &[
            ("projectile", Value::Num(25.0)),
            ("boom", Value::Bool(false)),
            ("blast", Value::Num(6.0)),
            ("pos.x", Value::Num(0.0)),
            ("pos.y", Value::Num(0.0)),
            ("pos.z", Value::Num(0.9)),
        ],
    );
    let mut total = 0;
    let mut seen = Vec::new();
    for _ in 0..3 {
        let r = step(&mut k, vec![]);
        total += hits_in(&r);
        seen.extend(r.signals.iter().filter(|s| s.name == "hit").cloned());
    }
    assert_eq!(
        total, 0,
        "boom = false must not detonate; hit signals: {seen:?}"
    );
}

#[test]
fn spent_projectiles_never_detonate_twice() {
    let mut k = Kernel::new(Law::default(), 9);
    let realm = install(&mut k, Actor::Admin, REALM, vec![]);
    let hub = k.state().capsules[&realm].self_entity;
    step(&mut k, vec![(player("vic"), signal(hub, "join", 0.0))]);
    let vic = avatar_of(&k, "vic");
    step(
        &mut k,
        vec![
            (Actor::Admin, set(vic, "pos.x", 12.0)),
            (Actor::Admin, set(vic, "pos.y", 0.0)),
        ],
    );
    let mut left = 70;
    while left > 0 {
        let batch = left.min(32);
        for _ in 0..batch {
            let wasm = k.add_blob(MINE.as_bytes().to_vec());
            let budget = Law::default().ceiling;
            let r = raw(
                &mut k,
                vec![(
                    player("eve"),
                    Action::Install {
                        wasm,
                        grants: vec![],
                        budget,
                    },
                )],
            );
            assert!(
                matches!(r.inputs[0], InputOutcome::Installed(_)),
                "{:?}",
                r.inputs[0]
            );
        }
        left -= batch;
    }
    for _ in 0..6 {
        step(&mut k, vec![]);
    }
    assert_eq!(projectiles(&k), 70, "the mines persist");
    let mut total = 0;
    for _ in 0..4 {
        let r = step(&mut k, vec![]);
        total += hits_in(&r);
    }
    let v = entry(&k, realm, vic);
    println!("vic entry after detonations: {v:?}");
    assert_eq!(
        total, 0,
        "spent mines re-detonated {total} times in 4 ticks (ring reuse)"
    );
}

#[test]
fn kernel_rejects_non_finite_input_values() {
    let Duel { mut k, ana, .. } = duel();
    for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let r = raw(&mut k, vec![(player("ana"), signal(ana, "mx", v))]);
        assert!(
            matches!(r.inputs[0], InputOutcome::Rejected(_)),
            "signal value {v} must be rejected"
        );
    }
    let own = match &step(
        &mut k,
        vec![(
            player("ana"),
            Action::Spawn {
                components: BTreeMap::from([("pos.x".to_owned(), Value::Num(1.0))]),
            },
        )],
    )
    .inputs[0]
    {
        InputOutcome::Spawned(id) => *id,
        other => panic!("spawn failed: {other:?}"),
    };
    let r = raw(
        &mut k,
        vec![(
            player("ana"),
            Action::Set {
                entity: own,
                key: "pos.x".into(),
                value: Value::Num(f64::NAN),
            },
        )],
    );
    assert!(
        matches!(r.inputs[0], InputOutcome::Rejected(_)),
        "NaN writes must be rejected"
    );
}

#[test]
fn huge_and_negative_inputs_are_clamped() {
    let Duel {
        mut k, realm, ana, ..
    } = duel();
    let x0 = entry(&k, realm, ana)[X];
    for _ in 0..10 {
        step(
            &mut k,
            vec![
                (player("ana"), signal(ana, "mx", 1e300)),
                (player("ana"), signal(ana, "my", -1e300)),
                (player("ana"), signal(ana, "run", 1e300)),
                (player("ana"), signal(ana, "weapon", 1e9)),
            ],
        );
    }
    let x1 = entry(&k, realm, ana)[X];
    let dx = x1 - x0;
    assert!(
        dx <= 0.8 * 10.0 + 1e-9 && dx > 0.0,
        "run speed caps movement: moved {dx} in 10 ticks"
    );
    assert!(x1.is_finite());
    assert_eq!(num(&k, ana, "weapon"), 9.0, "weapon clamps into 1..=9");
    step(&mut k, vec![(player("ana"), signal(ana, "weapon", -4.0))]);
    assert_eq!(num(&k, ana, "weapon"), 1.0, "weapon clamps into 1..=9");
    step(
        &mut k,
        vec![
            (player("ana"), signal(ana, "ax", 1e300)),
            (player("ana"), signal(ana, "az", -1e300)),
        ],
    );
    assert_eq!(num(&k, ana, "aim.x"), 1.0);
    assert_eq!(num(&k, ana, "aim.z"), -1.0);
}

#[test]
fn leave_and_rejoin() {
    let mut k = Kernel::new(Law::default(), 9);
    let realm = install(&mut k, Actor::Admin, REALM, vec![]);
    let hub = k.state().capsules[&realm].self_entity;
    step(&mut k, vec![(player("cara"), signal(hub, "join", 0.0))]);
    let cara = avatar_of(&k, "cara");
    assert_eq!(roster(&k, realm).len(), 1);
    step(&mut k, vec![(player("cara"), signal(cara, "leave", 1.0))]);
    step(&mut k, vec![]);
    assert_eq!(body_of(&k, "cara"), None, "the body is gone after leaving");
    assert_eq!(roster(&k, realm).len(), 0);
    step(&mut k, vec![(player("cara"), signal(hub, "join", 0.0))]);
    let cara2 = avatar_of(&k, "cara");
    assert_ne!(cara, cara2, "a fresh body on rejoin");
    assert_eq!(entry(&k, realm, cara2)[HEALTH], 100.0);
    assert_eq!(roster(&k, realm).len(), 1);
}

#[test]
fn realm_caps_players_at_32_without_faults() {
    let mut k = Kernel::new(Law::default(), 9);
    let realm = install(&mut k, Actor::Admin, REALM, vec![]);
    let hub = k.state().capsules[&realm].self_entity;
    for _wave in 0..12 {
        let inputs: Vec<(Actor, Action)> = (0..64)
            .map(|i| (player(&format!("p{i}")), signal(hub, "join", 0.0)))
            .collect();
        let r = step(&mut k, inputs);
        assert_eq!(r.inputs.len(), 64);
    }
    let n = roster(&k, realm).len();
    assert_eq!(n, 32, "the roster hard-caps at 32 players, got {n}");
    for i in 0..32 {
        assert!(body_of(&k, &format!("p{i}")).is_some(), "p{i} has a body");
    }
    assert!(body_of(&k, "p40").is_none(), "late joiners are refused");
}

#[test]
fn rotated_walls_still_stop_bullets() {
    let Duel {
        mut k,
        realm,
        ana,
        budi,
        ..
    } = duel();
    spawn(
        &mut k,
        &[
            ("shape", Value::Text("box".into())),
            ("pos.x", Value::Num(3.0)),
            ("pos.y", Value::Num(0.0)),
            ("pos.z", Value::Num(1.5)),
            ("scale.x", Value::Num(0.2)),
            ("scale.y", Value::Num(4.0)),
            ("scale.z", Value::Num(3.0)),
            ("rot.z", Value::Num(std::f64::consts::FRAC_PI_4)),
        ],
    );
    install(&mut k, player("ana"), RIFLE, read(ana));
    fire(&mut k, "ana", ana, 30);
    assert_eq!(
        entry(&k, realm, budi)[HEALTH],
        100.0,
        "a box rotated 45 degrees about z still blocks every bullet"
    );
}

#[test]
fn thin_wall_holds_at_run_speed() {
    let Duel {
        mut k, realm, ana, ..
    } = duel();
    spawn(
        &mut k,
        &[
            ("shape", Value::Text("box".into())),
            ("pos.x", Value::Num(3.0)),
            ("pos.y", Value::Num(0.0)),
            ("pos.z", Value::Num(1.5)),
            ("scale.x", Value::Num(0.05)),
            ("scale.y", Value::Num(4.0)),
            ("scale.z", Value::Num(3.0)),
        ],
    );
    step(&mut k, vec![(player("ana"), signal(ana, "mx", 1.0))]);
    step(&mut k, vec![(player("ana"), signal(ana, "run", 1.0))]);
    for _ in 0..20 {
        step(&mut k, vec![]);
    }
    let x = entry(&k, realm, ana)[X];
    assert!(
        x < 3.0,
        "no tunnelling through a 5 cm wall at run speed, stopped at {x}"
    );
}

#[test]
fn load_32_players_move_and_fire_within_fuel_budget() {
    let mut k = Kernel::new(Law::default(), 9);
    let realm = install(&mut k, Actor::Admin, REALM, vec![]);
    let hub = k.state().capsules[&realm].self_entity;
    for _wave in 0..6 {
        let inputs: Vec<(Actor, Action)> = (0..32)
            .map(|i| (player(&format!("p{i}")), signal(hub, "join", 0.0)))
            .collect();
        step(&mut k, inputs);
    }
    assert_eq!(roster(&k, realm).len(), 32);
    let bodies: Vec<EntityId> = (0..32).map(|i| avatar_of(&k, &format!("p{i}"))).collect();
    let wasm = k.add_blob(RIFLE.as_bytes().to_vec());
    let budget = Law::default().ceiling;
    for (i, body) in bodies.iter().enumerate().take(16) {
        let r = raw(
            &mut k,
            vec![(
                player(&format!("p{i}")),
                Action::Install {
                    wasm,
                    grants: read(*body),
                    budget,
                },
            )],
        );
        assert!(matches!(r.inputs[0], InputOutcome::Installed(_)));
    }
    let ceiling = Law::default().ceiling.fuel_per_tick;
    let region_cap = Law::default().region_fuel_per_tick;
    let mut max_realm = 0u64;
    let mut max_total = 0u64;
    for _ in 0..30 {
        let mut inputs: Vec<(Actor, Action)> = Vec::new();
        for (i, b) in bodies.iter().enumerate() {
            inputs.push((player(&format!("p{i}")), signal(*b, "mx", 1.0)));
            inputs.push((player(&format!("p{i}")), signal(*b, "run", 1.0)));
            if i < 16 {
                inputs.push((player(&format!("p{i}")), signal(*b, "fire", 1.0)));
            }
        }
        let r = step(&mut k, inputs);
        let total: u64 = r.runs.iter().map(|x| x.fuel_used).sum();
        let realm_fuel = r
            .runs
            .iter()
            .find(|x| x.capsule == realm)
            .map(|x| x.fuel_used)
            .expect("the realm ran");
        max_realm = max_realm.max(realm_fuel);
        max_total = max_total.max(total);
    }
    println!("max realm fuel per tick: {max_realm} / {ceiling}");
    println!("max region fuel per tick: {max_total} / {region_cap}");
    assert!(
        max_realm < ceiling,
        "the realm stays inside its fuel budget: {max_realm} < {ceiling}"
    );
    assert!(max_total <= region_cap);
    let before = k.state_hash();
    let journal = k.checkpoint();
    let (replayed, hashes) =
        Kernel::replay(&journal, k.blobs().clone()).expect("the journal replays");
    assert_eq!(
        *hashes.last().expect("nonempty journal"),
        before,
        "replay is bit-for-bit deterministic"
    );
    assert_eq!(replayed.state_hash(), before);
}

#[test]
fn overlong_names_cannot_join() {
    let mut k = Kernel::new(Law::default(), 9);
    let realm = install(&mut k, Actor::Admin, REALM, vec![]);
    let hub = k.state().capsules[&realm].self_entity;
    let long = "x".repeat(60);
    step(&mut k, vec![(player(&long), signal(hub, "join", 0.0))]);
    step(&mut k, vec![]);
    println!("roster after 60-char join: {}", roster(&k, realm).len());
}

#[test]
fn corpses_cannot_be_killed_again() {
    let Duel {
        mut k,
        realm,
        ana,
        budi,
    } = duel();
    install(&mut k, player("budi"), RIFLE, read(budi));
    step(&mut k, vec![(player("budi"), signal(budi, "ax", -1.0))]);
    fire(&mut k, "budi", budi, 40);
    let a = entry(&k, realm, ana);
    assert_eq!(a[DEATHS], 1.0, "budi killed ana: {a:?}");
    fire(&mut k, "budi", budi, 15);
    let a = entry(&k, realm, ana);
    println!(
        "corpse probe: deaths {}, dead {}, health {}",
        a[DEATHS], a[DEAD], a[HEALTH]
    );
    assert_eq!(
        a[DEATHS], 1.0,
        "a corpse must not be killed again during the respawn window: {a:?}"
    );
}
