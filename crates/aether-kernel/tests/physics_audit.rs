use std::collections::BTreeMap;

use aether_kernel::{Action, Actor, Input, InputOutcome, Kernel, Law};
use aether_world::{EntityId, Value};

fn spawn(k: &mut Kernel, components: &[(&str, Value)]) -> EntityId {
    let components: BTreeMap<String, Value> = components
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect();
    match &k
        .step(vec![Input {
            actor: Actor::Admin,
            action: Action::Spawn { components },
        }])
        .inputs[0]
    {
        InputOutcome::Spawned(id) => *id,
        other => panic!("spawn failed: {other:?}"),
    }
}

fn num(k: &Kernel, id: EntityId, key: &str) -> f64 {
    match k.world().entity(id).unwrap().components.get(key) {
        Some(Value::Num(n)) => *n,
        _ => 0.0,
    }
}

fn ball(x: f64, y: f64, z: f64) -> Vec<(&'static str, Value)> {
    vec![
        ("body", Value::Bool(true)),
        ("pos.x", Value::Num(x)),
        ("pos.y", Value::Num(y)),
        ("pos.z", Value::Num(z)),
        ("dim", Value::Num(1.0)),
        ("size", Value::Num(0.5)),
    ]
}

#[test]
fn a_fast_body_cannot_tunnel_through_a_thin_box() {
    let mut k = Kernel::new(Law::default(), 7);
    spawn(
        &mut k,
        &[
            ("shape", Value::Text("box".into())),
            ("pos.x", Value::Num(0.0)),
            ("pos.y", Value::Num(0.0)),
            ("pos.z", Value::Num(5.0)),
            ("dim", Value::Num(1.0)),
            ("scale.x", Value::Num(0.2)),
            ("scale.y", Value::Num(10.0)),
            ("scale.z", Value::Num(10.0)),
        ],
    );
    let mut moving = ball(-0.6, 0.0, 5.0);
    moving.push(("vel.x", Value::Num(10.0)));
    let b = spawn(&mut k, &moving);
    for _ in 0..2 {
        k.step(vec![]);
    }
    assert!(
        num(&k, b, "pos.x") < 0.0,
        "the box should stop the body before it reaches the far side; pos.x={} vel.x={}",
        num(&k, b, "pos.x"),
        num(&k, b, "vel.x")
    );
}

#[test]
fn huge_solids_collide_over_their_whole_extent() {
    let mut k = Kernel::new(Law::default(), 8);
    spawn(
        &mut k,
        &[
            ("shape", Value::Text("box".into())),
            ("pos.x", Value::Num(0.0)),
            ("pos.y", Value::Num(0.0)),
            ("pos.z", Value::Num(5.0)),
            ("dim", Value::Num(1.0)),
            ("scale.x", Value::Num(2000.0)),
            ("scale.y", Value::Num(10.0)),
            ("scale.z", Value::Num(10.0)),
        ],
    );
    let b = spawn(&mut k, &ball(600.0, 4.9, 5.0));
    k.step(vec![]);
    assert!(
        num(&k, b, "pos.y") > 5.0,
        "the body starts inside the huge box near its +y face and must be pushed out; pos.y={}",
        num(&k, b, "pos.y")
    );
}
