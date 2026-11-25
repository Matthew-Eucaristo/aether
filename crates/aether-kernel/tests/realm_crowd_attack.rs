use std::collections::BTreeMap;

use aether_kernel::{Action, Actor, Grant, Input, InputOutcome, Kernel, Law, TickReport};
use aether_world::{CapsuleId, EntityId, PlayerId, Value};

const REALM: &str = include_str!("../../../capsules/realm.wat");
const RIFLE: &str = include_str!("../../../capsules/rifle.wat");
const X: usize = 7;
const Y: usize = 8;

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

fn install_capsule(k: &mut Kernel, actor: Actor, src: &str, grants: Vec<Grant>) -> CapsuleId {
    let wasm = k.add_blob(src.as_bytes().to_vec());
    match &step(
        k,
        vec![(
            actor,
            Action::Install {
                wasm,
                grants,
                budget: Law::default().ceiling,
            },
        )],
    )
    .inputs[0]
    {
        InputOutcome::Installed(id) => *id,
        other => panic!("install failed: {other:?}"),
    }
}

fn install_realm(k: &mut Kernel) -> CapsuleId {
    let wasm = k.add_blob(REALM.as_bytes().to_vec());
    match &step(
        k,
        vec![(
            Actor::Admin,
            Action::Install {
                wasm,
                grants: vec![],
                budget: Law::default().ceiling,
            },
        )],
    )
    .inputs[0]
    {
        InputOutcome::Installed(id) => *id,
        other => panic!("install failed: {other:?}"),
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

fn body_of(k: &Kernel, name: &str) -> EntityId {
    let label = Value::Text(format!("player:{name}"));
    k.world()
        .entities()
        .find(|(_, e)| e.components.get("player") == Some(&label))
        .map(|(id, _)| id)
        .expect("the realm made a body")
}

fn crowd32() -> (Kernel, CapsuleId, Vec<EntityId>) {
    let mut k = Kernel::new(Law::default(), 9);
    let realm = install_realm(&mut k);
    let hub = k.state().capsules[&realm].self_entity;
    for _ in 0..6 {
        let inputs: Vec<(Actor, Action)> = (0..32)
            .map(|i| (player(&format!("p{i}")), signal(hub, "join", 0.0)))
            .collect();
        step(&mut k, inputs);
    }
    assert_eq!(roster(&k, realm).len(), 32);
    let bodies: Vec<EntityId> = (0..32).map(|i| body_of(&k, &format!("p{i}"))).collect();
    (k, realm, bodies)
}

fn teleport(k: &mut Kernel, bodies: &[EntityId], x: f64, y: f64) {
    let mut inputs = Vec::new();
    for body in bodies {
        inputs.push((Actor::Admin, set(*body, "pos.x", x)));
        inputs.push((Actor::Admin, set(*body, "pos.y", y)));
    }
    for chunk in inputs.chunks(32) {
        step(k, chunk.to_vec());
    }
}

fn two_bodies() -> (Kernel, CapsuleId, EntityId, EntityId) {
    let mut k = Kernel::new(Law::default(), 9);
    let realm = install_realm(&mut k);
    let hub = k.state().capsules[&realm].self_entity;
    step(
        &mut k,
        vec![
            (player("ana"), signal(hub, "join", 0.0)),
            (player("budi"), signal(hub, "join", 0.0)),
        ],
    );
    let ana = body_of(&k, "ana");
    let budi = body_of(&k, "budi");
    step(
        &mut k,
        vec![
            (Actor::Admin, set(ana, "pos.x", -2.0)),
            (Actor::Admin, set(ana, "pos.y", 0.0)),
            (Actor::Admin, set(budi, "pos.x", 2.0)),
            (Actor::Admin, set(budi, "pos.y", 0.0)),
        ],
    );
    (k, realm, ana, budi)
}

fn entry(rows: &[[f64; 20]], avatar: EntityId) -> [f64; 20] {
    rows.iter()
        .copied()
        .find(|e| e[0] == avatar.0 as f64)
        .expect("avatar is in the roster")
}

#[test]
fn coincident_crowd_of_32_stays_within_fuel_budget() {
    let (mut k, realm, bodies) = crowd32();
    teleport(&mut k, &bodies, 0.0, 0.0);
    for tick in 0..3 {
        let report = step(&mut k, vec![]);
        let rows = roster(&k, realm);
        let positions: Vec<String> = rows
            .iter()
            .enumerate()
            .map(|(i, e)| format!("{i}:{:.2},{:.2}", e[X], e[Y]))
            .collect();
        let farthest = rows
            .iter()
            .map(|e| e[X].abs().max(e[Y].abs()))
            .fold(0.0, f64::max);
        println!(
            "tick {tick} fuel={} farthest {farthest:.3}: {}",
            report.runs[0].fuel_used,
            positions.join(" ")
        );
        assert!(report.runs[0].fuel_used < 10_000_000);
        assert!(farthest.is_finite());
    }
}

#[test]
fn coincident_crowd_cannot_catapult_bodies_through_a_thin_wall() {
    let (mut k, realm, bodies) = crowd32();
    spawn(
        &mut k,
        &[
            ("shape", Value::Text("box".into())),
            ("pos.x", Value::Num(3.0)),
            ("pos.y", Value::Num(0.0)),
            ("pos.z", Value::Num(1.5)),
            ("scale.x", Value::Num(0.05)),
            ("scale.y", Value::Num(12.0)),
            ("scale.z", Value::Num(3.0)),
        ],
    );
    teleport(&mut k, &bodies, 0.0, 0.0);
    step(&mut k, vec![]);
    let rows = roster(&k, realm);
    let farthest = rows.iter().map(|e| e[X]).fold(f64::NEG_INFINITY, f64::max);
    println!(
        "after one tick through a thin wall: max_x={farthest}, positions={:?}",
        rows.iter().map(|e| [e[X], e[Y]]).collect::<Vec<_>>()
    );
    assert!(
        farthest < 3.0,
        "a coincident crowd catapulted a body through the wall in one tick: max_x={farthest}"
    );
}

#[test]
fn runners_cannot_tunnel_through_each_other() {
    let (mut k, realm, ana, budi) = two_bodies();
    step(
        &mut k,
        vec![
            (player("ana"), signal(ana, "mx", 1.0)),
            (player("ana"), signal(ana, "run", 1.0)),
            (player("budi"), signal(budi, "mx", -1.0)),
            (player("budi"), signal(budi, "run", 1.0)),
        ],
    );
    for tick in 0..5 {
        step(&mut k, vec![]);
        let rows = roster(&k, realm);
        let a = entry(&rows, ana);
        let b = entry(&rows, budi);
        println!(
            "tick {tick}: ana=({},{}), budi=({},{})",
            a[X], a[Y], b[X], b[Y]
        );
        assert!(
            a[X] < b[X],
            "the runners exchanged sides at tick {tick}: ana_x={} budi_x={}",
            a[X],
            b[X]
        );
    }
}

#[test]
fn dead_bodies_remain_body_to_body_obstacles() {
    let (mut k, realm, ana, budi) = two_bodies();
    install_capsule(
        &mut k,
        player("ana"),
        RIFLE,
        vec![Grant {
            entity: ana,
            write: false,
        }],
    );
    for _ in 0..40 {
        step(&mut k, vec![(player("ana"), signal(ana, "fire", 1.0))]);
    }
    let rows = roster(&k, realm);
    let dead = entry(&rows, budi);
    assert_eq!(dead[5], 1.0, "budi died: {dead:?}");
    assert!(dead[6] > 0.0, "budi is still dead: {dead:?}");
    let corpse_x = dead[X];
    step(
        &mut k,
        vec![
            (Actor::Admin, set(ana, "pos.x", corpse_x)),
            (Actor::Admin, set(ana, "pos.y", dead[Y])),
        ],
    );
    for _ in 0..6 {
        step(&mut k, vec![]);
    }
    let rows = roster(&k, realm);
    let a = entry(&rows, ana);
    let b = entry(&rows, budi);
    let separation = ((a[X] - b[X]).powi(2) + (a[Y] - b[Y]).powi(2)).sqrt();
    println!(
        "corpse overlap probe: ana=({},{}), corpse=({},{}), separation={separation}",
        a[X], a[Y], b[X], b[Y]
    );
    assert!(
        separation >= 0.7,
        "the live body overlaps a corpse by {}; bodies are documented to collide with each other",
        0.7 - separation
    );
}

#[test]
fn bodies_in_different_dimensions_do_not_crowd() {
    let (mut k, realm, ana, budi) = two_bodies();
    step(&mut k, vec![(player("budi"), signal(budi, "enter", 1.0))]);
    step(
        &mut k,
        vec![
            (Actor::Admin, set(ana, "pos.x", 0.0)),
            (Actor::Admin, set(ana, "pos.y", 0.0)),
            (Actor::Admin, set(budi, "pos.x", 0.0)),
            (Actor::Admin, set(budi, "pos.y", 0.0)),
        ],
    );
    step(&mut k, vec![]);
    let rows = roster(&k, realm);
    let a = entry(&rows, ana);
    let b = entry(&rows, budi);
    let separation = ((a[X] - b[X]).powi(2) + (a[Y] - b[Y]).powi(2)).sqrt();
    println!(
        "cross-dimension overlap probe: ana=({},{}), budi=({},{}), separation={separation}",
        a[X], a[Y], b[X], b[Y]
    );
    assert_eq!(
        separation, 0.0,
        "bodies in dimensions 0 and 1 must not push each other"
    );
}
