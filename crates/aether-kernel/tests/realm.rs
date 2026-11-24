use std::collections::BTreeMap;

use aether_kernel::{Action, Actor, Grant, Input, InputOutcome, Kernel, Law, TickReport};
use aether_world::{CapsuleId, EntityId, PlayerId, Value};

const REALM: &str = include_str!("../../../capsules/realm.wat");
const RIFLE: &str = include_str!("../../../capsules/rifle.wat");
const LAUNCHER: &str = include_str!("../../../capsules/launcher.wat");

const HEALTH: usize = 3;
const KILLS: usize = 4;
const DEATHS: usize = 5;
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

fn avatar_of(k: &Kernel, name: &str) -> EntityId {
    let label = Value::Text(format!("player:{name}"));
    k.world()
        .entities()
        .find(|(_, e)| e.components.get("player") == Some(&label))
        .map(|(id, _)| id)
        .expect("the realm made a body")
}

fn num(k: &Kernel, id: EntityId, key: &str) -> f64 {
    match k.world().entity(id).unwrap().components.get(key) {
        Some(Value::Num(n)) => *n,
        other => panic!("{key}: {other:?}"),
    }
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
            (player("budi"), signal(hub, "join", 0.0)),
            (Actor::Admin, signal(hub, "join", 0.0)),
        ],
    );
    assert_eq!(
        roster(&k, realm).len(),
        2,
        "one body per player, none for admin"
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
    assert_eq!(entry(&k, realm, budi)[X], 6.0, "admins can place bodies");
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

#[test]
fn rifle_fire_kills_and_is_credited() {
    let Duel {
        mut k,
        realm,
        ana,
        budi,
    } = duel();
    install(&mut k, player("ana"), RIFLE, read(ana));
    fire(&mut k, "ana", ana, 40);
    let b = entry(&k, realm, budi);
    let a = entry(&k, realm, ana);
    assert_eq!(b[DEATHS], 1.0, "budi died once: {b:?}");
    assert_eq!(a[KILLS], 1.0, "ana is credited: {a:?}");
    assert_eq!(a[HEALTH], 100.0);
    let me = k.state().capsules[&realm].self_entity;
    assert_eq!(
        k.world().entity(me).unwrap().components.get("feed"),
        Some(&Value::Text("ana > budi".into()))
    );
}

#[test]
fn bodies_move_only_by_intent_and_stop_at_walls() {
    let Duel {
        mut k, realm, ana, ..
    } = duel();
    let r = step(&mut k, vec![(player("ana"), set(ana, "pos.x", 30.0))]);
    assert!(
        matches!(r.inputs[0], InputOutcome::Rejected(_)),
        "players cannot teleport their bodies"
    );
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
        ],
    );
    step(&mut k, vec![(player("ana"), signal(ana, "mx", 1.0))]);
    for _ in 0..12 {
        step(&mut k, vec![]);
    }
    let x = entry(&k, realm, ana)[X];
    assert!(
        (2.4..=2.6).contains(&x),
        "walked into the wall and stopped at {x}"
    );
    assert_eq!(num(&k, ana, "pos.x"), x);
    step(&mut k, vec![(player("budi"), signal(ana, "mx", -1.0))]);
    step(&mut k, vec![(player("ana"), signal(ana, "mx", 0.0))]);
    step(&mut k, vec![]);
    assert_eq!(entry(&k, realm, ana)[X], x, "budi cannot steer ana");
    step(&mut k, vec![(player("ana"), signal(ana, "jump", 1.0))]);
    step(&mut k, vec![]);
    assert!(num(&k, ana, "pos.z") > 1.3, "jumping lifts the body");
}

#[test]
fn only_the_owner_can_pull_the_trigger() {
    let Duel {
        mut k,
        realm,
        ana,
        budi,
        ..
    } = duel();
    install(&mut k, player("ana"), RIFLE, read(ana));
    fire(&mut k, "budi", ana, 20);
    assert_eq!(entry(&k, realm, budi)[HEALTH], 100.0);
    assert!(
        k.world()
            .entities()
            .all(|(_, e)| !e.components.contains_key("projectile"))
    );
}

#[test]
fn damage_is_budgeted_per_player_not_per_gun() {
    let Duel {
        mut k,
        realm,
        ana,
        budi,
        ..
    } = duel();
    install(&mut k, player("ana"), RIFLE, read(ana));
    install(&mut k, player("ana"), RIFLE, read(ana));
    install(&mut k, player("ana"), RIFLE, read(ana));
    fire(&mut k, "ana", ana, 6);
    let lost = 100.0 - entry(&k, realm, budi)[HEALTH];
    assert!(lost > 0.0);
    assert!(
        lost <= 80.0 + 8.0 * 6.0,
        "three guns share one player's budget, lost {lost}"
    );
    assert_eq!(
        entry(&k, realm, budi)[DEATHS],
        0.0,
        "three rifles at once did not kill in 0.6 s"
    );
}

