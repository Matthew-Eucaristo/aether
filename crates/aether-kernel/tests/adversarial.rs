use std::collections::BTreeMap;

use aether_kernel::{
    Action, Actor, Budget, Fault, Input, InputOutcome, Kernel, Law, State, Status, TickReport,
};
use aether_world::{CapsuleId, EntityId, Value};

fn budget() -> Budget {
    Law::default().ceiling
}

fn install(k: &mut Kernel, src: &str) -> CapsuleId {
    install_with(k, src, budget())
}

fn install_with(k: &mut Kernel, src: &str, budget: Budget) -> CapsuleId {
    let wasm = k.add_blob(src.as_bytes().to_vec());
    let report = k.step(vec![Input {
        actor: Actor::Admin,
        action: Action::Install {
            wasm,
            grants: vec![],
            budget,
        },
    }]);
    match report.inputs[0] {
        InputOutcome::Installed(id) => id,
        ref other => panic!("install failed: {other:?}"),
    }
}

fn num(k: &Kernel, entity: EntityId, key: &str) -> Option<f64> {
    match k.world().entity(entity)?.components.get(key)? {
        Value::Num(n) => Some(*n),
        _ => None,
    }
}

fn self_entity(k: &Kernel, id: CapsuleId) -> EntityId {
    k.state().capsules[&id].self_entity
}

fn fault_of(report: &TickReport, id: CapsuleId) -> Option<Fault> {
    report
        .runs
        .iter()
        .find(|r| r.capsule == id)
        .and_then(|r| r.fault.clone())
}

fn act(k: &mut Kernel, actor: Actor, action: Action) -> InputOutcome {
    k.step(vec![Input { actor, action }]).inputs[0].clone()
}

fn spawn_input(actor: Actor) -> Input {
    Input {
        actor,
        action: Action::Spawn {
            components: BTreeMap::new(),
        },
    }
}

fn lockstep(k: &mut Kernel, restore_at: u64, steps: u64, inputs: impl Fn(u64) -> Vec<Input>) {
    let mut twin: Option<Kernel> = None;
    let prior = k.journal().ticks.len();
    assert!(prior > 0, "lockstep expects at least one prior tick");
    let mut hashes = vec![k.state_hash()];
    for t in 0..steps {
        if t == restore_at {
            let json = serde_json::to_string(k.state()).expect("state serializes");
            let state: State = serde_json::from_str(&json).expect("state deserializes");
            twin = Some(Kernel::from_state(state, k.blobs().clone()).expect("snapshot restores"));
        }
        let input = inputs(t);
        let a = k.step(input.clone());
        if let Some(twin) = twin.as_mut() {
            let b = twin.step(input);
            assert_eq!(a, b, "restored twin diverged at tick {}", a.tick);
        }
        hashes.push(a.state_hash);
    }
    let (_, replay_hashes) = Kernel::replay(k.journal(), k.blobs().clone()).expect("replays");
    assert_eq!(
        replay_hashes.len(),
        prior - 1 + hashes.len(),
        "replay length"
    );
    assert_eq!(&replay_hashes[prior - 1..], &hashes[..], "replay diverged");
}

const RECYCLE: &str = r#"(module
  (import "world" "spawn" (func $spawn (result i32)))
  (import "world" "despawn" (func $despawn (param i32)))
  (func (export "tick")
    (call $despawn (call $spawn))))"#;

#[test]
fn transient_entities_consume_their_ids_forever() {
    let mut k = Kernel::new(Law::default(), 1);
    install(&mut k, RECYCLE);
    for _ in 0..2 {
        k.step(Vec::new());
    }
    assert_eq!(k.world().len(), 1, "only the capsule's own entity persists");
    let report = k.step(vec![spawn_input(Actor::Admin)]);
    let InputOutcome::Spawned(id) = report.inputs[0] else {
        panic!("spawn failed: {:?}", report.inputs[0]);
    };
    assert_eq!(
        id.0, 4,
        "three transient entities (ids 1, 2, 3) lived and died; the guide says an entity id is never reused"
    );
}

const GHOST_A: &str = r#"(module
  (import "world" "spawn" (func $spawn (result i32)))
  (import "world" "despawn" (func $despawn (param i32)))
  (import "world" "entity" (func $entity (param i32) (result i64)))
  (import "world" "emit" (func $emit (param i64 i32 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "use")
  (func (export "tick")
    (local $h i32)
    (local.set $h (call $spawn))
    (call $emit (call $entity (local.get $h)) (i32.const 0) (i32.const 3) (f64.const 1))
    (call $despawn (local.get $h))))"#;

const GHOST_B: &str = r#"(module
  (import "world" "spawn" (func $spawn (result i32)))
  (import "world" "entity" (func $entity (param i32) (result i64)))
  (import "world" "self" (func $self (result i64)))
  (import "world" "kind" (func $kind (param i64 i32 i32) (result i32)))
  (import "world" "get_num" (func $get (param i64 i32 i32) (result f64)))
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (import "world" "signal_find" (func $sfind (param i32 i32 i64 i32) (result i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "use")
  (data (i32.const 8) "kid")
  (data (i32.const 16) "hits")
  (func (export "tick")
    (local $i i32) (local $n f64) (local $prev f64)
    (if (i32.eqz (call $kind (call $self) (i32.const 8) (i32.const 3)))
      (then
        (call $set (i32.const 0) (i32.const 8) (i32.const 3)
          (f64.convert_i32_s (call $spawn)))))
    (local.set $i (i32.const -1))
    (block $done
      (loop $next
        (local.set $i (call $sfind (i32.const 0) (i32.const 3)
          (call $entity (i32.trunc_f64_s (call $get (call $self) (i32.const 8) (i32.const 3))))
          (local.get $i)))
        (br_if $done (i32.lt_s (local.get $i) (i32.const 0)))
        (local.set $n (f64.add (local.get $n) (f64.const 1)))
        (br $next)))
    (if (call $kind (call $self) (i32.const 16) (i32.const 4))
      (then (local.set $prev (call $get (call $self) (i32.const 16) (i32.const 4)))))
    (call $set (i32.const 0) (i32.const 16) (i32.const 4)
      (f64.add (local.get $prev) (local.get $n)))))"#;

#[test]
fn a_dead_entitys_signals_must_not_land_on_a_recycled_id() {
    let mut k = Kernel::new(Law::default(), 1);
    install(&mut k, GHOST_A);
    install(&mut k, GHOST_B);
    let b = CapsuleId(1);
    let me_b = self_entity(&k, b);
    for _ in 0..4 {
        k.step(Vec::new());
    }
    assert_eq!(
        num(&k, me_b, "hits"),
        Some(0.0),
        "capsule B never existed when A's signals were sent, yet its entity collected hits"
    );
}

const DIRTY_FILL: &str = r#"(module
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "byte")
  (data (i32.const 8) "aaaaaaaa")
  (func (export "tick")
    (call $set (i32.const 0) (i32.const 0) (i32.const 4)
      (f64.convert_i32_u (i32.load8_u (i32.const 8))))
    (memory.fill (i32.const 8) (i32.const 255) (i32.const 8))
    (memory.copy (i32.const 32) (i32.const 8) (i32.const 8))))"#;

const DIRTY_V128: &str = r#"(module
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (memory (export "memory") 1)
  (global $v (mut v128) (v128.const i32x4 1 2 3 4))
  (data (i32.const 0) "lane")
  (func (export "tick")
    (call $set (i32.const 0) (i32.const 0) (i32.const 4)
      (f64.convert_i32_s (i32x4.extract_lane 0 (global.get $v))))
    (global.set $v (v128.const i32x4 9 9 9 9))))"#;

const TRAPDIRTY: &str = r#"(module
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (import "world" "tick" (func $tick (result i64)))
  (memory (export "memory") 1)
  (global $g (mut i32) (i32.const 5))
  (data (i32.const 0) "seen")
  (data (i32.const 8) "glob")
  (func (export "tick")
    (call $set (i32.const 0) (i32.const 0) (i32.const 4)
      (f64.convert_i32_u (i32.load8_u (i32.const 100))))
    (call $set (i32.const 0) (i32.const 8) (i32.const 4)
      (f64.convert_i32_s (global.get $g)))
    (i32.store8 (i32.const 100) (i32.const 77))
    (global.set $g (i32.const 999))
    (if (i64.eq (i64.rem_u (call $tick) (i64.const 2)) (i64.const 1))
      (then unreachable))))"#;

const GROWFAIL: &str = r#"(module
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "grew")
  (data (i32.const 8) "byte")
  (func (export "tick")
    (call $set (i32.const 0) (i32.const 0) (i32.const 4)
      (f64.convert_i32_s (memory.grow (i32.const 1))))
    (call $set (i32.const 0) (i32.const 8) (i32.const 4)
      (f64.convert_i32_u (i32.load8_u (i32.const 16))))
    (i32.store8 (i32.const 16) (i32.const 42))))"#;

const OVERLAP: &str = r#"(module
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "x")
  (data (i32.const 8) "y")
  (data (i32.const 16) "abcdefgh")
  (data (i32.const 20) "XYZ")
  (data (i32.const 65534) "qq")
  (func (export "tick")
    (call $set (i32.const 0) (i32.const 0) (i32.const 1)
      (f64.convert_i32_u (i32.load8_u (i32.const 20))))
    (call $set (i32.const 0) (i32.const 8) (i32.const 1)
      (f64.convert_i32_u (i32.load8_u (i32.const 23))))
    (memory.fill (i32.const 16) (i32.const 0) (i32.const 16))
    (i32.store8 (i32.const 65534) (i32.const 1))))"#;

