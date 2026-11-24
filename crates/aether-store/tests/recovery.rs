use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use aether_kernel::{Action, Actor, BlobHash, Input, Law, RULES};
use aether_store::{Store, StoreError};
use aether_world::{EntityId, Value};

const SEED: u64 = 7;

fn fresh_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    match fs::remove_dir_all(&dir) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => panic!("{e}"),
        _ => dir,
    }
}

fn spawn_input(v: f64) -> Input {
    Input {
        actor: Actor::Admin,
        action: Action::Spawn {
            components: BTreeMap::from([("open".to_owned(), Value::Num(v))]),
        },
    }
}

fn run(dir: &PathBuf, ticks: u64) -> Store {
    let mut store = Store::create(dir, Law::default(), SEED).unwrap();
    for i in 0..ticks {
        store.step(vec![spawn_input(i as f64)]).unwrap();
    }
    store
}

fn lines(path: &PathBuf) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn reencode(json_text: &str, edit: impl Fn(&mut serde_json::Value)) -> String {
    let (_, json) = json_text.split_once(' ').unwrap();
    let mut v: serde_json::Value = serde_json::from_str(json).unwrap();
    edit(&mut v);
    let bytes = serde_json::to_vec(&v).unwrap();
    format!(
        "{} {}",
        BlobHash::of(&bytes).to_hex(),
        String::from_utf8(bytes).unwrap()
    )
}

#[test]
fn an_empty_journal_restores_exactly_the_snapshot_state() {
    let dir = fresh_dir("audit-empty-journal");
    let store = run(&dir, 5);
    drop(store);
    fs::write(dir.join("journal.log"), b"").unwrap();
    let s = Store::open(&dir).expect("empty journal recovers (checkpoint crash shape)");
    assert_eq!(s.kernel().world().tick(), 0);
    let reference =
        Store::create(fresh_dir("audit-empty-journal-ref"), Law::default(), SEED).unwrap();
    assert_eq!(s.kernel().state_hash(), reference.kernel().state_hash());
}

#[test]
fn crash_after_journal_zeroing_during_checkpoint_recovers() {
    let dir = fresh_dir("audit-crash-zeroed");
    let mut store = run(&dir, 5);
    store.checkpoint().unwrap();
    let durable = store.kernel().state_hash();
    drop(store);
    fs::write(dir.join("journal.log"), b"").unwrap();
    let s = Store::open(&dir).expect("checkpoint-crash must recover");
    assert_eq!(s.kernel().world().tick(), 5);
    assert_eq!(s.kernel().state_hash(), durable);
}

#[test]
fn crash_mid_header_write_during_checkpoint_recovers() {
    let dir = fresh_dir("audit-crash-partial-header");
    let mut store = run(&dir, 5);
    store.checkpoint().unwrap();
    let durable = store.kernel().state_hash();
    drop(store);
    fs::write(dir.join("journal.log"), b"aether-journal rules=aether-").unwrap();
    let s = Store::open(&dir).expect("mid-header crash must recover");
    assert_eq!(s.kernel().state_hash(), durable);
    assert_eq!(
        lines(&dir.join("journal.log")),
        vec![format!("aether-journal rules={RULES}")]
    );
}

#[test]
fn a_header_from_newer_rules_is_rejected() {
    let dir = fresh_dir("audit-future-rules");
    let mut store = run(&dir, 3);
    store.checkpoint().unwrap();
    drop(store);
    let journal = dir.join("journal.log");
    let header = fs::read_to_string(&journal).unwrap();
    fs::write(&journal, header.replacen(RULES, "aether-rules/999", 1)).unwrap();
    match Store::open(&dir) {
        Err(StoreError::RulesChanged { .. }) | Err(StoreError::Corrupt { .. }) => {}
        Ok(s) => panic!(
            "SILENT ACCEPT: rules/999 journal opened at tick {}; header now {:?}",
            s.kernel().world().tick(),
            lines(&dir.join("journal.log")).first()
        ),
        Err(e) => panic!("unexpected error: {e}"),
    }
}

#[test]
fn a_record_for_the_wrong_tick_is_rejected() {
    let dir = fresh_dir("audit-tick-gap");
    let store = run(&dir, 6);
    drop(store);
    let journal = dir.join("journal.log");
    let mut all = lines(&journal);
    all[3] = reencode(&all[3], |v| v["tick"] = serde_json::json!(99));
    fs::write(&journal, all.join("\n") + "\n").unwrap();
    match Store::open(&dir) {
        Err(StoreError::Corrupt { reason, .. }) => {
            assert!(reason.contains("tick 99"), "{reason}");
        }
        other => panic!("expected corruption, got {:?}", other.err()),
    }
}

#[test]
fn swapped_records_are_rejected() {
    let dir = fresh_dir("audit-swapped");
    let store = run(&dir, 6);
    drop(store);
    let journal = dir.join("journal.log");
    let mut all = lines(&journal);
    all.swap(3, 4);
    fs::write(&journal, all.join("\n") + "\n").unwrap();
    assert!(matches!(
        Store::open(&dir),
        Err(StoreError::Corrupt { .. }) | Err(StoreError::Diverged { .. })
    ));
}

