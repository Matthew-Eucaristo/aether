use std::collections::BTreeMap;

use aether_kernel::{Action, Actor, Grant, Input, InputOutcome, Kernel, Law, TickReport};
use aether_world::{CapsuleId, EntityId, PlayerId, Value};

const REALM: &str = include_str!("../../../capsules/realm.wat");

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

fn signal(entity: EntityId, name: &str, value: f64) -> Action {
    Action::Signal {
        entity,
        name: name.into(),
        value,
    }
}

fn avatar_of(k: &Kernel, name: &str) -> EntityId {
    let label = Value::Text(format!("player:{name}"));
    k.world()
        .entities()
        .find(|(_, e)| e.components.get("player") == Some(&label))
        .map(|(id, _)| id)
        .expect("the realm made a body")
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

fn num(k: &Kernel, id: EntityId, key: &str) -> f64 {
    match k.world().entity(id).unwrap().components.get(key) {
        Some(Value::Num(n)) => *n,
        _ => 0.0,
    }
}

#[test]
fn a_home_in_another_dimension_returns_the_player_to_that_dimension() {
    let mut k = Kernel::new(Law::default(), 4);
    let realm = install(&mut k, Actor::Admin, REALM, vec![]);
    let hub = k.state().capsules[&realm].self_entity;
    step(&mut k, vec![(player("ana"), signal(hub, "join", 0.0))]);
    let ana = avatar_of(&k, "ana");
    step(&mut k, vec![(player("ana"), signal(ana, "enter", 3.0))]);
    step(&mut k, vec![]);
    assert_eq!(num(&k, ana, "dim"), 3.0);
    step(&mut k, vec![(player("ana"), signal(ana, "home", 1.0))]);
    step(&mut k, vec![(player("ana"), signal(ana, "leave", 1.0))]);
    step(&mut k, vec![]);
    step(&mut k, vec![(player("ana"), signal(hub, "join", 0.0))]);
    step(&mut k, vec![]);
    let returning = avatar_of(&k, "ana");
    assert_eq!(
        num(&k, returning, "dim"),
        3.0,
        "home was stored while ana stood in dimension 3"
    );
}

const HEALTH: usize = 3;
const RIFLE: &str = include_str!("../../../capsules/rifle.wat");

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

fn dimension_three_wall_scenario(crowd: bool) -> f64 {
    let mut k = Kernel::new(Law::default(), 10);
    let realm = install(&mut k, Actor::Admin, REALM, vec![]);
    let hub = k.state().capsules[&realm].self_entity;
    step(
        &mut k,
        vec![
            (player("ana"), signal(hub, "join", 0.0)),
            (player("budi"), signal(hub, "join", 0.0)),
            (player("carol"), signal(hub, "join", 0.0)),
        ],
    );
    let ana = avatar_of(&k, "ana");
    let carol = avatar_of(&k, "carol");
    step(
        &mut k,
        vec![
            (player("ana"), signal(ana, "enter", 3.0)),
            (player("carol"), signal(carol, "enter", 3.0)),
        ],
    );
    step(&mut k, vec![]);

    let columns = if crowd { 34 } else { 0 };
    let mut batch = Vec::new();
    for gx in 0..columns {
        for gy in 0..17 {
            let mut components = BTreeMap::new();
            components.insert("shape".to_owned(), Value::Text("box".into()));
            components.insert("pos.x".to_owned(), Value::Num((gx as f64 - 16.5) * 1.1));
            components.insert("pos.y".to_owned(), Value::Num((gy as f64 - 8.0) * 1.1));
            components.insert("pos.z".to_owned(), Value::Num(0.5));
            components.insert("scale.x".to_owned(), Value::Num(0.2));
            components.insert("scale.y".to_owned(), Value::Num(0.2));
            components.insert("scale.z".to_owned(), Value::Num(1.0));
            batch.push((Actor::Admin, Action::Spawn { components }));
            if batch.len() == 32 {
                step(&mut k, std::mem::take(&mut batch));
            }
        }
    }
    if !batch.is_empty() {
        step(&mut k, batch);
    }

    spawn(
        &mut k,
        &[
            ("shape", Value::Text("box".into())),
            ("pos.x", Value::Num(3.0)),
            ("pos.y", Value::Num(0.0)),
            ("pos.z", Value::Num(1.5)),
            ("dim", Value::Num(3.0)),
            ("scale.x", Value::Num(0.2)),
            ("scale.y", Value::Num(4.0)),
            ("scale.z", Value::Num(3.0)),
        ],
    );
    step(
        &mut k,
        vec![
            (Actor::Admin, set(ana, "pos.x", 0.0)),
            (Actor::Admin, set(ana, "pos.y", 0.0)),
            (Actor::Admin, set(carol, "pos.x", 6.0)),
            (Actor::Admin, set(carol, "pos.y", 0.0)),
        ],
    );
    install(&mut k, player("ana"), RIFLE, read(ana));
    step(&mut k, vec![(player("ana"), signal(ana, "ax", 1.0))]);
    fire(&mut k, "ana", ana, 20);
    entry(&k, realm, carol)[HEALTH]
}

#[test]
fn a_dimension_three_wall_stops_bullets_when_dimension_zero_is_empty() {
    assert_eq!(dimension_three_wall_scenario(false), 100.0);
}

#[test]
fn crowded_dimension_zero_does_not_blind_collision_queries_in_other_dimensions() {
    assert_eq!(
        dimension_three_wall_scenario(true),
        100.0,
        "the dimension-3 wall should take every bullet"
    );
}
