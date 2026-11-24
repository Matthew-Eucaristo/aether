use std::collections::BTreeMap;

use aether_kernel::{
    Action, Actor, Budget, Cap, Fault, Grant, Input, InputOutcome, Kernel, KernelError, Law, RULES,
    State, Status, TickReport,
};
use aether_world::{CapsuleId, EntityId, Owner, PlayerId, Usage, Value};

macro_rules! capsule {
    ($name:literal) => {
        include_str!(concat!("../../../capsules/", $name, ".wat"))
    };
}

fn budget() -> Budget {
    Law::default().ceiling
}

fn player(name: &str) -> Actor {
    Actor::Player(PlayerId(name.into()))
}

fn try_install(k: &mut Kernel, actor: Actor, src: &str, grants: Vec<Grant>) -> InputOutcome {
    let wasm = k.add_blob(src.as_bytes().to_vec());
    let report = k.step(vec![Input {
        actor,
        action: Action::Install {
            wasm,
            grants,
            budget: budget(),
        },
    }]);
    report.inputs[0].clone()
}

fn install(k: &mut Kernel, src: &str, grants: Vec<Grant>) -> CapsuleId {
    match try_install(k, Actor::Admin, src, grants) {
        InputOutcome::Installed(id) => id,
        other => panic!("install failed: {other:?}"),
    }
}