#[test]
fn a_missing_or_truncated_snapshot_is_rejected() {
    let dir = fresh_dir("audit-no-snapshot");
    let store = run(&dir, 3);
    drop(store);
    let snapshot = dir.join("snapshot.json");
    fs::remove_file(&snapshot).unwrap();
    assert!(matches!(Store::open(&dir), Err(StoreError::Io { .. })));

    let dir = fresh_dir("audit-half-snapshot");
    let store = run(&dir, 3);
    drop(store);
    let snapshot = dir.join("snapshot.json");
    let bytes = fs::read(&snapshot).unwrap();
    fs::write(&snapshot, &bytes[..bytes.len() / 2]).unwrap();
    assert!(matches!(Store::open(&dir), Err(StoreError::Corrupt { .. })));
}

#[test]
fn create_refuses_a_directory_that_still_holds_a_journal() {
    let dir = fresh_dir("audit-create-over-journal");
    let store = run(&dir, 5);
    drop(store);
    fs::remove_file(dir.join("snapshot.json")).unwrap();
    match Store::create(&dir, Law::default(), SEED) {
        Err(StoreError::Exists(_)) => {}
        Ok(mut s) => {
            let kept = lines(&dir.join("journal.log"));
            let tick = s.kernel().world().tick();
            s.step(vec![spawn_input(0.0)]).unwrap();
            panic!(
                "SILENT DESTRUCTION: create returned Ok at tick {tick}; journal lines now {kept:?}"
            );
        }
        Err(e) => panic!("unexpected error: {e}"),
    }
}

#[test]
fn old_rules_with_pending_records_and_an_old_snapshot_are_refused() {
    let dir = fresh_dir("audit-old-rules-pending");
    let store = run(&dir, 4);
    drop(store);
    let journal = dir.join("journal.log");
    let text = fs::read_to_string(&journal).unwrap();
    fs::write(&journal, text.replacen(RULES, "aether-rules/4", 1)).unwrap();
    let snapshot = dir.join("snapshot.json");
    let mut json: serde_json::Value =
        serde_json::from_slice(&fs::read(&snapshot).unwrap()).unwrap();
    json.as_object_mut().unwrap().remove("signals");
    json["law"]
        .as_object_mut()
        .unwrap()
        .remove("region_fuel_per_tick");
    fs::write(&snapshot, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(matches!(
        Store::open(&dir),
        Err(StoreError::RulesChanged { .. })
    ));
}

#[test]
fn a_valid_record_missing_only_its_newline_is_torn() {
    let dir = fresh_dir("audit-newlineless-record");
    let store = run(&dir, 6);
    let durable5 = {
        let reference = run(&fresh_dir("audit-newlineless-ref"), 5);
        reference.kernel().state_hash()
    };
    drop(store);
    let journal = dir.join("journal.log");
    let bytes = fs::read(&journal).unwrap();
    assert!(bytes.ends_with(b"\n"));
    fs::write(&journal, &bytes[..bytes.len() - 1]).unwrap();
    let s = Store::open(&dir).expect("torn tail must recover");
    assert_eq!(s.kernel().world().tick(), 5);
    assert_eq!(s.kernel().state_hash(), durable5);
}

#[test]
fn crash_mid_migration_truncation_fails_loudly_not_silently() {
    let dir = fresh_dir("audit-migration-crash");
    let mut store = run(&dir, 3);
    store.checkpoint().unwrap();
    drop(store);
    let journal = dir.join("journal.log");
    let header = fs::read_to_string(&journal).unwrap();
    fs::write(&journal, header.replacen(RULES, "aether-rules/4", 1)).unwrap();
    let snapshot = dir.join("snapshot.json");
    let mut json: serde_json::Value =
        serde_json::from_slice(&fs::read(&snapshot).unwrap()).unwrap();
    json.as_object_mut().unwrap().remove("signals");
    json["law"]
        .as_object_mut()
        .unwrap()
        .remove("region_fuel_per_tick");
    fs::write(&snapshot, serde_json::to_vec(&json).unwrap()).unwrap();
    Store::open(&dir).expect("migration target opens fine before the crash");
    fs::write(&journal, header.replacen(RULES, "aether-rules/4", 1)).unwrap();
    fs::write(&snapshot, serde_json::to_vec(&json).unwrap()).unwrap();
    fs::write(&journal, b"").unwrap();
    match Store::open(&dir) {
        Ok(_) => panic!("silently accepted"),
        Err(e) => println!("loud failure: {e}"),
    }
}

#[test]
fn checkpoint_preserves_the_state_hash_bit_for_bit() {
    let dir = fresh_dir("audit-hash-stability");
    let mut store = Store::create(&dir, Law::default(), SEED).unwrap();
    store
        .step(vec![Input {
            actor: Actor::Admin,
            action: Action::Spawn {
                components: BTreeMap::from([("x".to_owned(), Value::Num(-0.0))]),
            },
        }])
        .unwrap();
    store
        .step(vec![Input {
            actor: Actor::Admin,
            action: Action::Set {
                entity: EntityId(0),
                key: "x".into(),
                value: Value::Num(0.0),
            },
        }])
        .unwrap();
    let live = store.kernel().state_hash();
    store.checkpoint().unwrap();
    drop(store);
    let reopened = Store::open(&dir).unwrap();
    assert_eq!(
        reopened.kernel().state_hash(),
        live,
        "the same logical state changed its canonical hash across a checkpoint"
    );
}
