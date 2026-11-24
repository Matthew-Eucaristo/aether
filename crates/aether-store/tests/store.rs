use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use aether_kernel::{Action, Actor, BlobHash, Input, Kernel, KernelError, Law, RULES};
use aether_store::{Store, StoreError};
use aether_world::{EntityId, Value};

const BOUNCE: &str = include_str!("../../../capsules/bounce.wat");
const DICE: &str = include_str!("../../../capsules/dice.wat");
const SEED: u64 = 7;

fn fresh_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    match fs::remove_dir_all(&dir) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => panic!("{e}"),
        _ => dir,
    }
}

struct Pair {
    dir: PathBuf,
    open: Option<Store>,
    reference: Kernel,
    wasm: Vec<BlobHash>,
}

impl Pair {
    fn create(name: &str) -> Pair {
        let dir = fresh_dir(name);
        let mut store = Store::create(&dir, Law::default(), SEED).unwrap();
        let mut reference = Kernel::new(Law::default(), SEED);
        let wasm = [BOUNCE, DICE]
            .iter()
            .map(|src| {
                let h = store.add_blob(src.as_bytes().to_vec()).unwrap();
                assert_eq!(reference.add_blob(src.as_bytes().to_vec()), h);
                h
            })
            .collect();
        Pair {
            dir,
            open: Some(store),
            reference,
            wasm,
        }
    }

    fn store(&mut self) -> &mut Store {
        self.open.as_mut().expect("the store is open")
    }

    fn close(&mut self) -> &Path {
        self.open = None;
        &self.dir
    }

    fn inputs(&self, tick: u64) -> Vec<Input> {
        let n = |x: f64| Value::Num(x);
        match tick {
            1 | 2 => vec![Input {
                actor: Actor::Admin,
                action: Action::Install {
                    wasm: self.wasm[tick as usize - 1],
                    grants: Vec::new(),
                    budget: Law::default().ceiling,
                },
            }],
            3 => vec![Input {
                actor: Actor::Admin,
                action: Action::Spawn {
                    components: BTreeMap::from([("open".to_owned(), n(1.0))]),
                },
            }],
            t if t % 3 == 0 => vec![Input {
                actor: Actor::Admin,
                action: Action::Set {
                    entity: EntityId(3),
                    key: "open".into(),
                    value: n(t as f64),
                },
            }],
            _ => Vec::new(),
        }
    }

    fn run(&mut self, ticks: u64) {
        for _ in 0..ticks {
            let tick = self.reference.world().tick() + 1;
            let inputs = self.inputs(tick);
            let expected = self.reference.step(inputs.clone());
            assert_eq!(self.store().step(inputs).unwrap(), expected);
        }
    }

    fn reopen(&mut self) {
        self.open = None;
        self.open = Some(Store::open(&self.dir).unwrap());
        let open = self.open.as_ref().expect("just opened");
        assert_eq!(open.kernel().state(), self.reference.state());
    }

    fn journal(&self) -> PathBuf {
        self.dir.join("journal.log")
    }
}

fn lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn reopen_resumes_exactly() {
    let mut p = Pair::create("reopen");
    p.run(12);
    p.reopen();
    p.run(6);
    p.reopen();
    assert_eq!(p.store().kernel().world().tick(), 18);
}

#[test]
fn torn_tail_is_discarded() {
    let mut p = Pair::create("torn");
    p.run(8);
    let journal = p.journal();
    let clean = fs::read(&journal).unwrap();
    let mut torn = clean.clone();
    torn.extend_from_slice(b"0123abcd {\"tick\":9,\"pri");
    fs::write(&journal, &torn).unwrap();
    p.reopen();
    assert_eq!(fs::read(&journal).unwrap(), clean);
    p.run(3);
    p.reopen();
}

#[test]
fn damaged_final_record_counts_as_torn() {
    let mut p = Pair::create("damaged-final");
    p.run(6);
    let journal = p.journal();
    let mut all = lines(&journal);
    let last = all.pop().unwrap().replace("\"tick\":6", "\"tick\":0");
    let kept: String = all.iter().map(|l| format!("{l}\n")).collect();
    fs::write(&journal, format!("{kept}{last}\n")).unwrap();
    let store = Store::open(p.close()).unwrap();
    assert_eq!(store.kernel().world().tick(), 5);
    assert_eq!(fs::read_to_string(&journal).unwrap(), kept);
}

#[test]
fn damaged_middle_record_is_rejected() {
    let mut p = Pair::create("damaged-middle");
    p.run(6);
    let journal = p.journal();
    let mut all = lines(&journal);
    all[3] = all[3].replace("\"tick\":3", "\"tick\":4");
    fs::write(&journal, all.join("\n") + "\n").unwrap();
    match Store::open(p.close()) {
        Err(StoreError::Corrupt { reason, .. }) => assert!(reason.contains("checksum"), "{reason}"),
        other => panic!("expected corruption, got {:?}", other.err()),
    }
}