#[test]
fn reused_instances_survive_bulk_memory_dirtying() {
    let mut k = Kernel::new(Law::default(), 1);
    let id = install(&mut k, DIRTY_FILL);
    let me = self_entity(&k, id);
    for _ in 0..6 {
        assert_eq!(
            num(&k, me, "byte"),
            Some(97.0),
            "reset must restore the 'a'"
        );
    }
    lockstep(&mut k, 3, 6, |_| Vec::new());
    assert_eq!(num(&k, me, "byte"), Some(97.0));
}

#[test]
fn reused_instances_restore_v128_globals() {
    let mut k = Kernel::new(Law::default(), 1);
    let id = install(&mut k, DIRTY_V128);
    let me = self_entity(&k, id);
    lockstep(&mut k, 3, 6, |_| Vec::new());
    assert_eq!(num(&k, me, "lane"), Some(1.0));
}

#[test]
fn reuse_across_trap_boundaries_resets_cleanly() {
    let mut k = Kernel::new(Law::default(), 1);
    let id = install(&mut k, TRAPDIRTY);
    let me = self_entity(&k, id);
    lockstep(&mut k, 3, 8, |_| Vec::new());
    assert_eq!(num(&k, me, "seen"), Some(0.0));
    assert_eq!(num(&k, me, "glob"), Some(5.0));
}

#[test]
fn failed_growth_keeps_the_instance_and_stays_consistent() {
    let law = Law {
        ceiling: Budget {
            memory_bytes: 65_536,
            ..budget()
        },
        ..Law::default()
    };
    let mut k = Kernel::new(law, 1);
    let id = install(&mut k, GROWFAIL);
    let me = self_entity(&k, id);
    lockstep(&mut k, 3, 6, |_| Vec::new());
    assert_eq!(num(&k, me, "grew"), Some(-1.0));
    assert_eq!(num(&k, me, "byte"), Some(0.0));
}

#[test]
fn overlapping_and_edge_data_segments_reset_exactly() {
    let mut k = Kernel::new(Law::default(), 1);
    let id = install(&mut k, OVERLAP);
    let me = self_entity(&k, id);
    lockstep(&mut k, 3, 6, |_| Vec::new());
    assert_eq!(num(&k, me, "x"), Some(88.0), "'X' of the second segment");
    assert_eq!(num(&k, me, "y"), Some(104.0), "'h' survives the overlap");
}

const TABLE_DIRTY: &str = r#"(module
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (memory (export "memory") 1)
  (type $t (func (result i32)))
  (table 2 funcref)
  (elem (i32.const 0) $a $b)
  (func $a (result i32) (i32.const 42))
  (func $b (result i32) (i32.const 7))
  (data (i32.const 0) "v")
  (func (export "tick")
    (call $set (i32.const 0) (i32.const 0) (i32.const 1)
      (f64.convert_i32_s (call_indirect (type $t) (i32.const 0))))
    (table.set (i32.const 0) (ref.func $b))))"#;

const PASSIVE_INIT: &str = r#"(module
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (memory (export "memory") 1)
  (data $d "abcd")
  (data (i32.const 0) "v")
  (func (export "tick")
    (memory.init $d (i32.const 100) (i32.const 0) (i32.const 4))
    (call $set (i32.const 0) (i32.const 0) (i32.const 1)
      (f64.convert_i32_u (i32.load8_u (i32.const 100))))
    (data.drop $d)))"#;

#[test]
fn table_mutating_modules_stay_deterministic() {
    let mut k = Kernel::new(Law::default(), 1);
    let id = install(&mut k, TABLE_DIRTY);
    let me = self_entity(&k, id);
    lockstep(&mut k, 3, 8, |_| Vec::new());
    assert_eq!(
        num(&k, me, "v"),
        Some(42.0),
        "slot 0 must be re-initialised to $a every tick"
    );
    let report = k.step(Vec::new());
    assert_eq!(fault_of(&report, id), None);
}

#[test]
fn passive_data_segments_stay_deterministic() {
    let mut k = Kernel::new(Law::default(), 1);
    let id = install(&mut k, PASSIVE_INIT);
    let me = self_entity(&k, id);
    lockstep(&mut k, 3, 8, |_| Vec::new());
    assert_eq!(num(&k, me, "v"), Some(97.0));
    let report = k.step(Vec::new());
    assert_eq!(
        fault_of(&report, id),
        None,
        "memory.init after data.drop must never trap: every tick is a fresh instance"
    );
}

const V1: &str = r#"(module
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "v")
  (data (i32.const 8) "byte")
  (func (export "tick")
    (call $set (i32.const 0) (i32.const 0) (i32.const 1) (f64.const 1))
    (call $set (i32.const 0) (i32.const 8) (i32.const 4)
      (f64.convert_i32_u (i32.load8_u (i32.const 32))))
    (i32.store8 (i32.const 32) (i32.const 200))))"#;

const V2: &str = r#"(module
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "v")
  (data (i32.const 8) "byte")
  (func (export "tick")
    (call $set (i32.const 0) (i32.const 0) (i32.const 1) (f64.const 2))
    (call $set (i32.const 0) (i32.const 8) (i32.const 4)
      (f64.convert_i32_u (i32.load8_u (i32.const 32))))
    (i32.store8 (i32.const 32) (i32.const 55))))"#;

#[test]
fn upgrade_swaps_code_while_the_old_instance_is_cached() {
    let mut k = Kernel::new(Law::default(), 1);
    let id = install(&mut k, V1);
    let me = self_entity(&k, id);
    let v2 = k.add_blob(V2.as_bytes().to_vec());
    lockstep(&mut k, 2, 8, |t| {
        if t == 3 {
            vec![Input {
                actor: Actor::Admin,
                action: Action::Upgrade {
                    capsule: id,
                    wasm: v2,
                },
            }]
        } else {
            Vec::new()
        }
    });
    assert_eq!(num(&k, me, "v"), Some(2.0));
    assert_eq!(num(&k, me, "byte"), Some(0.0));
}

const ALWAYS_TRAP: &str = r#"(module
  (func (export "tick") unreachable))"#;

#[test]
fn suspend_and_resume_cycle_stays_deterministic() {
    let mut k = Kernel::new(Law::default(), 1);
    let id = install(&mut k, ALWAYS_TRAP);
    lockstep(&mut k, 2, 8, |t| {
        if t == 4 {
            vec![Input {
                actor: Actor::Admin,
                action: Action::Resume { capsule: id },
            }]
        } else {
            Vec::new()
        }
    });
    let status = k.state().capsules[&id].status;
    assert_eq!(status, Status::Suspended);
}

#[test]
fn shrinking_memory_law_below_the_module_mid_run_is_a_clean_fault() {
    let mut k = Kernel::new(Law::default(), 1);
    let id = install(&mut k, DIRTY_FILL);
    let me = self_entity(&k, id);
    let shrink_ok = Law {
        ceiling: Budget {
            memory_bytes: 131_072,
            ..budget()
        },
        ..Law::default()
    };
    let shrink_bad = Law {
        ceiling: Budget {
            memory_bytes: 32_768,
            ..budget()
        },
        ..Law::default()
    };
    lockstep(&mut k, 2, 10, |t| match t {
        3 => vec![Input {
            actor: Actor::Admin,
            action: Action::SetLaw {
                law: shrink_ok.clone(),
            },
        }],
        6 => vec![Input {
            actor: Actor::Admin,
            action: Action::SetLaw {
                law: shrink_bad.clone(),
            },
        }],
        _ => Vec::new(),
    });
    assert_eq!(k.state().capsules[&id].status, Status::Suspended);
    assert_eq!(num(&k, me, "byte"), Some(97.0), "last good write persists");
}

fn data_module(len: usize) -> String {
    format!(
        r#"(module
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 512) "{}")
  (data (i32.const 0) "x")
  (func (export "tick")
    (nop)
    (call $set (i32.const 0) (i32.const 0) (i32.const 1) (f64.const 1))))"#,
        "a".repeat(len)
    )
}

#[test]
fn instantiation_fuel_is_charged_exactly_at_the_boundary() {
    const D: u64 = 100;
    let mut k = Kernel::new(Law::default(), 1);
    let id = install_with(
        &mut k,
        &data_module(D as usize),
        Budget {
            fuel_per_tick: D + 1000,
            ..budget()
        },
    );
    let me = self_entity(&k, id);
    assert_eq!(num(&k, me, "x"), Some(1.0));

    let mut k = Kernel::new(Law::default(), 1);
    let id = install_with(
        &mut k,
        &data_module(D as usize),
        Budget {
            fuel_per_tick: D,
            ..budget()
        },
    );
    let report = k.step(Vec::new());
    assert_eq!(fault_of(&report, id), Some(Fault::OutOfFuel));
    assert_eq!(report.runs[0].fuel_used, D);

    let mut k = Kernel::new(Law::default(), 1);
    let id = install_with(
        &mut k,
        &data_module(D as usize),
        Budget {
            fuel_per_tick: D - 1,
            ..budget()
        },
    );
    let me = self_entity(&k, id);
    assert_eq!(num(&k, me, "x"), None, "the tick never ran");
    let report = k.step(Vec::new());
    assert_eq!(fault_of(&report, id), Some(Fault::OutOfFuel));
    assert_eq!(report.runs[0].fuel_used, D - 1);
    assert_eq!(num(&k, me, "x"), None);
}

