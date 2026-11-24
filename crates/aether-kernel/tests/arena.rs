use std::collections::BTreeMap;

use aether_kernel::{Action, Actor, Grant, Input, InputOutcome, Kernel, Law, TickReport};
use aether_world::{CapsuleId, EntityId, PlayerId, Value};

const ARENA: &str = include_str!("../../../capsules/arena.wat");
const RIFLE: &str = include_str!("../../../capsules/rifle.wat");
const LAUNCHER: &str = include_str!("../../../capsules/launcher.wat");

fn player(name: &str) -> Actor {
    Actor::Player(PlayerId(name.into()))
}

fn step(k: &mut Kernel, inputs: Vec<(Actor, Action)>) -> TickReport {
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
    let r = step(
        k,
        vec![(
            actor,
            Action::Install {
                wasm,
                grants,
                budget,
            },
        )],
    );
    match &r.inputs[0] {
        InputOutcome::Installed(id) => *id,
        other => panic!("install failed: {other:?}"),
    }
}

fn spawn(k: &mut Kernel, actor: Actor, components: &[(&str, Value)]) -> EntityId {
    let components: BTreeMap<String, Value> = components
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect();
    match &step(k, vec![(actor, Action::Spawn { components })]).inputs[0] {
        InputOutcome::Spawned(id) => *id,
        other => panic!("spawn failed: {other:?}"),
    }
}

fn avatar(k: &mut Kernel, name: &str, x: f64) -> EntityId {
    spawn(
        k,
        player(name),
        &[
            ("avatar", Value::Bool(true)),
            ("pos.x", Value::Num(x)),
            ("pos.y", Value::Num(0.0)),
            ("pos.z", Value::Num(0.9)),
            ("aim.x", Value::Num(if x < 3.0 { 1.0 } else { -1.0 })),
        ],
    )
}

fn signal(entity: EntityId, name: &str, value: f64) -> Action {
    Action::Signal {
        entity,
        name: name.into(),
        value,
    }
}

fn roster(k: &Kernel, arena: CapsuleId) -> Vec<[f64; 10]> {
    let me = k.state().capsules[&arena].self_entity;
    match k.world().entity(me).unwrap().components.get("roster") {
        Some(Value::Nums(n)) => n
            .chunks(10)
            .map(|c| c.try_into().expect("ten numbers per player"))
            .collect(),
        _ => Vec::new(),
    }
}

fn entry(k: &Kernel, arena: CapsuleId, avatar: EntityId) -> [f64; 10] {
    roster(k, arena)
        .into_iter()
        .find(|e| e[0] == avatar.0 as f64)
        .expect("avatar joined the arena")
}

fn assert_clean(reports: &[TickReport]) {
    for r in reports {
        for run in &r.runs {
            assert_eq!(run.fault, None, "tick {} capsule {:?}", r.tick, run.capsule);
        }
    }
}

struct Duel {
    k: Kernel,
    arena: CapsuleId,
    ana: EntityId,
    budi: EntityId,
}