#[test]
fn divergent_history_is_detected() {
    let mut p = Pair::create("diverged");
    p.run(6);
    let journal = p.journal();
    let mut all = lines(&journal);
    let json = all[3].split_once(' ').unwrap().1.replace("1.0", "2.0");
    assert_ne!(json, all[3].split_once(' ').unwrap().1);
    all[3] = format!("{} {json}", BlobHash::of(json.as_bytes()).to_hex());
    fs::write(&journal, all.join("\n") + "\n").unwrap();
    match Store::open(p.close()) {
        Err(StoreError::Diverged { tick, .. }) => assert_eq!(tick, 4),
        other => panic!("expected divergence, got {:?}", other.err()),
    }
}

#[test]
fn checkpoint_compacts_and_survives_a_crash_before_truncation() {
    let mut p = Pair::create("checkpoint");
    p.run(6);
    let journal = p.journal();
    let before = fs::read(&journal).unwrap();
    p.store().checkpoint().unwrap();
    assert_eq!(
        lines(&journal),
        vec![format!("aether-journal rules={RULES}")]
    );
    p.run(4);
    assert_eq!(lines(&journal).len(), 5);
    p.reopen();

    let mut crashed = before;
    let after = fs::read_to_string(&journal).unwrap();
    crashed.extend(after.split_once('\n').unwrap().1.as_bytes());
    fs::write(&journal, crashed).unwrap();
    p.reopen();
    p.run(2);
    p.reopen();
}

#[test]
fn damaged_or_missing_blobs_are_rejected() {
    let mut p = Pair::create("blobs");
    p.run(3);
    let blobs = p.dir.join("blobs");
    fs::write(blobs.join("leftover.tmp"), b"partial").unwrap();
    p.reopen();
    assert!(!blobs.join("leftover.tmp").exists());

    let bounce = blobs.join(p.wasm[0].to_hex());
    fs::write(&bounce, b"(module)").unwrap();
    assert!(matches!(
        Store::open(p.close()),
        Err(StoreError::Corrupt { .. })
    ));

    fs::remove_file(&bounce).unwrap();
    assert!(matches!(
        Store::open(p.close()),
        Err(StoreError::Kernel(KernelError::MissingBlob(h))) if h == p.wasm[0]
    ));
}

#[test]
fn installs_of_unstored_blobs_are_refused_before_journaling() {
    let mut p = Pair::create("unstored");
    let unknown = BlobHash::of(b"never stored");
    let journal = p.journal();
    let err = p.store().step(vec![Input {
        actor: Actor::Admin,
        action: Action::Install {
            wasm: unknown,
            grants: Vec::new(),
            budget: Law::default().ceiling,
        },
    }]);
    assert!(matches!(err, Err(StoreError::Kernel(KernelError::MissingBlob(h))) if h == unknown));
    assert_eq!(lines(&journal).len(), 1);
    assert_eq!(p.store().kernel().world().tick(), 0);
    p.run(2);
}

#[test]
fn create_refuses_an_existing_world() {
    let mut p = Pair::create("exists");
    assert!(matches!(
        Store::create(p.close(), Law::default(), SEED),
        Err(StoreError::Exists(_))
    ));
}

#[test]
fn checkpoint_drops_blobs_no_capsule_uses() {
    let mut p = Pair::create("prune");
    p.run(3);
    let unused = p.store().add_blob(b"(module)".to_vec()).unwrap();
    let blobs = p.dir.join("blobs");
    assert!(blobs.join(unused.to_hex()).exists());
    p.store().checkpoint().unwrap();
    assert!(!blobs.join(unused.to_hex()).exists());
    assert!(p.store().kernel().blobs().get(&unused).is_none());
    for used in &p.wasm {
        assert!(blobs.join(used.to_hex()).exists());
    }
    p.reopen();
    p.run(2);
}

#[test]
fn journals_from_other_rules_are_refused_until_checkpointed() {
    let mut p = Pair::create("rules");
    p.run(4);
    let journal = p.journal();
    let text = fs::read_to_string(&journal).unwrap();
    let old = text.replacen(RULES, "aether-rules/0", 1);
    fs::write(&journal, &old).unwrap();
    assert!(matches!(
        Store::open(p.close()),
        Err(StoreError::RulesChanged { .. })
    ));

    fs::write(&journal, &text).unwrap();
    p.reopen();
    p.store().checkpoint().unwrap();
    let header_only = fs::read_to_string(&journal).unwrap();
    fs::write(&journal, header_only.replacen(RULES, "aether-rules/0", 1)).unwrap();
    p.reopen();
    assert_eq!(
        lines(&journal),
        vec![format!("aether-journal rules={RULES}")]
    );
    p.run(2);
    p.reopen();
}

#[test]
fn a_journal_without_a_header_is_rejected() {
    let mut p = Pair::create("headless");
    p.run(3);
    let journal = p.journal();
    let text = fs::read_to_string(&journal).unwrap();
    fs::write(&journal, text.split_once('\n').unwrap().1).unwrap();
    assert!(matches!(
        Store::open(p.close()),
        Err(StoreError::Corrupt { .. })
    ));
}

#[test]
fn a_world_is_open_in_one_place_at_a_time() {
    let mut p = Pair::create("locked");
    p.run(2);
    assert!(matches!(Store::open(&p.dir), Err(StoreError::Locked(_))));
    p.run(2);
    p.reopen();
}