#[test]
fn modules_with_unsatisfiable_or_mistyped_imports_are_rejected_at_install() {
    let mut k = Kernel::new(Law::default(), 1);
    let cases = [
        r#"(module (import "world" "nuke" (func)) (func (export "tick")))"#,
        r#"(module (import "world" "sin" (func (param i32))) (func (export "tick")))"#,
        r#"(module (import "world" "g" (global i32)) (func (export "tick")))"#,
        r#"(module (import "world" "m" (memory 1)) (func (export "tick")))"#,
        r#"(module (import "world" "t" (table 1 funcref)) (func (export "tick")))"#,
    ];
    for src in cases {
        let wasm = k.add_blob(src.as_bytes().to_vec());
        let report = k.step(vec![Input {
            actor: Actor::Admin,
            action: Action::Install {
                wasm,
                grants: vec![],
                budget: budget(),
            },
        }]);
        assert!(
            matches!(report.inputs[0], InputOutcome::Rejected(_)),
            "{src} -> {:?}",
            report.inputs[0]
        );
    }
    assert!(k.state().capsules.is_empty());
}

#[test]
fn self_exported_reserved_names_disable_reuse_but_stay_deterministic() {
    let mut k = Kernel::new(Law::default(), 1);
    let src = r#"(module
      (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
      (memory 1)
      (export "memory" (memory 0))
      (export "aether:memory" (memory 0))
      (global $g (mut i32) (i32.const 3))
      (data (i32.const 0) "g")
      (func (export "tick")
        (call $set (i32.const 0) (i32.const 0) (i32.const 1)
          (f64.convert_i32_s (global.get $g)))
        (global.set $g (i32.const 88))))"#;
    let id = install(&mut k, src);
    let me = self_entity(&k, id);
    lockstep(&mut k, 3, 6, |_| Vec::new());
    assert_eq!(num(&k, me, "g"), Some(3.0));
}

#[test]
fn extended_const_expressions_are_outside_the_enabled_features() {
    let mut k = Kernel::new(Law::default(), 1);
    let src = r#"(module
      (global $g (mut i32) (i32.add (i32.const 1) (i32.const 2)))
      (func (export "tick")))"#;
    let wasm = k.add_blob(src.as_bytes().to_vec());
    let report = k.step(vec![Input {
        actor: Actor::Admin,
        action: Action::Install {
            wasm,
            grants: vec![],
            budget: budget(),
        },
    }]);
    assert!(
        matches!(report.inputs[0], InputOutcome::Rejected(_)),
        "{:?}",
        report.inputs[0]
    );
}

