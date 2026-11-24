use std::collections::BTreeMap;

use aether_kernel::{Action, Actor, Fault, Grant, Input, InputOutcome, Kernel, Law, TickReport};
use aether_world::{CapsuleId, EntityId, Value};

const CAPSULES: [(&str, &str); 4] = [
    ("bounce", include_str!("../../../capsules/bounce.wat")),
    ("greedy", include_str!("../../../capsules/greedy.wat")),
    ("thief", include_str!("../../../capsules/thief.wat")),
    ("dice", include_str!("../../../capsules/dice.wat")),
];

fn install(k: &mut Kernel, src: &str, grants: Vec<Grant>) -> CapsuleId {
    let wasm = k.add_blob(src.as_bytes().to_vec());
    let report = k.step(vec![Input {
        actor: Actor::Admin,
        action: Action::Install {
            wasm,
            grants,
            budget: Law::default().ceiling,
        },
    }]);
    match report.inputs[0] {
        InputOutcome::Installed(id) => id,
        ref other => panic!("install failed: {other:?}"),
    }
}

fn num(k: &Kernel, e: EntityId, key: &str) -> String {
    match k.world().entity(e).and_then(|e| e.components.get(key)) {
        Some(Value::Num(n)) => format!("{n:>5.1}"),
        _ => "    -".into(),
    }
}

fn describe(report: &TickReport, id: CapsuleId) -> String {
    match report.runs.iter().find(|r| r.capsule == id) {
        None => "suspended".into(),
        Some(run) => match &run.fault {
            None => format!("ok ({} fuel)", run.fuel_used),
            Some(Fault::OutOfFuel) => format!(
                "out of fuel{}",
                if run.suspended { " -> suspended" } else { "" }
            ),
            Some(Fault::Denied(why)) => format!(
                "denied: {why}{}",
                if run.suspended { " -> suspended" } else { "" }
            ),
            Some(Fault::DepthLimit) => "call depth limit".into(),
            Some(Fault::Trap(why)) => format!("trap: {why}"),
        },
    }
}

fn main() {
    let mut k = Kernel::new(Law::default(), 2026);
    let src = BTreeMap::from(CAPSULES);

    let door = match &k
        .step(vec![Input {
            actor: Actor::Admin,
            action: Action::Spawn {
                components: BTreeMap::from([("open".to_owned(), Value::Num(0.0))]),
            },
        }])
        .inputs[0]
    {
        InputOutcome::Spawned(id) => *id,
        other => panic!("{other:?}"),
    };
    let bounce = install(&mut k, src["bounce"], vec![]);
    let dice = install(&mut k, src["dice"], vec![]);
    let greedy = install(&mut k, src["greedy"], vec![]);
    let thief = install(
        &mut k,
        src["thief"],
        vec![Grant {
            entity: door,
            write: false,
        }],
    );

    let ball = {
        let state = &k.state().capsules[&bounce];
        state.clist[1].expect("bounce spawned its ball").entity
    };
    let dice_self = k.state().capsules[&dice].self_entity;

    println!("tick | ball.x | dice | door.open | greedy                      | thief");
    for _ in 0..8 {
        let r = k.step(Vec::new());
        println!(
            "{:>4} | {} | {} | {}     | {:<27} | {}",
            r.tick,
            num(&k, ball, "pos.x"),
            num(&k, dice_self, "roll"),
            num(&k, door, "open"),
            describe(&r, greedy),
            describe(&r, thief),
        );
    }

    let (replayed, hashes) =
        Kernel::replay(k.journal(), k.blobs().clone()).expect("journal replays");
    println!();
    println!("ticks in journal : {}", k.journal().ticks.len());
    println!("live state hash  : {}", k.state_hash());
    println!(
        "replay state hash: {}",
        hashes.last().expect("ticks were run")
    );
    println!("replay identical : {}", replayed.state() == k.state());
}