fn duel() -> Duel {
    let mut k = Kernel::new(Law::default(), 9);
    let arena = install(&mut k, Actor::Admin, ARENA, vec![]);
    let ana = avatar(&mut k, "ana", 0.0);
    let budi = avatar(&mut k, "budi", 6.0);
    let hub = k.state().capsules[&arena].self_entity;
    let r = step(
        &mut k,
        vec![
            (player("ana"), signal(hub, "join", ana.0 as f64)),
            (player("budi"), signal(hub, "join", budi.0 as f64)),
            (player("budi"), signal(hub, "join", ana.0 as f64)),
        ],
    );
    assert_clean(&[r]);
    assert_eq!(
        roster(&k, arena).len(),
        2,
        "budi cannot join with ana's avatar"
    );
    Duel {
        k,
        arena,
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

#[test]
fn rifle_fire_kills_and_is_credited() {
    let Duel {
        mut k,
        arena,
        ana,
        budi,
    } = duel();
    install(&mut k, player("ana"), RIFLE, read(ana));
    let mut reports = Vec::new();
    for _ in 0..40 {
        reports.push(step(
            &mut k,
            vec![(player("ana"), signal(ana, "fire", 1.0))],
        ));
    }
    assert_clean(&reports);
    let b = entry(&k, arena, budi);
    let a = entry(&k, arena, ana);
    assert_eq!(b[4], 1.0, "budi died once: {b:?}");
    assert_eq!(a[3], 1.0, "ana is credited with the kill: {a:?}");
    assert_eq!(a[2], 100.0, "ana was never hit");
    let fuel: u64 = reports
        .iter()
        .flat_map(|r| &r.runs)
        .filter(|r| r.capsule == arena)
        .map(|r| r.fuel_used)
        .max()
        .unwrap();
    eprintln!("MEASURE rifle: arena peak fuel {fuel}, ana {a:?}, budi {b:?}");
    assert!(fuel < 500_000, "arena peak fuel {fuel}");
}

#[test]
fn only_the_owner_can_pull_the_trigger() {
    let Duel {
        mut k,
        arena,
        ana,
        budi,
        ..
    } = duel();
    install(&mut k, player("ana"), RIFLE, read(ana));
    let mut reports = Vec::new();
    for _ in 0..20 {
        reports.push(step(
            &mut k,
            vec![(player("budi"), signal(ana, "fire", 1.0))],
        ));
    }
    assert_clean(&reports);
    assert_eq!(entry(&k, arena, budi)[2], 100.0);
    assert!(
        k.world()
            .entities()
            .all(|(_, e)| !e.components.contains_key("projectile"))
    );
}

#[test]
fn walls_stop_bullets() {
    let Duel {
        mut k,
        arena,
        ana,
        budi,
        ..
    } = duel();
    spawn(
        &mut k,
        Actor::Admin,
        &[
            ("shape", Value::Text("box".into())),
            ("pos.x", Value::Num(3.0)),
            ("pos.y", Value::Num(0.0)),
            ("pos.z", Value::Num(1.5)),
            ("scale.x", Value::Num(0.2)),
            ("scale.y", Value::Num(4.0)),
            ("scale.z", Value::Num(3.0)),
            ("rot.z", Value::Num(0.3)),
        ],
    );
    install(&mut k, player("ana"), RIFLE, read(ana));
    let mut reports = Vec::new();
    for _ in 0..40 {
        reports.push(step(
            &mut k,
            vec![(player("ana"), signal(ana, "fire", 1.0))],
        ));
    }
    assert_clean(&reports);
    assert_eq!(
        entry(&k, arena, budi)[2],
        100.0,
        "the wall took every bullet"
    );
    let bullets = k
        .world()
        .entities()
        .filter(|(_, e)| e.components.contains_key("projectile"))
        .count();
    assert!(bullets <= 3, "spent bullets are removed, {bullets} left");
}

#[test]
fn missiles_explode_with_splash_damage() {
    let Duel {
        mut k,
        arena,
        ana,
        budi,
        ..
    } = duel();
    install(&mut k, player("ana"), LAUNCHER, read(ana));
    step(
        &mut k,
        vec![(
            player("ana"),
            Action::Set {
                entity: ana,
                key: "weapon".into(),
                value: Value::Num(2.0),
            },
        )],
    );
    let mut reports = Vec::new();
    let mut flash = 0.0f64;
    for i in 0..30 {
        let fire = if i == 0 {
            vec![(player("ana"), signal(ana, "fire", 2.0))]
        } else {
            vec![]
        };
        reports.push(step(&mut k, fire));
        for (_, e) in k.world().entities() {
            if let Some(Value::Num(l)) = e.components.get("light") {
                flash = flash.max(*l);
            }
        }
    }
    assert_clean(&reports);
    let b = entry(&k, arena, budi);
    eprintln!("MEASURE missile: budi health {} peak light {flash}", b[2]);
    assert!(
        b[2] < 100.0 && b[2] > 40.0,
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