fn hostile(body: &str) -> String {
    format!(
        r#"(module
  (import "world" "self" (func $self (result i64)))
  (import "world" "entity" (func $entity (param i32) (result i64)))
  (import "world" "exists" (func $exists (param i64) (result i32)))
  (import "world" "kind" (func $kind (param i64 i32 i32) (result i32)))
  (import "world" "get_num" (func $get_num (param i64 i32 i32) (result f64)))
  (import "world" "get_bool" (func $get_bool (param i64 i32 i32) (result i32)))
  (import "world" "get_text" (func $get_text (param i64 i32 i32 i32 i32) (result i32)))
  (import "world" "get_nums" (func $get_nums (param i64 i32 i32 i32 i32) (result i32)))
  (import "world" "owner" (func $owner (param i64 i32 i32) (result i32)))
  (import "world" "find" (func $find (param i32 i32 i64) (result i64)))
  (import "world" "set_num" (func $set_num (param i32 i32 i32 f64)))
  (import "world" "set_text" (func $set_text (param i32 i32 i32 i32 i32)))
  (import "world" "set_bool" (func $set_bool (param i32 i32 i32 i32)))
  (import "world" "set_nums" (func $set_nums (param i32 i32 i32 i32 i32)))
  (import "world" "remove" (func $remove (param i32 i32 i32) (result i32)))
  (import "world" "despawn" (func $despawn (param i32)))
  (import "world" "signal_find" (func $signal_find (param i32 i32 i64 i32) (result i32)))
  (import "world" "signal_entity" (func $signal_entity (param i32) (result i64)))
  (import "world" "signal_value" (func $signal_value (param i32) (result f64)))
  (import "world" "signal_name" (func $signal_name (param i32 i32 i32) (result i32)))
  (import "world" "signal_from" (func $signal_from (param i32 i32 i32) (result i32)))
  (import "world" "emit" (func $emit (param i64 i32 i32 f64)))
  (import "world" "installer" (func $installer (param i64 i32 i32) (result i32)))
  (import "world" "log" (func $log (param i32 i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "k")
  (data (i32.const 8) "use")
  (func (export "tick") {body}))"#
    )
}

fn hostile_cases() -> Vec<(&'static str, String, bool)> {
    let c: Vec<(&str, &str, bool)> = vec![
        (
            "kind ptr negative",
            r"(drop (call $kind (call $self) (i32.const -1) (i32.const 1)))",
            true,
        ),
        (
            "kind len negative",
            r"(drop (call $kind (call $self) (i32.const 0) (i32.const -1)))",
            true,
        ),
        (
            "kind len huge",
            r"(drop (call $kind (call $self) (i32.const 0) (i32.const 2147483647)))",
            true,
        ),
        (
            "kind ptr huge",
            r"(drop (call $kind (call $self) (i32.const 2147483647) (i32.const 1)))",
            true,
        ),
        (
            "kind ptr+len wrap",
            r"(drop (call $kind (call $self) (i32.const 2147483640) (i32.const 16)))",
            true,
        ),
        (
            "get_num ptr negative",
            r"(drop (call $get_num (call $self) (i32.const -1) (i32.const 1)))",
            true,
        ),
        (
            "get_bool len negative",
            r"(drop (call $get_bool (call $self) (i32.const 0) (i32.const -1)))",
            true,
        ),
        (
            "get_text key len huge",
            r"(drop (call $get_text (call $self) (i32.const 0) (i32.const 2147483647) (i32.const 100) (i32.const 64)))",
            true,
        ),
        (
            "get_text buf ptr negative",
            r"(drop (call $get_text (call $self) (i32.const 0) (i32.const 1) (i32.const -1) (i32.const 64)))",
            true,
        ),
        (
            "get_text buf len negative",
            r"(drop (call $get_text (call $self) (i32.const 0) (i32.const 1) (i32.const 100) (i32.const -1)))",
            true,
        ),
        (
            "get_text buf ptr huge",
            r"(drop (call $get_text (call $self) (i32.const 0) (i32.const 1) (i32.const 2147483647) (i32.const 64)))",
            true,
        ),
        (
            "get_nums key ptr negative",
            r"(drop (call $get_nums (call $self) (i32.const -1) (i32.const 1) (i32.const 100) (i32.const 8)))",
            true,
        ),
        (
            "get_nums max negative",
            r"(drop (call $get_nums (call $self) (i32.const 0) (i32.const 1) (i32.const 100) (i32.const -1)))",
            true,
        ),
        (
            "get_nums max huge",
            r"(drop (call $get_nums (call $self) (i32.const 0) (i32.const 1) (i32.const 100) (i32.const 2147483647)))",
            true,
        ),
        (
            "get_nums dst negative",
            r"(drop (call $get_nums (call $self) (i32.const 0) (i32.const 1) (i32.const -8) (i32.const 8)))",
            true,
        ),
        (
            "exists negative id",
            r"(drop (call $exists (i64.const -1)))",
            false,
        ),
        (
            "exists huge id",
            r"(drop (call $exists (i64.const 9223372036854775807)))",
            false,
        ),
        (
            "kind negative id",
            r"(drop (call $kind (i64.const -1) (i32.const 0) (i32.const 1)))",
            false,
        ),
        (
            "set_num key ptr negative",
            r"(call $set_num (i32.const 0) (i32.const -1) (i32.const 1) (f64.const 1))",
            true,
        ),
        (
            "set_num key len negative",
            r"(call $set_num (i32.const 0) (i32.const 0) (i32.const -1) (f64.const 1))",
            true,
        ),
        (
            "set_num handle negative",
            r"(call $set_num (i32.const -1) (i32.const 0) (i32.const 1) (f64.const 1))",
            true,
        ),
        (
            "set_num handle huge",
            r"(call $set_num (i32.const 2147483647) (i32.const 0) (i32.const 1) (f64.const 1))",
            true,
        ),
        (
            "set_text text ptr negative",
            r"(call $set_text (i32.const 0) (i32.const 0) (i32.const 1) (i32.const -1) (i32.const 3))",
            true,
        ),
        (
            "set_text text len negative",
            r"(call $set_text (i32.const 0) (i32.const 0) (i32.const 1) (i32.const 100) (i32.const -1))",
            true,
        ),
        (
            "set_text text len over max",
            r"(call $set_text (i32.const 0) (i32.const 0) (i32.const 1) (i32.const 100) (i32.const 4097))",
            true,
        ),
        (
            "set_text len huge drains fuel",
            r"(call $set_text (i32.const 0) (i32.const 0) (i32.const 1) (i32.const 100) (i32.const 2147483647))",
            true,
        ),
        (
            "set_nums count negative",
            r"(call $set_nums (i32.const 0) (i32.const 0) (i32.const 1) (i32.const 100) (i32.const -1))",
            true,
        ),
        (
            "set_nums count huge drains fuel",
            r"(call $set_nums (i32.const 0) (i32.const 0) (i32.const 1) (i32.const 100) (i32.const 268435456))",
            true,
        ),
        (
            "set_nums src negative",
            r"(call $set_nums (i32.const 0) (i32.const 0) (i32.const 1) (i32.const -16) (i32.const 2))",
            true,
        ),
        (
            "set_bool ptr negative",
            r"(call $set_bool (i32.const 0) (i32.const -1) (i32.const 1) (i32.const 1))",
            true,
        ),
        (
            "remove ptr negative",
            r"(drop (call $remove (i32.const 0) (i32.const -1) (i32.const 1)))",
            true,
        ),
        (
            "despawn negative handle",
            r"(call $despawn (i32.const -1))",
            true,
        ),
        (
            "despawn huge handle",
            r"(call $despawn (i32.const 2147483647))",
            true,
        ),
        (
            "entity negative handle",
            r"(drop (call $entity (i32.const -1)))",
            true,
        ),
        (
            "signal_entity negative index",
            r"(drop (call $signal_entity (i32.const -1)))",
            true,
        ),
        (
            "signal_entity huge index",
            r"(drop (call $signal_entity (i32.const 2147483647)))",
            true,
        ),
        (
            "signal_value negative index",
            r"(drop (call $signal_value (i32.const -1)))",
            true,
        ),
        (
            "signal_find ptr negative",
            r"(drop (call $signal_find (i32.const -1) (i32.const 3) (i64.const -1) (i32.const -1)))",
            true,
        ),
        (
            "signal_find len negative",
            r"(drop (call $signal_find (i32.const 8) (i32.const -1) (i64.const -1) (i32.const -1)))",
            true,
        ),
        (
            "signal_find after huge",
            r"(drop (call $signal_find (i32.const 8) (i32.const 3) (i64.const -1) (i32.const 2147483646)))",
            false,
        ),
        (
            "signal_find entity negative means any",
            r"(drop (call $signal_find (i32.const 8) (i32.const 3) (i64.const -5) (i32.const -1)))",
            false,
        ),
        (
            "emit name ptr negative",
            r"(call $emit (call $self) (i32.const -1) (i32.const 3) (f64.const 1))",
            true,
        ),
        (
            "emit name len negative",
            r"(call $emit (call $self) (i32.const 8) (i32.const -1) (f64.const 1))",
            true,
        ),
        (
            "emit to negative entity",
            r"(call $emit (i64.const -9) (i32.const 8) (i32.const 3) (f64.const 1))",
            true,
        ),
        (
            "emit to missing entity",
            r"(call $emit (i64.const 99999) (i32.const 8) (i32.const 3) (f64.const 1))",
            true,
        ),
        (
            "owner buf ptr negative",
            r"(drop (call $owner (call $self) (i32.const -1) (i32.const 64)))",
            true,
        ),
        (
            "owner buf len negative",
            r"(drop (call $owner (call $self) (i32.const 100) (i32.const -1)))",
            false,
        ),
        (
            "owner buf ptr huge",
            r"(drop (call $owner (call $self) (i32.const 2147483647) (i32.const 64)))",
            true,
        ),
        (
            "installer negative capsule",
            r"(drop (call $installer (i64.const -1) (i32.const 100) (i32.const 64)))",
            false,
        ),
        (
            "installer missing capsule",
            r"(drop (call $installer (i64.const 777) (i32.const 100) (i32.const 64)))",
            false,
        ),
        (
            "installer buf ptr huge",
            r"(drop (call $installer (i64.const 0) (i32.const 2147483647) (i32.const 64)))",
            true,
        ),
        (
            "log ptr negative",
            r"(call $log (i32.const -1) (i32.const 4))",
            true,
        ),
        (
            "log len negative",
            r"(call $log (i32.const 0) (i32.const -1))",
            true,
        ),
        (
            "log len huge drains fuel",
            r"(call $log (i32.const 0) (i32.const 2147483647))",
            true,
        ),
        (
            "find ptr negative",
            r"(drop (call $find (i32.const -1) (i32.const 1) (i64.const -1)))",
            true,
        ),
        (
            "find after negative beyond -1",
            r"(drop (call $find (i32.const 0) (i32.const 1) (i64.const -100)))",
            false,
        ),
    ];
    c.into_iter()
        .map(|(name, body, fault)| (name, hostile(body), fault))
        .collect()
}

#[test]
fn hostile_arguments_never_panic_the_kernel() {
    for (name, src, expect_fault) in hostile_cases() {
        let mut k = Kernel::new(Law::default(), 1);
        let id = install(&mut k, &src);
        let report = k.step(Vec::new());
        let fault = fault_of(&report, id);
        if expect_fault {
            assert!(fault.is_some(), "{name}: expected a fault, ran clean");
        } else {
            assert_eq!(fault, None, "{name}: unexpected fault");
        }
        let next = k.step(Vec::new());
        let _ = fault_of(&next, id);
    }
}

#[test]
fn hostile_arguments_in_signal_buffers_with_a_live_signal() {
    for (name, idx_body) in [
        (
            "signal_name buf ptr negative",
            "(drop (call $signal_name (i32.const 0) (i32.const -1) (i32.const 64)))",
        ),
        (
            "signal_name buf len negative",
            "(drop (call $signal_name (i32.const 0) (i32.const 100) (i32.const -1)))",
        ),
        (
            "signal_name buf ptr huge",
            "(drop (call $signal_name (i32.const 0) (i32.const 2147483647) (i32.const 64)))",
        ),
        (
            "signal_from buf ptr negative",
            "(drop (call $signal_from (i32.const 0) (i32.const -1) (i32.const 64)))",
        ),
        (
            "signal_from buf ptr huge",
            "(drop (call $signal_from (i32.const 0) (i32.const 2147483647) (i32.const 64)))",
        ),
    ] {
        let mut k = Kernel::new(Law::default(), 1);
        let id = install(&mut k, &hostile(idx_body));
        let me = self_entity(&k, id);
        let report = k.step(vec![Input {
            actor: Actor::Admin,
            action: Action::Signal {
                entity: me,
                name: "use".into(),
                value: 1.0,
            },
        }]);
        let fault = fault_of(&report, id);
        if name.ends_with("len negative") {
            assert_eq!(
                fault, None,
                "{name}: negative capacity must mean 'does not fit'"
            );
        } else {
            assert!(fault.is_some(), "{name}: expected a fault");
        }
    }
}

const EMIT4: &str = r#"(module
  (import "world" "self" (func $self (result i64)))
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (import "world" "emit" (func $emit (param i64 i32 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "x")
  (data (i32.const 8) "ping")
  (func (export "tick")
    (call $set (i32.const 0) (i32.const 0) (i32.const 1) (f64.const 1))
    (call $emit (call $self) (i32.const 8) (i32.const 4) (f64.const 1))
    (call $emit (call $self) (i32.const 8) (i32.const 4) (f64.const 1))
    (call $emit (call $self) (i32.const 8) (i32.const 4) (f64.const 1))))"#;

#[test]
fn running_out_of_write_budget_rolls_back_the_whole_tick() {
    let mut k = Kernel::new(Law::default(), 1);
    let id = install_with(
        &mut k,
        EMIT4,
        Budget {
            writes_per_tick: 3,
            ..budget()
        },
    );
    let me = self_entity(&k, id);
    assert_eq!(
        num(&k, me, "x"),
        None,
        "the install tick already exceeded the budget and rolled back"
    );
    let report = k.step(Vec::new());
    assert_eq!(
        fault_of(&report, id),
        Some(Fault::Denied("no writes left this tick".into()))
    );
    assert_eq!(num(&k, me, "x"), None, "rolled back again");
    assert!(report.signals.is_empty(), "{:?}", report.signals);

    let mut k = Kernel::new(Law::default(), 1);
    let src3 = r#"(module
      (import "world" "self" (func $self (result i64)))
      (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
      (import "world" "emit" (func $emit (param i64 i32 i32 f64)))
      (memory (export "memory") 1)
      (data (i32.const 0) "x")
      (data (i32.const 8) "ping")
      (func (export "tick")
        (call $set (i32.const 0) (i32.const 0) (i32.const 1) (f64.const 1))
        (call $emit (call $self) (i32.const 8) (i32.const 4) (f64.const 1))
        (call $emit (call $self) (i32.const 8) (i32.const 4) (f64.const 1))))"#;
    let id = install_with(
        &mut k,
        src3,
        Budget {
            writes_per_tick: 3,
            ..budget()
        },
    );
    let me = self_entity(&k, id);
    assert_eq!(num(&k, me, "x"), Some(1.0));
    let report = k.step(Vec::new());
    assert_eq!(fault_of(&report, id), None);
    assert_eq!(
        report.signals.len(),
        2,
        "the install tick's emits arrive now"
    );
    assert_eq!(
        k.state().signals.len(),
        2,
        "two new emits survive into next tick"
    );
}

const MUTUAL: &str = r#"(module
  (func $a (param i32)
    (if (local.get 0) (then (call $b (i32.sub (local.get 0) (i32.const 1))))))
  (func $b (param i32) (local i64) (local i64) (local i64) (local f64) (local f64)
    (if (local.get 0) (then (call $a (i32.sub (local.get 0) (i32.const 1))))))
  (func (export "tick") (call $a (i32.const 1_000_000))))"#;

const DIRECT_SMALL: &str = r#"(module
  (func $dive (param i32)
    (if (local.get 0) (then (call $dive (i32.sub (local.get 0) (i32.const 1))))))
  (func (export "tick") (call $dive (i32.const 100_000))))"#;

