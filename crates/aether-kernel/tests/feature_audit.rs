use std::collections::BTreeMap;

use aether_kernel::{
    Action, Actor, Budget, Grant, Input, InputOutcome, Kernel, Law, Stake, TickReport,
};
use aether_world::{CapsuleId, EntityId, Owner, PlayerId, Value};

const HOUSE: &str = include_str!("../../../capsules/house.wat");

fn player(name: &str) -> Actor {
    Actor::Player(PlayerId(name.into()))
}

fn inp(actor: Actor, action: Action) -> Input {
    Input { actor, action }
}

fn act(k: &mut Kernel, actor: Actor, action: Action) -> (InputOutcome, TickReport) {
    let r = k.step(vec![inp(actor, action)]);
    (r.inputs[0].clone(), r)
}

fn install(k: &mut Kernel, actor: Actor, src: &str, grants: Vec<Grant>) -> CapsuleId {
    let wasm = k.add_blob(src.as_bytes().to_vec());
    match act(
        k,
        actor,
        Action::Install {
            wasm,
            grants,
            budget: Law::default().ceiling,
        },
    )
    .0
    {
        InputOutcome::Installed(id) => id,
        other => panic!("install failed: {other:?}"),
    }
}

fn thing(k: &mut Kernel, who: &str) -> EntityId {
    let components = BTreeMap::from([("gem".to_owned(), Value::Text(who.into()))]);
    match act(k, player(who), Action::Spawn { components }).0 {
        InputOutcome::Spawned(id) => id,
        other => panic!("spawn failed: {other:?}"),
    }
}

fn pledge(k: &mut Kernel, who: &str, covenant: CapsuleId, stake: Stake) {
    let outcome = act(k, player(who), Action::Pledge { covenant, stake }).0;
    assert_eq!(outcome, InputOutcome::Applied);
}

fn num(k: &Kernel, id: EntityId, key: &str) -> Option<f64> {
    match k.world().entity(id)?.components.get(key)? {
        Value::Num(v) => Some(*v),
        _ => None,
    }
}

fn capsule_owned_entity(k: &Kernel, capsule: CapsuleId) -> EntityId {
    k.world()
        .entities()
        .find(|(_, e)| e.owner == Owner::Capsule(capsule))
        .map(|(id, _)| id)
        .expect("the house owns entities")
}

const AWARDER: &str = r#"(module
  (import "world" "self" (func $self (result i64)))
  (import "world" "signal_find" (func $signal_find (param i32 i32 i64 i32) (result i32)))
  (import "world" "signal_value" (func $signal_value (param i32) (result f64)))
  (import "world" "award" (func $award (param i32 i64 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "player:budi")
  (func (export "tick") (local $s i32)
    (local.set $s (i32.const -1))
    (block $done
      (loop $next
        (local.set $s (call $signal_find (i32.const 0) (i32.const 0) (call $self) (local.get $s)))
        (br_if $done (i32.lt_s (local.get $s) (i32.const 0)))
        (drop (call $award (i32.const 0) (i64.trunc_sat_f64_s (call $signal_value (local.get $s))) (i32.const 0) (i32.const 11)))
        (br $next)))))"#;

const WRITER: &str = r#"(module
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "stolen")
  (func (export "tick")
    (call $set (i32.const 1) (i32.const 0) (i32.const 6) (f64.const 1))))"#;

const CONDITIONAL_WRITER: &str = r#"(module
  (import "world" "entity" (func $entity (param i32) (result i64)))
  (import "world" "kind" (func $kind (param i64 i32 i32) (result i32)))
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "stolen")
  (data (i32.const 8) "go")
  (func (export "tick")
    (if (i32.eq (call $kind (call $entity (i32.const 0)) (i32.const 8) (i32.const 2)) (i32.const 1))
      (then (call $set (i32.const 1) (i32.const 0) (i32.const 6) (f64.const 1))))))"#;

const NEAR_WRITER: &str = r#"(module
  (import "world" "near" (func $near (param i32 i32 i64 f64 f64 f64 i64) (result i64)))
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "pos.x")
  (data (i32.const 8) "stale")
  (func (export "tick")
    (drop (call $near (i32.const 0) (i32.const 0) (i64.const 0) (f64.const 0) (f64.const 0) (f64.const 2) (i64.const -1)))
    (call $set (i32.const 1) (i32.const 0) (i32.const 5) (f64.const 100))
    (if (i64.ge_s (call $near (i32.const 0) (i32.const 0) (i64.const 0) (f64.const 0) (f64.const 0) (f64.const 2) (i64.const -1)) (i64.const 0))
      (then (call $set (i32.const 0) (i32.const 8) (i32.const 5) (f64.const 1))))))"#;

fn covenant_setup() -> (Kernel, CapsuleId) {
    let law = Law {
        max_consecutive_faults: 100,
        ..Law::default()
    };
    let mut k = Kernel::new(law, 42);
    let covenant = install(&mut k, Actor::Admin, AWARDER, vec![]);
    (k, covenant)
}

