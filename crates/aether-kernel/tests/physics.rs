use std::collections::BTreeMap;

use aether_kernel::{Action, Actor, Input, InputOutcome, Kernel, Law, Terrain, height};
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
        ("size", Value::Num(0.5)),
    ]
}

fn run(k: &mut Kernel, ticks: usize) {
    for _ in 0..ticks {
        k.step(Vec::new());
    }
}

#[test]
fn bodies_fall_bounce_and_come_to_rest_on_the_terrain() {
    let terrain = Terrain {
        seed: 5,
        relief: 20,
        scale: 80,
        plaza: 0,
    };
    let mut k = Kernel::new(
        Law {
            terrain,
            ..Law::default()
        },
        1,
    );
    let b = spawn(&mut k, &ball(60.0, -20.0, 40.0));
    let mut lowest = f64::MAX;
    for _ in 0..200 {
        k.step(Vec::new());
        let floor = height(&terrain, num(&k, b, "pos.x"), num(&k, b, "pos.y")) + 0.5;
        let z = num(&k, b, "pos.z");
        assert!(z >= floor - 1e-9, "never below the terrain");
        lowest = lowest.min(z - floor);
    }
    assert!(lowest < 1e-9, "it reached the ground");
    let report = k.step(Vec::new());
    assert!(
        !report.touched.contains(&b),
        "a body at rest writes nothing: vel {} {} {}",
        num(&k, b, "vel.x"),
        num(&k, b, "vel.y"),
        num(&k, b, "vel.z")
    );
}

#[test]
fn boxes_stop_bodies_and_balls_trade_momentum() {
    let mut k = Kernel::new(Law::default(), 2);
    spawn(
        &mut k,
        &[
            ("shape", Value::Text("box".into())),
            ("pos.x", Value::Num(0.0)),
            ("pos.y", Value::Num(0.0)),
            ("pos.z", Value::Num(1.0)),
            ("scale.x", Value::Num(4.0)),
            ("scale.y", Value::Num(4.0)),
            ("scale.z", Value::Num(2.0)),
            ("rot.z", Value::Num(0.6)),
        ],
    );
    let on_box = spawn(&mut k, &ball(0.5, 0.2, 6.0));
    run(&mut k, 80);
    assert!(
        (num(&k, on_box, "pos.z") - 2.5).abs() < 1e-6,
        "the ball rests on top of the box: {}",
        num(&k, on_box, "pos.z")
    );
    let mut a = ball(10.0, 0.0, 0.5);
    a.push(("vel.x", Value::Num(8.0)));
    let a = spawn(&mut k, &a);
    let b = spawn(&mut k, &ball(14.0, 0.0, 0.5));
    let momentum = |k: &Kernel| num(k, a, "vel.x") + num(k, b, "vel.x");
    run(&mut k, 12);
    assert!(
        num(&k, b, "pos.x") > 14.2,
        "the hit ball moves away: {}",
        num(&k, b, "pos.x")
    );
    assert!(
        momentum(&k) <= 8.0 + 1e-9,
        "collisions never create momentum: {}",
        momentum(&k)
    );
}

#[test]
fn only_the_first_bodies_in_the_law_move() {
    let mut k = Kernel::new(
        Law {
            max_bodies: 1,
            ..Law::default()
        },
        3,
    );
    let first = spawn(&mut k, &ball(0.0, 0.0, 10.0));
    let second = spawn(&mut k, &ball(5.0, 0.0, 10.0));
    run(&mut k, 3);
    assert!(num(&k, first, "pos.z") < 10.0);
    assert_eq!(num(&k, second, "pos.z"), 10.0);
}