#[test]
fn mutual_recursion_across_frame_sizes_hits_the_depth_limit() {
    let mut k = Kernel::new(Law::default(), 1);
    let id = install(&mut k, MUTUAL);
    let report = k.step(Vec::new());
    assert_eq!(fault_of(&report, id), Some(Fault::DepthLimit));
}

#[test]
fn deep_flat_recursion_hits_depth_limit_not_native_overflow() {
    let mut k = Kernel::new(Law::default(), 1);
    let id = install(&mut k, DIRECT_SMALL);
    let report = k.step(Vec::new());
    assert_eq!(fault_of(&report, id), Some(Fault::DepthLimit));
}

#[test]
fn the_max_frame_boundary_is_exact() {
    let ok = format!(
        "(module (func (export \"tick\") (local {})))",
        vec!["i64"; 16_374].join(" ")
    );
    let too_big = format!(
        "(module (func (export \"tick\") (local {})))",
        vec!["i64"; 16_375].join(" ")
    );
    let mut k = Kernel::new(Law::default(), 1);
    let wasm = k.add_blob(ok.as_bytes().to_vec());
    let report = k.step(vec![Input {
        actor: Actor::Admin,
        action: Action::Install {
            wasm,
            grants: vec![],
            budget: budget(),
        },
    }]);
    assert!(
        matches!(report.inputs[0], InputOutcome::Installed(_)),
        "{:?}",
        report.inputs[0]
    );
    let wasm = k.add_blob(too_big.as_bytes().to_vec());
    let report = k.step(vec![Input {
        actor: Actor::Admin,
        action: Action::Install {
            wasm,
            grants: vec![],
            budget: budget(),
        },
    }]);
    assert!(
        matches!(report.inputs[0], InputOutcome::Rejected(_)),
        "{:?}",
        report.inputs[0]
    );
}

const MATH_EDGE: &str = r#"(module
  (import "world" "sin" (func $sin (param f64) (result f64)))
  (import "world" "cos" (func $cos (param f64) (result f64)))
  (import "world" "atan2" (func $atan2 (param f64 f64) (result f64)))
  (import "world" "exp" (func $exp (param f64) (result f64)))
  (import "world" "ln" (func $ln (param f64) (result f64)))
  (import "world" "pow" (func $pow (param f64 f64) (result f64)))
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "r0")
  (data (i32.const 8) "r1")
  (data (i32.const 16) "r2")
  (data (i32.const 24) "r3")
  (data (i32.const 32) "r4")
  (data (i32.const 40) "r5")
  (func (export "tick")
    (call $set (i32.const 0) (i32.const 0) (i32.const 2) (call $sin (f64.const 1e300)))
    (call $set (i32.const 0) (i32.const 8) (i32.const 2) (call $exp (f64.const -745)))
    (call $set (i32.const 0) (i32.const 16) (i32.const 2) (call $ln (f64.const 5e-324)))
    (call $set (i32.const 0) (i32.const 24) (i32.const 2) (call $pow (f64.const 2) (f64.const 0.5)))
    (call $set (i32.const 0) (i32.const 32) (i32.const 2) (call $atan2 (f64.const -0) (f64.const 1)))
    (call $set (i32.const 0) (i32.const 40) (i32.const 2) (call $cos (f64.const 6.283185307179586)))))"#;

#[test]
fn math_edge_results_are_bit_identical_across_replay_and_restore() {
    let mut k = Kernel::new(Law::default(), 1);
    let id = install(&mut k, MATH_EDGE);
    let me = self_entity(&k, id);
    lockstep(&mut k, 3, 6, |_| Vec::new());
    let bits = |key| match k.world().entity(me).unwrap().components.get(key) {
        Some(Value::Num(n)) => n.to_bits(),
        other => panic!("{key}: {other:?}"),
    };
    assert_eq!(
        bits("r1"),
        1u64,
        "exp(-745) rounds to the smallest subnormal"
    );
    assert_eq!(bits("r4"), 0x8000000000000000u64, "atan2(-0, 1) is -0.0");
    assert_eq!(bits("r3"), 0x3ff6a09e667f3bcdu64, "sqrt(2)");
}

#[test]
fn number_array_length_limit_is_exact() {
    let big = format!(
        r#"(module
      (import "world" "set_nums" (func $set_nums (param i32 i32 i32 i32 i32)))
      (memory (export "memory") 130)
      (data (i32.const 0) "nums")
      (func (export "tick")
        (call $set_nums (i32.const 0) (i32.const 0) (i32.const 4) (i32.const 1024) (i32.const {}))))"#,
        1 << 20
    );
    let mut k = Kernel::new(Law::default(), 1);
    let id = install(&mut k, &big);
    let report = k.step(Vec::new());
    assert_eq!(fault_of(&report, id), None, "exactly 2^20 numbers fit");
    let me = self_entity(&k, id);
    match k.world().entity(me).unwrap().components.get("nums") {
        Some(Value::Nums(n)) => assert_eq!(n.len(), 1 << 20),
        other => panic!("{other:?}"),
    }

    let over = big.replace("(i32.const 1048576)", "(i32.const 1048577)");
    let mut k = Kernel::new(Law::default(), 1);
    let id = install(&mut k, &over);
    let report = k.step(Vec::new());
    assert_eq!(
        fault_of(&report, id),
        Some(Fault::Denied("guest string is too long".into()))
    );
}

#[test]
fn key_and_text_length_limits_are_exact() {
    let key64 = "k".repeat(64);
    let src = format!(
        r#"(module
      (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
      (import "world" "set_text" (func $set_text (param i32 i32 i32 i32 i32)))
      (memory (export "memory") 1)
      (data (i32.const 0) "{key64}")
      (data (i32.const 128) "{}")
      (func (export "tick")
        (call $set (i32.const 0) (i32.const 0) (i32.const 64) (f64.const 1))
        (call $set_text (i32.const 0) (i32.const 0) (i32.const 64) (i32.const 128) (i32.const 4096))))"#,
        "t".repeat(4096)
    );
    let mut k = Kernel::new(Law::default(), 1);
    let id = install(&mut k, &src);
    let report = k.step(Vec::new());
    assert_eq!(
        fault_of(&report, id),
        None,
        "64-byte key and 4096-byte text fit"
    );

    let mut k = Kernel::new(Law::default(), 1);
    let src = src.replacen(
        &format!("(data (i32.const 0) \"{key64}\")"),
        &format!("(data (i32.const 0) \"{}z\")", "k".repeat(64)),
        1,
    );
    let src = src.replace(
        "(call $set (i32.const 0) (i32.const 0) (i32.const 64) (f64.const 1))",
        "(call $set (i32.const 0) (i32.const 0) (i32.const 65) (f64.const 1))",
    );
    let id = install(&mut k, &src);
    let report = k.step(Vec::new());
    assert_eq!(
        fault_of(&report, id),
        Some(Fault::Denied("guest string is too long".into()))
    );

    let mut k = Kernel::new(Law::default(), 1);
    let src = format!(
        r#"(module
      (import "world" "set_text" (func $set_text (param i32 i32 i32 i32 i32)))
      (memory (export "memory") 1)
      (data (i32.const 0) "k")
      (data (i32.const 128) "{}")
      (func (export "tick")
        (call $set_text (i32.const 0) (i32.const 0) (i32.const 1) (i32.const 128) (i32.const 4097))))"#,
        "t".repeat(4097)
    );
    let id = install(&mut k, &src);
    let report = k.step(Vec::new());
    assert_eq!(
        fault_of(&report, id),
        Some(Fault::Denied("guest string is too long".into()))
    );
}