#[test]
fn pledged_capsule_entities_can_be_written_through_granted_capsule_handles() {
    let (mut k, covenant) = covenant_setup();
    let house = install(&mut k, player("budi"), HOUSE, vec![]);
    let part = capsule_owned_entity(&k, house);
    let early = install(
        &mut k,
        player("budi"),
        CONDITIONAL_WRITER,
        vec![Grant {
            entity: part,
            write: true,
        }],
    );
    pledge(&mut k, "budi", covenant, Stake::Capsule(house));

    let (direct, _) = act(
        &mut k,
        player("budi"),
        Action::Set {
            entity: part,
            key: "stolen".into(),
            value: Value::Num(1.0),
        },
    );
    assert!(
        matches!(direct, InputOutcome::Rejected(_)),
        "direct writes to the pledged house should already be rejected"
    );

    let wasm = k.add_blob(WRITER.as_bytes().to_vec());
    let (late, _) = act(
        &mut k,
        player("budi"),
        Action::Install {
            wasm,
            grants: vec![Grant {
                entity: part,
                write: true,
            }],
            budget: Law::default().ceiling,
        },
    );
    assert!(
        matches!(late, InputOutcome::Rejected(ref why) if why.contains("pledged")),
        "no new write grants on a pledged house: {late:?}"
    );
    let (granted, _) = act(
        &mut k,
        player("budi"),
        Action::Grant {
            capsule: early,
            grant: Grant {
                entity: part,
                write: true,
            },
        },
    );
    assert!(matches!(granted, InputOutcome::Rejected(_)), "{granted:?}");
    let writer_self = k.state().capsules[&early].self_entity;
    act(
        &mut k,
        player("budi"),
        Action::Set {
            entity: writer_self,
            key: "go".into(),
            value: Value::Num(1.0),
        },
    );
    for _ in 0..3 {
        k.step(vec![]);
    }
    assert_eq!(
        num(&k, part, "stolen"),
        None,
        "a pledged entity must stay frozen even through a granted capsule handle"
    );
}

#[test]
fn preexisting_capsule_write_capability_survives_covenant_transfer() {
    let (mut k, covenant) = covenant_setup();
    let cheap = thing(&mut k, "budi");
    let gem = thing(&mut k, "ana");
    let writer = install(
        &mut k,
        player("ana"),
        CONDITIONAL_WRITER,
        vec![Grant {
            entity: gem,
            write: true,
        }],
    );
    let both = k.step(vec![
        inp(
            player("budi"),
            Action::Pledge {
                covenant,
                stake: Stake::Entity(cheap),
            },
        ),
        inp(
            player("ana"),
            Action::Pledge {
                covenant,
                stake: Stake::Entity(gem),
            },
        ),
    ]);
    assert_eq!(
        both.inputs,
        vec![InputOutcome::Applied, InputOutcome::Applied]
    );
    assert_eq!(
        k.world().entity(gem).unwrap().owner,
        player("budi").owner(),
        "the covenant should transfer ana's gem to budi"
    );

    let self_entity = k.state().capsules[&writer].self_entity;
    let outcome = act(
        &mut k,
        player("ana"),
        Action::Set {
            entity: self_entity,
            key: "go".into(),
            value: Value::Num(1.0),
        },
    )
    .0;
    assert_eq!(outcome, InputOutcome::Applied);
    assert_eq!(
        num(&k, gem, "stolen"),
        None,
        "transfer of ownership must invalidate capsule capabilities held by the loser"
    );
}

#[test]
fn award_ignores_the_write_budget() {
    let law = Law {
        max_consecutive_faults: 100,
        ..Law::default()
    };
    let mut k = Kernel::new(law, 42);
    let wasm = k.add_blob(AWARDER.as_bytes().to_vec());
    let outcome = act(
        &mut k,
        Actor::Admin,
        Action::Install {
            wasm,
            grants: vec![],
            budget: Budget {
                writes_per_tick: 0,
                ..Law::default().ceiling
            },
        },
    )
    .0;
    let InputOutcome::Installed(covenant) = outcome else {
        panic!("install failed: {outcome:?}")
    };
    let cheap = thing(&mut k, "budi");
    let gem = thing(&mut k, "ana");
    let r = k.step(vec![
        inp(
            player("budi"),
            Action::Pledge {
                covenant,
                stake: Stake::Entity(cheap),
            },
        ),
        inp(
            player("ana"),
            Action::Pledge {
                covenant,
                stake: Stake::Entity(gem),
            },
        ),
    ]);
    assert_eq!(r.inputs, vec![InputOutcome::Applied, InputOutcome::Applied]);
    assert_eq!(
        k.world().entity(gem).unwrap().owner,
        player("ana").owner(),
        "with writes_per_tick = 0 the award must not mutate ownership"
    );
}

#[test]
fn near_queries_see_writes_made_earlier_in_the_same_capsule_run() {
    let mut k = Kernel::new(Law::default(), 43);
    let target = thing(&mut k, "ana");
    let outcome = act(
        &mut k,
        player("ana"),
        Action::Set {
            entity: target,
            key: "pos.x".into(),
            value: Value::Num(0.0),
        },
    )
    .0;
    assert_eq!(outcome, InputOutcome::Applied);
    let probe = install(
        &mut k,
        player("ana"),
        NEAR_WRITER,
        vec![Grant {
            entity: target,
            write: true,
        }],
    );
    let self_entity = k.state().capsules[&probe].self_entity;
    assert_eq!(
        num(&k, self_entity, "stale"),
        None,
        "a write consumes budget and must invalidate the per-query near cache"
    );
}