fn spawn(k: &mut Kernel, actor: Actor, components: &[(&str, Value)]) -> EntityId {
    let components: BTreeMap<String, Value> = components
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect();
    match &k
        .step(vec![Input {
            actor,
            action: Action::Spawn { components },
        }])
        .inputs[0]
    {
        InputOutcome::Spawned(id) => *id,
        other => panic!("spawn failed: {other:?}"),
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

fn ball(k: &Kernel, bounce: CapsuleId) -> EntityId {
    let handle = num(k, self_entity(k, bounce), "ball").expect("bounce stored its ball handle");
    k.state().capsules[&bounce].clist[handle as usize]
        .expect("ball handle is live")
        .entity
}

fn run(k: &mut Kernel, ticks: usize) -> Vec<TickReport> {
    (0..ticks).map(|_| k.step(Vec::new())).collect()
}

fn fault_of(report: &TickReport, id: CapsuleId) -> Option<Fault> {
    report
        .runs
        .iter()
        .find(|r| r.capsule == id)
        .and_then(|r| r.fault.clone())
}

#[test]
fn replaying_the_journal_reproduces_every_tick_hash() {
    let mut k = Kernel::new(Law::default(), 42);
    let mut hashes = Vec::new();
    let bounce = install(&mut k, capsule!("bounce"), vec![]);
    hashes.push(k.state_hash());
    install(&mut k, capsule!("dice"), vec![]);
    hashes.push(k.state_hash());
    hashes.extend(run(&mut k, 60).into_iter().map(|r| r.state_hash));

    let xs: Vec<f64> = {
        let ball = ball(&k, bounce);
        let mut probe = Kernel::from_state(k.journal().start.clone(), k.blobs().clone()).unwrap();
        k.journal()
            .ticks
            .iter()
            .filter_map(|inputs| {
                probe.step(inputs.clone());
                num(&probe, ball, "pos.x")
            })
            .collect()
    };
    assert!(
        xs.iter().all(|x| x.abs() <= 5.5),
        "ball stays in bounds: {xs:?}"
    );
    assert!(xs.iter().any(|x| *x > 4.0) && xs.iter().any(|x| *x < -4.0));

    let (replayed, replay_hashes) = Kernel::replay(k.journal(), k.blobs().clone()).unwrap();
    assert_eq!(replay_hashes, hashes);
    assert_eq!(replayed.state(), k.state());
}

#[test]
fn restored_snapshot_continues_identically() {
    let mut a = Kernel::new(Law::default(), 7);
    install(&mut a, capsule!("bounce"), vec![]);
    install(&mut a, capsule!("dice"), vec![]);
    run(&mut a, 20);

    let json = serde_json::to_string(a.state()).unwrap();
    let restored: State = serde_json::from_str(&json).unwrap();
    let mut b = Kernel::from_state(restored, a.blobs().clone()).unwrap();
    assert_eq!(a.state_hash(), b.state_hash());

    for _ in 0..20 {
        assert_eq!(a.step(Vec::new()).state_hash, b.step(Vec::new()).state_hash);
    }
}

#[test]
fn infinite_loop_is_cut_off_then_suspended_while_others_keep_running() {
    let mut k = Kernel::new(Law::default(), 1);
    let bounce = install(&mut k, capsule!("bounce"), vec![]);
    let greedy = install(&mut k, capsule!("greedy"), vec![]);
    let ball = ball(&k, bounce);

    let start_x = num(&k, ball, "pos.x").unwrap();
    let reports = run(&mut k, 5);
    for report in &reports {
        assert_eq!(fault_of(report, bounce), None);
    }
    assert_eq!(num(&k, ball, "pos.x").unwrap(), start_x + 5.0 * 0.5);

    let greedy_runs: Vec<_> = reports
        .iter()
        .filter_map(|r| r.runs.iter().find(|run| run.capsule == greedy))
        .collect();
    assert_eq!(
        greedy_runs.len(),
        2,
        "install tick + 2 more faults reach the limit of 3"
    );
    assert!(
        greedy_runs
            .iter()
            .all(|r| r.fault == Some(Fault::OutOfFuel))
    );
    assert!(
        greedy_runs
            .iter()
            .all(|r| r.fuel_used == budget().fuel_per_tick)
    );
    assert!(greedy_runs.last().unwrap().suspended);
    assert_eq!(k.state().capsules[&greedy].status, Status::Suspended);
}

#[test]
fn bounce_moves_its_ball_every_tick() {
    let mut k = Kernel::new(Law::default(), 1);
    let bounce = install(&mut k, capsule!("bounce"), vec![]);
    let ball = ball(&k, bounce);
    let before = num(&k, ball, "pos.x").unwrap();
    run(&mut k, 1);
    assert_eq!(num(&k, ball, "pos.x").unwrap(), before + 0.5);
}

#[test]
fn write_without_capability_is_denied_and_world_unchanged() {
    let mut k = Kernel::new(Law::default(), 1);
    let door = spawn(&mut k, Actor::Admin, &[("open", Value::Num(0.0))]);

    let read_only = install(
        &mut k,
        capsule!("thief"),
        vec![Grant {
            entity: door,
            write: false,
        }],
    );
    let no_grant = install(&mut k, capsule!("thief"), vec![]);
    let report = k.step(Vec::new());

    assert_eq!(
        fault_of(&report, read_only),
        Some(Fault::Denied("handle 1 does not grant write access".into()))
    );
    assert_eq!(
        fault_of(&report, no_grant),
        Some(Fault::Denied(
            "handle 1 is not in this capsule's capability list".into()
        ))
    );
    assert_eq!(num(&k, door, "open"), Some(0.0));
}

#[test]
fn granted_write_capability_allows_the_write() {
    let mut k = Kernel::new(Law::default(), 1);
    let door = spawn(&mut k, Actor::Admin, &[("open", Value::Num(0.0))]);
    let opener = install(
        &mut k,
        capsule!("thief"),
        vec![Grant {
            entity: door,
            write: true,
        }],
    );
    let report = k.step(Vec::new());
    assert_eq!(fault_of(&report, opener), None);
    assert_eq!(num(&k, door, "open"), Some(1.0));
}

#[test]
fn faulted_tick_rolls_back_every_write_it_made() {
    let mut k = Kernel::new(Law::default(), 1);
    let id = install(&mut k, capsule!("rollback"), vec![]);
    let report = k.step(Vec::new());
    assert!(matches!(fault_of(&report, id), Some(Fault::Trap(_))));
    assert_eq!(num(&k, self_entity(&k, id), "touched"), None);
}

#[test]
fn memory_growth_beyond_budget_fails() {
    let mut k = Kernel::new(Law::default(), 1);
    let id = install(&mut k, capsule!("hog"), vec![]);
    assert_eq!(num(&k, self_entity(&k, id), "grow"), Some(-1.0));
}

#[test]
fn law_change_applies_to_running_capsules_immediately() {
    let mut k = Kernel::new(Law::default(), 1);
    let id = install(&mut k, capsule!("spawner"), vec![]);
    run(&mut k, 3);
    let before = k.world().len();
    assert_eq!(before, 1 + 4, "self entity plus one spawn per tick");

    let law = Law {
        allow_spawn: false,
        ..Law::default()
    };
    let report = k.step(vec![Input {
        actor: Actor::Admin,
        action: Action::SetLaw { law },
    }]);
    assert_eq!(
        fault_of(&report, id),
        Some(Fault::Denied(
            "no spawns left this tick under the region law".into()
        ))
    );
    assert_eq!(k.world().len(), before);
}

#[test]
fn region_entity_cap_is_enforced() {
    let law = Law {
        max_entities: 3,
        ..Law::default()
    };
    let mut k = Kernel::new(law, 1);
    let id = install(&mut k, capsule!("spawner"), vec![]);
    let reports = run(&mut k, 2);
    assert_eq!(k.world().len(), 3);
    assert_eq!(fault_of(&reports[0], id), None);
    assert_eq!(
        fault_of(&reports[1], id),
        Some(Fault::Denied(
            "the region holds its maximum number of entities".into()
        ))
    );
}

#[test]
fn players_only_change_what_they_own() {
    let mut k = Kernel::new(Law::default(), 1);
    let alice = player("alice");
    let bob = player("bob");
    let house = spawn(&mut k, alice.clone(), &[("open", Value::Num(0.0))]);

    let r = k.step(vec![
        Input {
            actor: bob.clone(),
            action: Action::Set {
                entity: house,
                key: "open".into(),
                value: Value::Num(1.0),
            },
        },
        Input {
            actor: alice.clone(),
            action: Action::Set {
                entity: house,
                key: "color".into(),
                value: Value::Text("red".into()),
            },
        },
    ]);
    assert_eq!(
        r.inputs[0],
        InputOutcome::Rejected(format!("actor may not modify entity {}", house.0))
    );
    assert_eq!(r.inputs[1], InputOutcome::Applied);

    let grant_write = vec![Grant {
        entity: house,
        write: true,
    }];
    assert_eq!(
        try_install(&mut k, bob.clone(), capsule!("thief"), grant_write.clone()),
        InputOutcome::Rejected(format!(
            "actor may not grant write access to entity {}",
            house.0
        ))
    );
    let InputOutcome::Installed(alices) =
        try_install(&mut k, alice.clone(), capsule!("thief"), grant_write)
    else {
        panic!("alice may grant write access to her own house");
    };
    assert_eq!(num(&k, house, "open"), Some(1.0));

    let r = k.step(vec![Input {
        actor: bob,
        action: Action::Uninstall { capsule: alices },
    }]);
    assert_eq!(
        r.inputs[0],
        InputOutcome::Rejected(format!("actor may not manage capsule {}", alices.0))
    );
    assert!(k.state().capsules.contains_key(&alices));
}

#[test]
fn uninstall_removes_everything_the_capsule_created() {
    let mut k = Kernel::new(Law::default(), 1);
    let id = install(&mut k, capsule!("bounce"), vec![]);
    run(&mut k, 3);
    assert_eq!(k.world().len(), 2);
    let r = k.step(vec![Input {
        actor: Actor::Admin,
        action: Action::Uninstall { capsule: id },
    }]);
    assert_eq!(r.inputs[0], InputOutcome::Applied);
    assert!(k.world().is_empty());
    assert!(k.state().capsules.is_empty());
}

#[test]
fn invalid_modules_are_rejected_at_install() {
    let mut k = Kernel::new(Law::default(), 1);
    let cases = [
        "(module (func (export \"run\")))",
        "(module (import \"os\" \"exec\" (func)) (func (export \"tick\")))",
        "(module (func (export \"tick\") (param i32)))",
        "(module (memory 1 1 shared) (func (export \"tick\")))",
        "(module (memory i64 1) (func (export \"tick\")))",
        "(module (func $s) (start $s) (func (export \"tick\")))",
        "(module (func (export \"tick\") (drop (i32x4.relaxed_trunc_f32x4_s (v128.const i32x4 0 0 0 0)))))",
        "(module (func (export \"tick\") (return_call 0)))",
        "not wasm at all",
    ];
    for src in cases {
        let outcome = try_install(&mut k, Actor::Admin, src, vec![]);
        assert!(
            matches!(outcome, InputOutcome::Rejected(_)),
            "{src} -> {outcome:?}"
        );
    }
    assert!(k.state().capsules.is_empty());
    assert!(k.world().is_empty());
}

fn recursive(prelude: &str, sig: &str, body: &str, entry: &str) -> String {
    format!(
        "(module {prelude} (type $t (func {sig})) (table 1 funcref) (elem (i32.const 0) $f) \
         (func $f {sig} {body}) (func (export \"tick\") {entry}))"
    )
}

fn repeat(n: usize, f: impl Fn(usize) -> String) -> String {
    (0..n).map(f).collect::<Vec<_>>().join(" ")
}

fn frame_shapes() -> Vec<(&'static str, String)> {
    let n = 1500;
    let i64_locals = format!(
        "(local {}) {} (call $f) {} (drop)",
        repeat(n, |_| "i64".into()),
        repeat(n, |i| format!("(local.set {i} (i64.const {i}))")),
        "(local.get 0)".to_owned()
            + &repeat(n - 1, |i| format!(" (local.get {}) (i64.add)", i + 1))
    );
    let v128_locals = format!(
        "(local {}) {} (call $f) {} (drop)",
        repeat(n, |_| "v128".into()),
        repeat(n, |i| format!(
            "(local.set {i} (v128.const i32x4 {i} 1 2 3))"
        )),
        "(local.get 0)".to_owned()
            + &repeat(n - 1, |i| format!(" (local.get {}) (v128.xor)", i + 1))
    );
    let operand_stack = format!(
        "{} (call $f) {}",
        repeat(2000, |i| format!("(f64.const {i})")),
        repeat(2000, |_| "(drop)".into())
    );
    let params = 400;
    let sig_params = format!("(param {})", repeat(params, |_| "i64".into()));
    let pass = repeat(params, |i| format!("(local.get {i})"));
    let consts = repeat(params, |i| format!("(i64.const {i})"));
    vec![
        ("minimal", recursive("", "", "(call $f)", "(call $f)")),
        ("i64 locals", recursive("", "", &i64_locals, "(call $f)")),
        ("v128 locals", recursive("", "", &v128_locals, "(call $f)")),
        (
            "operand stack",
            recursive("", "", &operand_stack, "(call $f)"),
        ),
        (
            "many params",
            recursive(
                "",
                &sig_params,
                &format!("{pass} (call $f)"),
                &format!("{consts} (call $f)"),
            ),
        ),
        (
            "indirect",
            recursive(
                "",
                "",
                &format!("{operand_stack} (call_indirect (type $t) (i32.const 0))"),
                "(call_indirect (type $t) (i32.const 0))",
            ),
        ),
    ]
}

#[test]
fn every_frame_shape_hits_the_deterministic_depth_limit_before_the_native_stack() {
    for (name, src) in frame_shapes() {
        let mut k = Kernel::new(Law::default(), 1);
        let bounce = install(&mut k, capsule!("bounce"), vec![]);
        let deep = install(&mut k, &src, vec![]);
        let report = k.step(Vec::new());
        assert_eq!(fault_of(&report, bounce), None, "{name}");
        assert_eq!(fault_of(&report, deep), Some(Fault::DepthLimit), "{name}");
    }
}

#[test]
fn bounded_recursion_within_the_limit_completes() {
    let mut k = Kernel::new(Law::default(), 1);
    let src = r#"(module
      (func $down (param i32) (if (local.get 0) (then (call $down (i32.sub (local.get 0) (i32.const 1))))))
      (func (export "tick") (call $down (i32.const 4000))))"#;
    let id = install(&mut k, src, vec![]);
    assert_eq!(fault_of(&k.step(Vec::new()), id), None);
}

#[test]
fn oversized_frames_and_reserved_exports_are_rejected() {
    let mut k = Kernel::new(Law::default(), 1);
    let huge = format!(
        "(module (func (export \"tick\") (local {})))",
        repeat(20_000, |_| "i64".into())
    );
    let reserved =
        "(module (global (export \"aether:depth\") i32 (i32.const 0)) (func (export \"tick\")))";
    for src in [huge.as_str(), reserved] {
        let outcome = try_install(&mut k, Actor::Admin, src, vec![]);
        assert!(matches!(outcome, InputOutcome::Rejected(_)), "{outcome:?}");
    }
}

#[test]
fn non_finite_numbers_never_enter_the_world() {
    let mut k = Kernel::new(Law::default(), 1);
    let src = r#"(module
      (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
      (memory (export "memory") 1)
      (data (i32.const 0) "v")
      (func (export "tick")
        (call $set (i32.const 0) (i32.const 0) (i32.const 1)
          (f64.div (f64.const 0) (f64.const 0)))))"#;
    let id = install(&mut k, src, vec![]);
    let report = k.step(Vec::new());
    assert_eq!(
        fault_of(&report, id),
        Some(Fault::Denied("numbers must be finite".into()))
    );
    assert_eq!(num(&k, self_entity(&k, id), "v"), None);
}

#[test]
fn tampered_journal_produces_a_different_hash() {
    let mut k = Kernel::new(Law::default(), 3);
    spawn(&mut k, Actor::Admin, &[("gold", Value::Num(10.0))]);
    install(&mut k, capsule!("dice"), vec![]);
    run(&mut k, 10);

    let mut forged = k.journal().clone();
    let Action::Spawn { components } = &mut forged.ticks[0][0].action else {
        panic!("first input is the spawn");
    };
    components.insert("gold".into(), Value::Num(1_000_000.0));

    let (_, honest) = Kernel::replay(k.journal(), k.blobs().clone()).unwrap();
    let (_, tampered) = Kernel::replay(&forged, k.blobs().clone()).unwrap();
    assert_eq!(honest.last(), Some(&k.state_hash()));
    assert_ne!(honest.last(), tampered.last());
}

#[test]
fn dangling_capabilities_read_as_absent_and_ids_are_never_reused() {
    let mut k = Kernel::new(Law::default(), 1);
    let door = spawn(&mut k, Actor::Admin, &[("open", Value::Num(0.0))]);
    let watcher = r#"(module
      (import "world" "entity" (func $entity (param i32) (result i64)))
      (import "world" "exists" (func $exists (param i64) (result i32)))
      (import "world" "kind" (func $kind (param i64 i32 i32) (result i32)))
      (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
      (memory (export "memory") 1)
      (data (i32.const 0) "open")
      (data (i32.const 8) "alive")
      (data (i32.const 16) "has_open")
      (func (export "tick")
        (call $set (i32.const 0) (i32.const 8) (i32.const 5)
          (f64.convert_i32_s (call $exists (call $entity (i32.const 1)))))
        (call $set (i32.const 0) (i32.const 16) (i32.const 8)
          (f64.convert_i32_s (i32.ne (i32.const 0) (call $kind (call $entity (i32.const 1)) (i32.const 0) (i32.const 4)))))))"#;
    let id = install(
        &mut k,
        watcher,
        vec![Grant {
            entity: door,
            write: false,
        }],
    );
    let me = self_entity(&k, id);
    assert_eq!(num(&k, me, "alive"), Some(1.0));
    assert_eq!(num(&k, me, "has_open"), Some(1.0));

    let report = k.step(vec![Input {
        actor: Actor::Admin,
        action: Action::Despawn { entity: door },
    }]);
    assert_eq!(fault_of(&report, id), None);
    assert_eq!(num(&k, me, "alive"), Some(0.0));
    assert_eq!(num(&k, me, "has_open"), Some(0.0));

    let fresh = spawn(&mut k, Actor::Admin, &[("open", Value::Num(1.0))]);
    assert!(fresh > door, "entity ids are never reused");
    k.step(Vec::new());
    assert_eq!(num(&k, me, "alive"), Some(0.0));
}

#[test]
fn installers_control_what_their_capsules_create() {
    let mut k = Kernel::new(Law::default(), 1);
    let ana = player("ana");
    let spawner = match try_install(&mut k, ana.clone(), capsule!("spawner"), vec![]) {
        InputOutcome::Installed(id) => id,
        other => panic!("{other:?}"),
    };
    let child = k
        .world()
        .entities()
        .find(|(id, e)| e.owner == Owner::Capsule(spawner) && *id != self_entity(&k, spawner))
        .map(|(id, _)| id)
        .expect("spawner created an entity");
    let set = |actor: Actor| Input {
        actor,
        action: Action::Set {
            entity: child,
            key: "color".into(),
            value: Value::Text("red".into()),
        },
    };
    let report = k.step(vec![set(ana.clone()), set(player("budi"))]);
    assert_eq!(report.inputs[0], InputOutcome::Applied);
    assert!(matches!(report.inputs[1], InputOutcome::Rejected(_)));

    let grant = vec![Grant {
        entity: child,
        write: true,
    }];
    assert!(matches!(
        try_install(&mut k, ana, capsule!("dice"), grant.clone()),
        InputOutcome::Installed(_)
    ));
    assert!(matches!(
        try_install(&mut k, player("budi"), capsule!("dice"), grant),
        InputOutcome::Rejected(_)
    ));
}

#[test]
fn instantiation_costs_a_fixed_fuel_per_initialized_byte_on_every_platform() {
    let plain = "(module (memory (export \"memory\") 1) (func (export \"tick\") (nop)))";
    let with_data = "(module (memory (export \"memory\") 1) (data (i32.const 0) \"ball\") (func (export \"tick\") (nop)))";
    let mut used = Vec::new();
    for src in [plain, with_data] {
        let mut k = Kernel::new(Law::default(), 1);
        let wasm = k.add_blob(src.as_bytes().to_vec());
        let tiny = Budget {
            fuel_per_tick: 10,
            ..budget()
        };
        let report = k.step(vec![Input {
            actor: Actor::Admin,
            action: Action::Install {
                wasm,
                grants: vec![],
                budget: tiny,
            },
        }]);
        let run = &report.runs[0];
        assert_eq!(run.fault, None, "{src}");
        used.push(run.fuel_used);
    }
    assert_eq!(used[1], used[0] + 4, "four data bytes cost four fuel");
    assert!(used[1] <= 10);
}

#[test]
fn bulk_memory_operations_pay_one_fuel_per_byte() {
    let mut used = Vec::new();
    for len in [0, 65_536] {
        let src = format!(
            "(module (memory 2) (func (export \"tick\") (memory.fill (i32.const 0) (i32.const 7) (i32.const {len}))))"
        );
        let mut k = Kernel::new(Law::default(), 1);
        let id = install(&mut k, &src, vec![]);
        let report = k.step(Vec::new());
        assert_eq!(fault_of(&report, id), None);
        used.push(report.runs[0].fuel_used);
    }
    assert_eq!(used[1] - used[0], 65_536);
}

#[test]
fn capsules_read_and_discover_any_entity_but_write_only_through_capabilities() {
    let mut k = Kernel::new(Law::default(), 1);
    let ana = player("ana");
    spawn(
        &mut k,
        ana.clone(),
        &[
            ("hp", Value::Num(10.0)),
            ("name", Value::Text("slime".into())),
        ],
    );
    spawn(&mut k, Actor::Admin, &[("hp", Value::Num(5.0))]);
    let scanner = r#"(module
      (import "world" "self" (func $self (result i64)))
      (import "world" "find" (func $find (param i32 i32 i64) (result i64)))
      (import "world" "kind" (func $kind (param i64 i32 i32) (result i32)))
      (import "world" "get_num" (func $get (param i64 i32 i32) (result f64)))
      (import "world" "get_bool" (func $get_bool (param i64 i32 i32) (result i32)))
      (import "world" "get_text" (func $get_text (param i64 i32 i32 i32 i32) (result i32)))
      (import "world" "owner" (func $owner (param i64 i32 i32) (result i32)))
      (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
      (import "world" "set_bool" (func $set_bool (param i32 i32 i32 i32)))
      (import "world" "set_text" (func $set_text (param i32 i32 i32 i32 i32)))
      (memory (export "memory") 1)
      (data (i32.const 0) "hp")
      (data (i32.const 8) "total")
      (data (i32.const 16) "name")
      (data (i32.const 24) "first")
      (data (i32.const 32) "first_owner")
      (data (i32.const 48) "flag")
      (data (i32.const 56) "flag_kind")
      (data (i32.const 72) "flag_value")
      (func (export "tick")
        (local $id i64) (local $sum f64) (local $n i32)
        (local.set $id (i64.const -1))
        (block $done
          (loop $next
            (local.set $id (call $find (i32.const 0) (i32.const 2) (local.get $id)))
            (br_if $done (i64.lt_s (local.get $id) (i64.const 0)))
            (local.set $sum (f64.add (local.get $sum) (call $get (local.get $id) (i32.const 0) (i32.const 2))))
            (br $next)))
        (call $set (i32.const 0) (i32.const 8) (i32.const 5) (local.get $sum))
        (local.set $id (call $find (i32.const 0) (i32.const 2) (i64.const -1)))
        (local.set $n (call $get_text (local.get $id) (i32.const 16) (i32.const 4) (i32.const 100) (i32.const 64)))
        (call $set_text (i32.const 0) (i32.const 24) (i32.const 5) (i32.const 100) (local.get $n))
        (local.set $n (call $owner (local.get $id) (i32.const 200) (i32.const 64)))
        (call $set_text (i32.const 0) (i32.const 32) (i32.const 11) (i32.const 200) (local.get $n))
        (call $set_bool (i32.const 0) (i32.const 48) (i32.const 4) (i32.const 1))
        (call $set (i32.const 0) (i32.const 56) (i32.const 9)
          (f64.convert_i32_s (call $kind (call $self) (i32.const 48) (i32.const 4))))
        (call $set (i32.const 0) (i32.const 72) (i32.const 10)
          (f64.convert_i32_s (call $get_bool (call $self) (i32.const 48) (i32.const 4))))))"#;
    let id = install(&mut k, scanner, vec![]);
    let me = self_entity(&k, id);
    let text = |k: &Kernel, key: &str| match k.world().entity(me).unwrap().components.get(key) {
        Some(Value::Text(t)) => t.clone(),
        other => panic!("{key}: {other:?}"),
    };
    assert_eq!(num(&k, me, "total"), Some(15.0));
    assert_eq!(text(&k, "first"), "slime");
    assert_eq!(text(&k, "first_owner"), "player:ana");
    assert_eq!(num(&k, me, "flag_kind"), Some(3.0));
    assert_eq!(num(&k, me, "flag_value"), Some(1.0));

    spawn(&mut k, ana, &[("hp", Value::Num(1.0))]);
    assert_eq!(num(&k, me, "total"), Some(16.0));
}

#[test]
fn player_quotas_cover_their_entities_and_their_capsules() {
    let mut k = Kernel::new(
        Law {
            player_quota: Usage {
                entities: 4,
                bytes: 4096,
            },
            ..Law::default()
        },
        1,
    );
    let ana = player("ana");
    for _ in 0..2 {
        spawn(&mut k, ana.clone(), &[]);
    }
    let big = Input {
        actor: ana.clone(),
        action: Action::Spawn {
            components: BTreeMap::from([("bio".to_owned(), Value::Text("x".repeat(4000)))]),
        },
    };
    let report = k.step(vec![big]);
    assert!(
        matches!(&report.inputs[0], InputOutcome::Rejected(r) if r.contains("bytes")),
        "{:?}",
        report.inputs[0]
    );

    let spawner = match try_install(&mut k, ana.clone(), capsule!("spawner"), vec![]) {
        InputOutcome::Installed(id) => id,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        k.player_usage(&PlayerId("ana".into())).entities,
        4,
        "two spawned, the capsule's own entity and its first spawn"
    );
    let report = k.step(Vec::new());
    assert!(
        matches!(fault_of(&report, spawner), Some(Fault::Denied(r)) if r.contains("quota")),
        "{:?}",
        fault_of(&report, spawner)
    );
    let report = k.step(vec![Input {
        actor: ana.clone(),
        action: Action::Spawn {
            components: BTreeMap::new(),
        },
    }]);
    assert!(matches!(&report.inputs[0], InputOutcome::Rejected(_)));
    assert_eq!(k.player_usage(&PlayerId("ana".into())).entities, 4);

    let budi = spawn(&mut k, player("budi"), &[]);
    assert!(k.world().entity(budi).is_some(), "quotas are per player");
    spawn(&mut k, Actor::Admin, &[]);
}

#[test]
fn every_kernel_field_changes_the_state_hash() {
    let mut k = Kernel::new(Law::default(), 1);
    let id = install(&mut k, capsule!("bounce"), vec![]);
    let base = k.state().clone();
    let blobs = k.blobs().clone();
    let hash = |s: State| Kernel::from_state(s, blobs.clone()).unwrap().state_hash();
    let original = hash(base.clone());
    assert_eq!(original, k.state_hash());

    type Edit = fn(&mut State, CapsuleId);
    let edits: Vec<(&str, Edit)> = vec![
        ("seed", |s, _| s.seed += 1),
        ("next_capsule", |s, _| s.next_capsule += 1),
        ("law.allow_spawn", |s, _| s.law.allow_spawn = false),
        ("law.max_inputs_per_actor", |s, _| {
            s.law.max_inputs_per_actor += 1
        }),
        ("law.player_quota", |s, _| s.law.player_quota.bytes += 1),
        ("law.ceiling", |s, _| s.law.ceiling.spawns_per_tick += 1),
        ("capsule.faults", |s, c| {
            s.capsules.get_mut(&c).unwrap().consecutive_faults += 1
        }),
        ("capsule.status", |s, c| {
            s.capsules.get_mut(&c).unwrap().status = Status::Suspended
        }),
        ("capsule.budget", |s, c| {
            s.capsules.get_mut(&c).unwrap().budget.fuel_per_tick += 1
        }),
        ("capsule.clist", |s, c| {
            s.capsules.get_mut(&c).unwrap().clist.push(None)
        }),
        ("capsule.installer", |s, c| {
            s.capsules.get_mut(&c).unwrap().installer = Actor::Player(PlayerId("x".into()))
        }),
    ];
    for (name, edit) in edits {
        let mut s = base.clone();
        edit(&mut s, id);
        assert_ne!(hash(s), original, "{name} does not reach the state hash");
    }
}

fn counter(step: u32) -> String {
    format!(
        r#"(module
      (import "world" "self" (func $self (result i64)))
      (import "world" "kind" (func $kind (param i64 i32 i32) (result i32)))
      (import "world" "get_num" (func $get (param i64 i32 i32) (result f64)))
      (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
      (memory (export "memory") 1)
      (data (i32.const 0) "count")
      (func (export "tick") (local $n f64)
        (if (call $kind (call $self) (i32.const 0) (i32.const 5))
          (then (local.set $n (call $get (call $self) (i32.const 0) (i32.const 5)))))
        (call $set (i32.const 0) (i32.const 0) (i32.const 5) (f64.add (local.get $n) (f64.const {step})))))"#
    )
}

fn act(k: &mut Kernel, actor: Actor, action: Action) -> InputOutcome {
    k.step(vec![Input { actor, action }]).inputs[0].clone()
}

#[test]
fn upgrade_swaps_code_and_keeps_the_capsules_state() {
    let mut k = Kernel::new(Law::default(), 1);
    let ana = player("ana");
    let id = match try_install(&mut k, ana.clone(), &counter(1), vec![]) {
        InputOutcome::Installed(id) => id,
        other => panic!("{other:?}"),
    };
    run(&mut k, 2);
    let me = self_entity(&k, id);
    assert_eq!(num(&k, me, "count"), Some(3.0));

    let v2 = k.add_blob(counter(100).into_bytes());
    let upgrade = |wasm| Action::Upgrade { capsule: id, wasm };
    assert!(matches!(
        act(&mut k, player("budi"), upgrade(v2)),
        InputOutcome::Rejected(_)
    ));
    assert_eq!(num(&k, me, "count"), Some(4.0));

    let broken = k.add_blob(b"(module)".to_vec());
    assert!(matches!(
        act(&mut k, ana.clone(), upgrade(broken)),
        InputOutcome::Rejected(_)
    ));
    assert_eq!(num(&k, me, "count"), Some(5.0), "old code keeps running");

    assert_eq!(act(&mut k, ana, upgrade(v2)), InputOutcome::Applied);
    assert_eq!(num(&k, me, "count"), Some(105.0));
    assert_eq!(self_entity(&k, id), me);
}

#[test]
fn capabilities_can_be_granted_and_revoked_after_install() {
    let mut k = Kernel::new(Law::default(), 1);
    let door = spawn(&mut k, Actor::Admin, &[("open", Value::Num(0.0))]);
    let thief = install(&mut k, capsule!("thief"), vec![]);
    let grant = |write| Action::Grant {
        capsule: thief,
        grant: Grant {
            entity: door,
            write,
        },
    };
    assert!(matches!(
        act(&mut k, player("ana"), grant(true)),
        InputOutcome::Rejected(_)
    ));
    assert_eq!(
        act(&mut k, Actor::Admin, grant(true)),
        InputOutcome::Granted(1)
    );
    assert_eq!(num(&k, door, "open"), Some(1.0));

    let report = k.step(vec![
        Input {
            actor: Actor::Admin,
            action: Action::Revoke {
                capsule: thief,
                handle: 1,
            },
        },
        Input {
            actor: Actor::Admin,
            action: Action::Set {
                entity: door,
                key: "open".into(),
                value: Value::Num(0.0),
            },
        },
    ]);
    assert_eq!(report.inputs[0], InputOutcome::Applied);
    assert!(matches!(fault_of(&report, thief), Some(Fault::Denied(_))));
    assert_eq!(num(&k, door, "open"), Some(0.0));
    assert!(matches!(
        act(
            &mut k,
            Actor::Admin,
            Action::Revoke {
                capsule: thief,
                handle: 0
            }
        ),
        InputOutcome::Rejected(_)
    ));
}

#[test]
fn rules_fingerprint() {
    let mut k = Kernel::new(Law::default(), 42);
    let door = spawn(&mut k, Actor::Admin, &[("open", Value::Num(0.0))]);
    spawn(
        &mut k,
        player("ana"),
        &[("hp", Value::Num(7.0)), ("name", Value::Text("ana".into()))],
    );
    for name in [
        capsule!("bounce"),
        capsule!("orbit"),
        capsule!("dice"),
        capsule!("greedy"),
        capsule!("deep"),
        capsule!("hog"),
        capsule!("rollback"),
        capsule!("spawner"),
    ] {
        install(&mut k, name, vec![]);
    }
    let thief = install(
        &mut k,
        capsule!("thief"),
        vec![Grant {
            entity: door,
            write: false,
        }],
    );
    let mut reports = run(&mut k, 5);
    act(
        &mut k,
        Actor::Admin,
        Action::Grant {
            capsule: thief,
            grant: Grant {
                entity: door,
                write: true,
            },
        },
    );
    let v2 = k.add_blob(counter(3).into_bytes());
    act(
        &mut k,
        Actor::Admin,
        Action::Upgrade {
            capsule: CapsuleId(2),
            wasm: v2,
        },
    );
    reports.extend(run(&mut k, 20));
    let mut h = blake3::Hasher::new();
    h.update(&k.state_hash().0);
    h.update(&serde_json::to_vec(&reports).unwrap());
    let fingerprint = h.finalize().to_hex();
    assert_eq!(
        (RULES, fingerprint.as_str()),
        (
            "aether-rules/4",
            "faa0f4d06879621cfaa6a518dc8ae47233e29e91bec269a8398442d98ae5be72"
        ),
        "the kernel's observable behaviour changed: if that is intended, bump RULES and update this fingerprint"
    );
}

#[test]
fn each_actor_gets_a_bounded_number_of_inputs_per_tick() {
    let mut k = Kernel::new(
        Law {
            max_inputs_per_actor: 2,
            ..Law::default()
        },
        1,
    );
    let spawn = |actor: Actor| Input {
        actor,
        action: Action::Spawn {
            components: BTreeMap::new(),
        },
    };
    let report = k.step(vec![
        spawn(player("ana")),
        spawn(player("ana")),
        spawn(player("ana")),
        spawn(player("budi")),
    ]);
    assert!(matches!(report.inputs[0], InputOutcome::Spawned(_)));
    assert!(matches!(report.inputs[1], InputOutcome::Spawned(_)));
    assert!(matches!(&report.inputs[2], InputOutcome::Rejected(r) if r.contains("more than 2")));
    assert!(matches!(report.inputs[3], InputOutcome::Spawned(_)));
}

#[test]
fn inconsistent_snapshots_are_refused() {
    let mut k = Kernel::new(Law::default(), 1);
    let id = install(&mut k, capsule!("bounce"), vec![]);
    let base = k.state().clone();
    let blobs = k.blobs().clone();
    type Break = fn(&mut State, CapsuleId);
    let breaks: Vec<(&str, Break)> = vec![
        ("capsule id at next_capsule", |s, _| s.next_capsule = 0),
        ("handle 0 not own entity", |s, c| {
            s.capsules.get_mut(&c).unwrap().clist[0] = None
        }),
        ("handle 0 read-only", |s, c| {
            let cap = s.capsules.get_mut(&c).unwrap();
            cap.clist[0] = Some(Cap {
                entity: cap.self_entity,
                write: false,
            });
        }),
        ("own entity missing", |s, c| {
            s.capsules.get_mut(&c).unwrap().self_entity = EntityId(999)
        }),
        ("orphaned capsule entity", |s, c| {
            s.capsules.remove(&c);
        }),
    ];
    assert!(Kernel::from_state(base.clone(), blobs.clone()).is_ok());
    for (name, edit) in breaks {
        let mut s = base.clone();
        edit(&mut s, id);
        assert!(
            matches!(
                Kernel::from_state(s, blobs.clone()),
                Err(KernelError::InvalidSnapshot(_))
            ),
            "{name} was accepted"
        );
    }
}

#[test]
fn a_capsules_own_entity_leaves_only_with_the_capsule() {
    let mut k = Kernel::new(Law::default(), 1);
    let ana = player("ana");
    let id = match try_install(&mut k, ana.clone(), capsule!("dice"), vec![]) {
        InputOutcome::Installed(id) => id,
        other => panic!("{other:?}"),
    };
    let me = self_entity(&k, id);
    for actor in [ana.clone(), Actor::Admin] {
        assert!(matches!(
            act(&mut k, actor, Action::Despawn { entity: me }),
            InputOutcome::Rejected(r) if r.contains("uninstall")
        ));
    }
    assert_eq!(
        act(&mut k, ana, Action::Uninstall { capsule: id }),
        InputOutcome::Applied
    );
    assert!(k.world().entity(me).is_none());
}

#[test]
fn helix_moves_eight_entities_through_three_dimensions() {
    let mut k = Kernel::new(Law::default(), 1);
    let id = install(&mut k, capsule!("helix"), vec![]);
    let heights = |k: &Kernel| -> Vec<f64> {
        k.world()
            .entities()
            .filter(|(e, ent)| ent.owner == Owner::Capsule(id) && *e != self_entity(k, id))
            .map(|(e, _)| num(k, e, "pos.z").unwrap())
            .collect()
    };
    let before = heights(&k);
    assert_eq!(before.len(), 8);
    let report = k.step(Vec::new());
    assert_eq!(fault_of(&report, id), None);
    for (a, b) in before.iter().zip(heights(&k)) {
        assert!((b - a - 0.03).abs() < 1e-9 || b < *a, "{a} -> {b}");
    }
}