const HANDLE_CHURN: &str = r#"(module
  (import "world" "self" (func $self (result i64)))
  (import "world" "spawn" (func $spawn (result i32)))
  (import "world" "despawn" (func $despawn (param i32)))
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (import "world" "get_num" (func $get (param i64 i32 i32) (result f64)))
  (import "world" "kind" (func $kind (param i64 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "h")
  (data (i32.const 8) "phase")
  (data (i32.const 16) "probe")
  (func (export "tick")
    (local $h i32)
    (if (i32.eqz (call $kind (call $self) (i32.const 0) (i32.const 1)))
      (then
        (call $set (i32.const 0) (i32.const 0) (i32.const 1)
          (f64.convert_i32_s (call $spawn)))))
    (if (call $kind (call $self) (i32.const 8) (i32.const 5))
      (then
        (local.set $h (i32.trunc_f64_s (call $get (call $self) (i32.const 0) (i32.const 1))))
        (call $despawn (local.get $h))
        (call $set (local.get $h) (i32.const 16) (i32.const 5) (f64.const 1)))
      (else
        (call $set (i32.const 0) (i32.const 8) (i32.const 5) (f64.const 1))))))"#;

#[test]
fn writing_through_a_stale_handle_faults_and_rolls_back() {
    let mut k = Kernel::new(Law::default(), 1);
    let id = install(&mut k, HANDLE_CHURN);
    let me = self_entity(&k, id);
    assert_eq!(num(&k, me, "phase"), Some(1.0), "install tick initialises");
    let report = k.step(Vec::new());
    assert_eq!(
        fault_of(&report, id),
        Some(Fault::Denied(
            "handle 1 is not in this capsule's capability list".into()
        )),
        "the despawned slot is gone"
    );
    assert_eq!(num(&k, me, "probe"), None, "the stale write never landed");
    assert_eq!(num(&k, me, "phase"), Some(1.0), "nothing new was committed");
}

#[test]
fn duplicate_grants_for_one_entity_share_writes_safely() {
    let mut k = Kernel::new(Law::default(), 1);
    let door = match act(
        &mut k,
        Actor::Admin,
        Action::Spawn {
            components: BTreeMap::from([("open".to_owned(), Value::Num(0.0))]),
        },
    ) {
        InputOutcome::Spawned(id) => id,
        other => panic!("{other:?}"),
    };
    let src = r#"(module
      (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
      (memory (export "memory") 1)
      (data (i32.const 0) "open")
      (func (export "tick")
        (call $set (i32.const 1) (i32.const 0) (i32.const 4) (f64.const 1))
        (call $set (i32.const 2) (i32.const 0) (i32.const 4) (f64.const 2))))"#;
    let wasm = k.add_blob(src.as_bytes().to_vec());
    let report = k.step(vec![Input {
        actor: Actor::Admin,
        action: Action::Install {
            wasm,
            grants: vec![
                aether_kernel::Grant {
                    entity: door,
                    write: true,
                },
                aether_kernel::Grant {
                    entity: door,
                    write: true,
                },
            ],
            budget: budget(),
        },
    }]);
    let InputOutcome::Installed(id) = report.inputs[0] else {
        panic!("{:?}", report.inputs[0])
    };
    assert_eq!(fault_of(&report, id), None);
    assert_eq!(num(&k, door, "open"), Some(2.0), "handle 2 wrote last");
}

#[test]
fn a_grant_on_an_entity_despawned_the_same_tick_faults_cleanly() {
    let mut k = Kernel::new(Law::default(), 1);
    let ana = Actor::Player(aether_world::PlayerId("ana".into()));
    let house = match act(
        &mut k,
        ana.clone(),
        Action::Spawn {
            components: BTreeMap::from([("hp".to_owned(), Value::Num(10.0))]),
        },
    ) {
        InputOutcome::Spawned(id) => id,
        other => panic!("{other:?}"),
    };
    let src = r#"(module
      (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
      (memory (export "memory") 1)
      (data (i32.const 0) "hp")
      (func (export "tick")
        (call $set (i32.const 1) (i32.const 0) (i32.const 2) (f64.const 0))))"#;
    let wasm = k.add_blob(src.as_bytes().to_vec());
    let report = k.step(vec![Input {
        actor: ana.clone(),
        action: Action::Install {
            wasm,
            grants: vec![aether_kernel::Grant {
                entity: house,
                write: true,
            }],
            budget: budget(),
        },
    }]);
    let InputOutcome::Installed(id) = report.inputs[0] else {
        panic!("{:?}", report.inputs[0])
    };
    assert_eq!(fault_of(&report, id), None);
    let report = k.step(vec![Input {
        actor: ana.clone(),
        action: Action::Despawn { entity: house },
    }]);
    assert!(
        matches!(fault_of(&report, id), Some(Fault::Denied(_))),
        "{:?}",
        fault_of(&report, id)
    );
    assert!(k.world().entity(house).is_none());
}

const SOMETIMES_GROW: &str = r#"(module
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (import "world" "tick" (func $tick (result i64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "byte")
  (data (i32.const 8) "size")
  (func (export "tick")
    (call $set (i32.const 0) (i32.const 0) (i32.const 4)
      (f64.convert_i32_u (i32.load8_u (i32.const 64))))
    (i32.store8 (i32.const 64) (i32.const 33))
    (if (i64.eq (i64.rem_u (call $tick) (i64.const 2)) (i64.const 1))
      (then (drop (memory.grow (i32.const 1)))))
    (call $set (i32.const 0) (i32.const 8) (i32.const 4)
      (f64.convert_i32_s (memory.size)))))"#;

#[test]
fn alternating_growth_cycles_the_cache_without_diverging() {
    let law = Law {
        ceiling: Budget {
            memory_bytes: 1 << 20,
            ..budget()
        },
        ..Law::default()
    };
    let mut k = Kernel::new(law, 1);
    let id = install(&mut k, SOMETIMES_GROW);
    let me = self_entity(&k, id);
    lockstep(&mut k, 3, 10, |_| Vec::new());
    assert_eq!(
        num(&k, me, "byte"),
        Some(0.0),
        "memory starts zeroed each tick"
    );
}

const MULTIPAGE: &str = r#"(module
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (memory (export "memory") 3)
  (data (i32.const 0) "b0")
  (data (i32.const 8) "b1")
  (data (i32.const 16) "b2")
  (data (i32.const 70000) "\ff")
  (func (export "tick")
    (call $set (i32.const 0) (i32.const 0) (i32.const 2)
      (f64.convert_i32_u (i32.load8_u (i32.const 100))))
    (call $set (i32.const 0) (i32.const 8) (i32.const 2)
      (f64.convert_i32_u (i32.load8_u (i32.const 70000))))
    (call $set (i32.const 0) (i32.const 16) (i32.const 2)
      (f64.convert_i32_u (i32.load8_u (i32.const 131071))))
    (i32.store8 (i32.const 100) (i32.const 11))
    (i32.store8 (i32.const 70000) (i32.const 22))
    (i32.store8 (i32.const 131071) (i32.const 33))))"#;

#[test]
fn multi_page_memories_reset_fully() {
    let mut k = Kernel::new(Law::default(), 1);
    let id = install(&mut k, MULTIPAGE);
    let me = self_entity(&k, id);
    lockstep(&mut k, 3, 6, |_| Vec::new());
    assert_eq!(num(&k, me, "b0"), Some(0.0));
    assert_eq!(
        num(&k, me, "b1"),
        Some(255.0),
        "data segment byte on page 2"
    );
    assert_eq!(num(&k, me, "b2"), Some(0.0), "page 3 starts zeroed");
}

const NAN_GLOBAL: &str = r#"(module
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (memory (export "memory") 1)
  (global $g (mut f64) (f64.const nan:0x123456789abcd))
  (data (i32.const 0) "lo")
  (func (export "tick")
    (call $set (i32.const 0) (i32.const 0) (i32.const 2)
      (f64.convert_i32_u (i32.wrap_i64 (i64.reinterpret_f64 (global.get $g)))))
    (global.set $g (f64.const nan:0xdeadbeef))))"#;

#[test]
fn nan_payloads_in_globals_survive_reset_bit_exactly() {
    let mut k = Kernel::new(Law::default(), 1);
    let id = install(&mut k, NAN_GLOBAL);
    let me = self_entity(&k, id);
    lockstep(&mut k, 3, 6, |_| Vec::new());
    let v = num(&k, me, "lo").expect("lo");
    assert_eq!(
        v, 0x6789abcdu32 as f64,
        "low 32 bits of the NaN payload are intact"
    );
}

const WRITER: &str = r#"(module
  (import "world" "self" (func $self (result i64)))
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "x")
  (func (export "tick")
    (call $set (i32.const 0) (i32.const 0) (i32.const 1) (f64.const 42))))"#;

const READER: &str = r#"(module
  (import "world" "self" (func $self (result i64)))
  (import "world" "find" (func $find (param i32 i32 i64) (result i64)))
  (import "world" "get_num" (func $get (param i64 i32 i32) (result f64)))
  (import "world" "kind" (func $kind (param i64 i32 i32) (result i32)))
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "x")
  (data (i32.const 8) "seen")
  (func (export "tick")
    (local $id i64)
    (local.set $id (call $find (i32.const 0) (i32.const 1) (i64.const -1)))
    (if (i64.ge_s (local.get $id) (i64.const 0))
      (then
        (if (call $kind (local.get $id) (i32.const 0) (i32.const 1))
          (then
            (call $set (i32.const 0) (i32.const 8) (i32.const 4)
              (call $get (local.get $id) (i32.const 0) (i32.const 1)))))))))"#;

#[test]
fn later_capsules_see_earlier_capsules_writes_in_the_same_tick() {
    let mut k = Kernel::new(Law::default(), 1);
    install(&mut k, WRITER);
    let reader = install(&mut k, READER);
    let me = self_entity(&k, reader);
    assert_eq!(
        num(&k, me, "seen"),
        Some(42.0),
        "the reader runs after the writer and sees x"
    );
    lockstep(&mut k, 2, 5, |_| Vec::new());
}

#[test]
fn inputs_apply_in_their_given_order() {
    let mut k = Kernel::new(Law::default(), 1);
    let report = k.step(vec![
        spawn_input(Actor::Admin),
        Input {
            actor: Actor::Admin,
            action: Action::Set {
                entity: EntityId(0),
                key: "x".into(),
                value: Value::Num(1.0),
            },
        },
        Input {
            actor: Actor::Admin,
            action: Action::Set {
                entity: EntityId(0),
                key: "x".into(),
                value: Value::Num(2.0),
            },
        },
    ]);
    assert!(matches!(report.inputs[0], InputOutcome::Spawned(_)));
    assert_eq!(num(&k, EntityId(0), "x"), Some(2.0), "last write wins");
}

const IDLE: &str = r#"(module
  (memory (export "memory") 1)
  (func (export "tick")))"#;

use aether_kernel::{Grant, Sender};
use aether_world::PlayerId;

fn player(name: &str) -> Actor {
    Actor::Player(PlayerId(name.into()))
}

fn grant_action(capsule: CapsuleId, entity: EntityId, write: bool) -> Input {
    Input {
        actor: player("ana"),
        action: Action::Grant {
            capsule,
            grant: Grant { entity, write },
        },
    }
}