#[test]
fn walls_and_solid_spheres_stop_bullets() {
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
            ("shape", Value::Text("sphere".into())),
            ("solid", Value::Bool(true)),
            ("pos.x", Value::Num(3.0)),
            ("pos.y", Value::Num(0.0)),
            ("pos.z", Value::Num(1.0)),
            ("size", Value::Num(1.2)),
        ],
    );
    install(&mut k, player("ana"), RIFLE, read(ana));
    fire(&mut k, "ana", ana, 30);
    assert_eq!(
        entry(&k, realm, budi)[HEALTH],
        100.0,
        "the boulder took every bullet"
    );
    let bullets = k
        .world()
        .entities()
        .filter(|(_, e)| e.components.contains_key("projectile"))
        .count();
    assert!(bullets <= 3, "spent bullets are removed, {bullets} left");
}

const FORGER: &str = r#"(module
  (import "world" "spawn" (func $spawn (result i32)))
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (import "world" "kind" (func $kind (param i64 i32 i32) (result i32)))
  (import "world" "self" (func $self (result i64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "projectile")
  (data (i32.const 16) "pos.x")
  (data (i32.const 24) "pos.y")
  (data (i32.const 32) "pos.z")
  (data (i32.const 40) "from")
  (data (i32.const 48) "boom")
  (data (i32.const 56) "blast")
  (data (i32.const 64) "done")
  (func (export "tick") (local $h i32)
    (if (call $kind (call $self) (i32.const 64) (i32.const 4)) (then (return)))
    (call $set (i32.const 0) (i32.const 64) (i32.const 4) (f64.const 1))
    (local.set $h (call $spawn))
    (call $set (local.get $h) (i32.const 0) (i32.const 10) (f64.const 25))
    (call $set (local.get $h) (i32.const 56) (i32.const 5) (f64.const 6))
    (call $set (local.get $h) (i32.const 16) (i32.const 5) (f64.const 0))
    (call $set (local.get $h) (i32.const 24) (i32.const 5) (f64.const 0))
    (call $set (local.get $h) (i32.const 32) (i32.const 5) (f64.const 0.9))
    (call $set (local.get $h) (i32.const 40) (i32.const 4) (f64.const FROM))
    (call $set (local.get $h) (i32.const 48) (i32.const 4) (f64.const 1))))"#;

#[test]
fn forged_shooters_get_no_credit() {
    let Duel {
        mut k,
        realm,
        ana,
        budi,
        ..
    } = duel();
    step(
        &mut k,
        vec![
            (Actor::Admin, set(budi, "pos.x", 0.5)),
            (Actor::Admin, set(budi, "pos.y", 0.0)),
        ],
    );
    let forged = FORGER.replace("FROM", &budi.0.to_string());
    for _ in 0..4 {
        install(&mut k, player("ana"), &forged, vec![]);
        step(&mut k, vec![]);
        step(&mut k, vec![]);
    }
    let b = entry(&k, realm, budi);
    let a = entry(&k, realm, ana);
    assert!(
        b[HEALTH] < 100.0 || b[DEATHS] > 0.0,
        "the forged blast still hurt budi: {b:?}"
    );
    assert_eq!(b[KILLS], 0.0, "budi is never credited for ana's forgery");
    assert!(
        a[HEALTH] < 100.0 || a[DEATHS] > 0.0,
        "a forged from does not shield the real shooter"
    );
}

#[test]
fn missiles_explode_with_splash_damage() {
    let Duel {
        mut k,
        realm,
        ana,
        budi,
        ..
    } = duel();
    install(&mut k, player("ana"), LAUNCHER, read(ana));
    step(&mut k, vec![(player("ana"), signal(ana, "weapon", 2.0))]);
    let mut flash = 0.0f64;
    for i in 0..30 {
        let fire = if i == 0 {
            vec![(player("ana"), signal(ana, "fire", 2.0))]
        } else {
            vec![]
        };
        step(&mut k, fire);
        for (_, e) in k.world().entities() {
            if let Some(Value::Num(l)) = e.components.get("light") {
                flash = flash.max(*l);
            }
        }
    }
    let b = entry(&k, realm, budi);
    assert!(
        b[HEALTH] < 100.0 && b[HEALTH] > 40.0,
        "one missile hurts but does not kill: {b:?}"
    );
    assert!(
        flash > 30.0,
        "the explosion lit the scene, peak light {flash}"
    );
    let left = k
        .world()
        .entities()
        .filter(|(_, e)| {
            e.components.contains_key("points") || e.components.contains_key("projectile")
        })
        .count();
    assert_eq!(left, 0, "missile, fireball and sparks were cleaned up");
}