#[test]
fn a_player_cannot_grant_write_on_another_players_entity() {
    let mut k = Kernel::new(Law::default(), 1);
    let spawned = act(
        &mut k,
        player("bob"),
        Action::Spawn {
            components: BTreeMap::new(),
        },
    );
    let InputOutcome::Spawned(entity) = spawned else {
        panic!("spawn failed: {spawned:?}");
    };
    let wasm = k.add_blob(IDLE.as_bytes().to_vec());
    let report = k.step(vec![Input {
        actor: player("ana"),
        action: Action::Install {
            wasm,
            grants: vec![Grant {
                entity,
                write: true,
            }],
            budget: budget(),
        },
    }]);
    assert!(
        matches!(report.inputs[0], InputOutcome::Rejected(_)),
        "install must be rejected, got {:?}",
        report.inputs[0]
    );
    let id = install(&mut k, IDLE);
    let outcome = k.step(vec![Input {
        actor: player("ana"),
        action: Action::Grant {
            capsule: id,
            grant: Grant {
                entity,
                write: true,
            },
        },
    }]);
    assert!(
        matches!(outcome.inputs[0], InputOutcome::Rejected(_)),
        "grant must be rejected, got {:?}",
        outcome.inputs[0]
    );
}

#[test]
fn grant_rights_follow_the_installer_not_the_grantee() {
    let mut k = Kernel::new(Law::default(), 1);
    let bob_capsule = {
        let wasm = k.add_blob(IDLE.as_bytes().to_vec());
        let report = k.step(vec![Input {
            actor: player("bob"),
            action: Action::Install {
                wasm,
                grants: vec![],
                budget: budget(),
            },
        }]);
        match report.inputs[0] {
            InputOutcome::Installed(id) => id,
            ref other => panic!("install failed: {other:?}"),
        }
    };
    let bob_entity = self_entity(&k, bob_capsule);
    let ana_capsule = {
        let wasm = k.add_blob(IDLE.as_bytes().to_vec());
        let report = k.step(vec![Input {
            actor: player("ana"),
            action: Action::Install {
                wasm,
                grants: vec![],
                budget: budget(),
            },
        }]);
        match report.inputs[0] {
            InputOutcome::Installed(id) => id,
            ref other => panic!("install failed: {other:?}"),
        }
    };
    let ana_entity = self_entity(&k, ana_capsule);
    let outcome = k.step(vec![grant_action(ana_capsule, ana_entity, true)]);
    assert!(
        matches!(outcome.inputs[0], InputOutcome::Granted(_)),
        "own capsule entity grant must succeed, got {:?}",
        outcome.inputs[0]
    );
    let outcome = k.step(vec![grant_action(ana_capsule, bob_entity, true)]);
    assert!(
        matches!(outcome.inputs[0], InputOutcome::Rejected(_)),
        "foreign capsule entity grant must be rejected, got {:?}",
        outcome.inputs[0]
    );
}

const REAPER: &str = r#"(module
  (import "world" "despawn" (func $despawn (param i32)))
  (import "world" "self" (func $self (result i64)))
  (memory (export "memory") 1)
  (func (export "tick")
    (call $despawn (i32.const 1))))"#;

#[test]
fn a_write_grant_does_not_allow_despawning_the_granted_entity() {
    let mut k = Kernel::new(Law::default(), 1);
    let spawned = act(
        &mut k,
        Actor::Admin,
        Action::Spawn {
            components: BTreeMap::new(),
        },
    );
    let InputOutcome::Spawned(entity) = spawned else {
        panic!("spawn failed: {spawned:?}");
    };
    let wasm = k.add_blob(REAPER.as_bytes().to_vec());
    let report = k.step(vec![Input {
        actor: Actor::Admin,
        action: Action::Install {
            wasm,
            grants: vec![Grant {
                entity,
                write: true,
            }],
            budget: budget(),
        },
    }]);
    let InputOutcome::Installed(id) = report.inputs[0] else {
        panic!("install failed: {:?}", report.inputs[0]);
    };
    let report = k.step(Vec::new());
    match fault_of(&report, id) {
        Some(Fault::Denied(ref m)) => assert_eq!(
            m,
            "handle 1 refers to an entity this capsule did not create"
        ),
        other => panic!("expected a host fault, got {other:?}"),
    }
    assert!(
        k.world().entity(entity).is_some(),
        "the granted entity survived the despawn attempt"
    );
}

const PROBER: &str = r#"(module
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "x")
  (func (export "tick")
    (call $set (i32.const 1) (i32.const 0) (i32.const 1) (f64.const 7))))"#;

#[test]
fn a_revoked_handle_never_writes_again() {
    let mut k = Kernel::new(Law::default(), 1);
    let spawned = act(
        &mut k,
        Actor::Admin,
        Action::Spawn {
            components: BTreeMap::new(),
        },
    );
    let InputOutcome::Spawned(entity) = spawned else {
        panic!("spawn failed: {spawned:?}");
    };
    let wasm = k.add_blob(PROBER.as_bytes().to_vec());
    let report = k.step(vec![Input {
        actor: Actor::Admin,
        action: Action::Install {
            wasm,
            grants: vec![Grant {
                entity,
                write: true,
            }],
            budget: budget(),
        },
    }]);
    let InputOutcome::Installed(id) = report.inputs[0] else {
        panic!("install failed: {:?}", report.inputs[0]);
    };
    let report = k.step(Vec::new());
    assert_eq!(
        fault_of(&report, id),
        None,
        "write through live handle works"
    );
    assert_eq!(num(&k, entity, "x"), Some(7.0));
    let outcome = k.step(vec![Input {
        actor: Actor::Admin,
        action: Action::Revoke {
            capsule: id,
            handle: 1,
        },
    }]);
    assert!(
        matches!(outcome.inputs[0], InputOutcome::Applied),
        "revoke applies: {:?}",
        outcome.inputs[0]
    );
    let report = k.step(Vec::new());
    match fault_of(&report, id) {
        Some(Fault::Denied(ref m)) => {
            assert_eq!(m, "handle 1 is not in this capsule's capability list")
        }
        other => panic!("expected a host fault, got {other:?}"),
    }
    assert_eq!(num(&k, entity, "x"), Some(7.0), "no further write landed");
}

const SNITCH: &str = r#"(module
  (import "world" "self" (func $self (result i64)))
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (import "world" "set_text" (func $set_text (param i32 i32 i32 i32 i32)))
  (import "world" "signal_find" (func $find (param i32 i32 i64 i32) (result i32)))
  (import "world" "signal_from" (func $from (param i32 i32 i32) (result i32)))
  (import "world" "signal_entity" (func $sigent (param i32) (result i64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "hit")
  (data (i32.const 16) "from")
  (func (export "tick")
    (local $i i32)
    (local $n i32)
    (local.set $i (call $find (i32.const 0) (i32.const 3) (i64.const -1) (i32.const -1)))
    (if (i32.ge_s (local.get $i) (i32.const 0))
      (then
        (call $set (i32.const 0) (i32.const 0) (i32.const 3)
          (f64.convert_i64_s (call $sigent (local.get $i))))
        (local.set $n (call $from (local.get $i) (i32.const 64) (i32.const 64)))
        (call $set_text (i32.const 0) (i32.const 16) (i32.const 4)
          (i32.const 64) (local.get $n))))))"#;

const EMITTER: &str = r#"(module
  (import "world" "emit" (func $emit (param i64 i32 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "hit")
  (func (export "tick")
    (call $emit (i64.const 0) (i32.const 0) (i32.const 3) (f64.const 1))))"#;

#[test]
fn signal_from_always_names_the_emitting_capsule() {
    let mut k = Kernel::new(Law::default(), 1);
    let snitch = install(&mut k, SNITCH);
    let emitter = install(&mut k, EMITTER);
    let snitch_entity = self_entity(&k, snitch);
    let target = snitch_entity;
    let src = EMITTER.replace("(i64.const 0)", &format!("(i64.const {})", target.0));
    let wasm = k.add_blob(src.as_bytes().to_vec());
    let report = k.step(vec![Input {
        actor: Actor::Admin,
        action: Action::Upgrade {
            capsule: emitter,
            wasm,
        },
    }]);
    assert!(matches!(report.inputs[0], InputOutcome::Applied));
    k.step(Vec::new());
    k.step(Vec::new());
    assert_eq!(
        num(&k, snitch_entity, "hit"),
        Some(target.0 as f64),
        "signal landed on its target"
    );
    match k
        .world()
        .entity(snitch_entity)
        .and_then(|e| e.components.get("from"))
    {
        Some(Value::Text(t)) => assert_eq!(t, &format!("capsule:{}", emitter.0)),
        other => panic!("expected sender label text, got {other:?}"),
    }
    let outcome = k.step(vec![Input {
        actor: Actor::Admin,
        action: Action::Uninstall { capsule: emitter },
    }]);
    assert!(matches!(outcome.inputs[0], InputOutcome::Applied));
    k.step(Vec::new());
    let report = k.step(vec![Input {
        actor: player("ana"),
        action: Action::Signal {
            entity: target,
            name: "hit".into(),
            value: 2.0,
        },
    }]);
    assert!(matches!(report.inputs[0], InputOutcome::Applied));
    k.step(Vec::new());
    match k
        .world()
        .entity(snitch_entity)
        .and_then(|e| e.components.get("from"))
    {
        Some(Value::Text(t)) => assert_eq!(t, "player:ana", "actor signal labelled correctly"),
        other => panic!("expected sender label text, got {other:?}"),
    }
}

const PEEKER: &str = r#"(module
  (import "world" "self" (func $self (result i64)))
  (import "world" "get_num" (func $get (param i64 i32 i32) (result f64)))
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "secret")
  (data (i32.const 16) "seen")
  (func (export "tick" ) (param)
    (call $set (i32.const 0) (i32.const 16) (i32.const 4)
      (call $get (i64.const 0) (i32.const 0) (i32.const 6)))))"#;

#[test]
fn reading_is_public_by_contract() {
    let mut k = Kernel::new(Law::default(), 1);
    let spawned = act(
        &mut k,
        Actor::Admin,
        Action::Spawn {
            components: BTreeMap::new(),
        },
    );
    let InputOutcome::Spawned(entity) = spawned else {
        panic!("spawn failed: {spawned:?}");
    };
    act(
        &mut k,
        Actor::Admin,
        Action::Set {
            entity,
            key: "secret".into(),
            value: Value::Num(99.0),
        },
    );
    let src = PEEKER.replace("(i64.const 0)", &format!("(i64.const {})", entity.0));
    let wasm = k.add_blob(src.as_bytes().to_vec());
    let report = k.step(vec![Input {
        actor: Actor::Admin,
        action: Action::Install {
            wasm,
            grants: vec![],
            budget: budget(),
        },
    }]);
    let InputOutcome::Installed(id) = report.inputs[0] else {
        panic!("install failed: {:?}", report.inputs[0]);
    };
    let me = self_entity(&k, id);
    assert_eq!(
        num(&k, me, "seen"),
        Some(99.0),
        "reads by raw id need no grant"
    );
}

#[test]
fn inputs_beyond_the_per_actor_limit_are_rejected() {
    let mut k = Kernel::new(Law::default(), 1);
    let inputs: Vec<Input> = (0..40).map(|_| spawn_input(player("ana"))).collect();
    let report = k.step(inputs);
    let ok = report
        .inputs
        .iter()
        .filter(|o| matches!(o, InputOutcome::Spawned(_)))
        .count();
    let rejected = report
        .inputs
        .iter()
        .filter(|o| matches!(o, InputOutcome::Rejected(_)))
        .count();
    assert_eq!(ok, 32, "the first 32 inputs apply");
    assert_eq!(rejected, 8, "inputs 33..40 are rejected");
}

#[test]
fn key_length_charset_and_text_length_limits_hold() {
    let mut k = Kernel::new(Law::default(), 1);
    let spawned = act(
        &mut k,
        Actor::Admin,
        Action::Spawn {
            components: BTreeMap::new(),
        },
    );
    let InputOutcome::Spawned(entity) = spawned else {
        panic!("spawn failed: {spawned:?}");
    };
    let mut set =
        |key: String, value: Value| act(&mut k, Actor::Admin, Action::Set { entity, key, value });
    let key64: String = "a".repeat(64);
    let key65: String = "a".repeat(65);
    assert_eq!(set(key64.clone(), Value::Num(1.0)), InputOutcome::Applied);
    assert!(
        matches!(set(key65, Value::Num(1.0)), InputOutcome::Rejected(_)),
        "65-byte key rejected"
    );
    assert!(
        matches!(
            set(String::new(), Value::Num(1.0)),
            InputOutcome::Rejected(_)
        ),
        "empty key rejected"
    );
    assert!(
        matches!(
            set("bad key".into(), Value::Num(1.0)),
            InputOutcome::Rejected(_)
        ),
        "space in key rejected"
    );
    assert!(
        matches!(
            set("bad/key".into(), Value::Num(1.0)),
            InputOutcome::Rejected(_)
        ),
        "slash in key rejected"
    );
    let text_ok: String = "x".repeat(4096);
    let text_big: String = "x".repeat(4097);
    assert_eq!(set("t".into(), Value::Text(text_ok)), InputOutcome::Applied);
    assert!(
        matches!(
            set("t".into(), Value::Text(text_big)),
            InputOutcome::Rejected(_)
        ),
        "4097-byte text rejected"
    );
    assert!(
        matches!(
            set("n".into(), Value::Num(f64::NAN)),
            InputOutcome::Rejected(_)
        ),
        "NaN rejected"
    );
    assert!(
        matches!(
            set("n".into(), Value::Num(f64::INFINITY)),
            InputOutcome::Rejected(_)
        ),
        "infinity rejected"
    );
}

#[test]
fn the_region_entity_limit_holds() {
    let law = Law {
        max_entities: 5,
        ..Law::default()
    };
    let mut k = Kernel::new(law, 1);
    let mut spawned = 0;
    let mut rejected = 0;
    for _ in 0..8 {
        match act(
            &mut k,
            Actor::Admin,
            Action::Spawn {
                components: BTreeMap::new(),
            },
        ) {
            InputOutcome::Spawned(_) => spawned += 1,
            InputOutcome::Rejected(_) => rejected += 1,
            other => panic!("unexpected: {other:?}"),
        }
    }
    assert_eq!(spawned, 5);
    assert_eq!(rejected, 3);
}

const SPINNER: &str = r#"(module
  (memory (export "memory") 1)
  (func (export "tick")
    (loop (br 0))))"#;

#[test]
fn the_fuel_floor_reaches_every_capsule_in_a_crowded_tick() {
    let law = Law {
        region_fuel_per_tick: 1_000_000,
        max_consecutive_faults: 1_000,
        ..Law::default()
    };
    let mut k = Kernel::new(law, 1);
    const COUNT: usize = 8;
    let mut ids = Vec::new();
    for _ in 0..COUNT {
        ids.push(install(&mut k, SPINNER));
    }
    let report = k.step(Vec::new());
    assert_eq!(report.runs.len(), COUNT);
    let total: u64 = report.runs.iter().map(|r| r.fuel_used).sum();
    assert!(
        total <= 1_000_000,
        "the region never burns more than its budget: {total}"
    );
    for (i, run) in report.runs.iter().enumerate() {
        assert_eq!(run.fault, Some(Fault::OutOfFuel), "capsule {i} spins out");
        assert!(
            run.fuel_used >= 125_000,
            "capsule {i} starved: {} < floor",
            run.fuel_used
        );
    }
    lockstep(&mut k, 2, 4, |_| Vec::new());
}

const SPAMMER: &str = r#"(module
  (import "world" "emit" (func $emit (param i64 i32 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "s")
  (func (export "tick")
    (call $emit (i64.const 0) (i32.const 0) (i32.const 1) (f64.const 1))
    (call $emit (i64.const 0) (i32.const 0) (i32.const 1) (f64.const 2))
    (call $emit (i64.const 0) (i32.const 0) (i32.const 1) (f64.const 3))
    (call $emit (i64.const 0) (i32.const 0) (i32.const 1) (f64.const 4))))"#;

#[test]
fn signal_floods_are_capped_by_the_write_budget() {
    let mut k = Kernel::new(Law::default(), 1);
    let spawned = act(
        &mut k,
        Actor::Admin,
        Action::Spawn {
            components: BTreeMap::new(),
        },
    );
    let InputOutcome::Spawned(target) = spawned else {
        panic!("spawn failed: {spawned:?}");
    };
    let src = SPAMMER.replace("(i64.const 0)", &format!("(i64.const {})", target.0));
    let wasm = k.add_blob(src.as_bytes().to_vec());
    let report = k.step(vec![Input {
        actor: Actor::Admin,
        action: Action::Install {
            wasm,
            grants: vec![],
            budget: Budget {
                writes_per_tick: 2,
                ..budget()
            },
        },
    }]);
    let InputOutcome::Installed(id) = report.inputs[0] else {
        panic!("install failed: {:?}", report.inputs[0]);
    };
    let report = k.step(Vec::new());
    match fault_of(&report, id) {
        Some(Fault::Denied(ref m)) => assert_eq!(m, "no writes left this tick"),
        other => panic!("expected write budget fault, got {other:?}"),
    }
    assert!(
        report.signals.iter().all(|s| s.from != Sender::Capsule(id)),
        "no partial signal leak: {:?}",
        report.signals
    );
}

#[test]
fn spawn_limits_are_enforced_per_tick() {
    let law = Law {
        max_entities: 1_000,
        ..Law::default()
    };
    let mut k = Kernel::new(law, 1);
    const SPAWNER: &str = r#"(module
      (import "world" "spawn" (func $spawn (result i32)))
      (memory (export "memory") 1)
      (func (export "tick")
        (drop (call $spawn))
        (drop (call $spawn))
        (drop (call $spawn))
        (drop (call $spawn))))"#;
    let wasm = k.add_blob(SPAWNER.as_bytes().to_vec());
    let report = k.step(vec![Input {
        actor: Actor::Admin,
        action: Action::Install {
            wasm,
            grants: vec![],
            budget: Budget {
                spawns_per_tick: 2,
                ..budget()
            },
        },
    }]);
    let InputOutcome::Installed(id) = report.inputs[0] else {
        panic!("install failed: {:?}", report.inputs[0]);
    };
    match fault_of(&report, id) {
        Some(Fault::Denied(ref m)) => {
            assert_eq!(m, "no spawns left this tick under the region law")
        }
        other => panic!("expected spawn budget fault, got {other:?}"),
    }
    let n = k.world().entities().count();
    assert_eq!(
        n, 1,
        "only the spawner's own entity exists; its spawns rolled back"
    );
}
