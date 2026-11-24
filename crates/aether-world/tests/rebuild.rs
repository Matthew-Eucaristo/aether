use std::collections::{BTreeMap, BTreeSet};

use aether_world::{CapsuleId, EntityId, Owner, PlayerId, Usage, Value, World, entity_bytes};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const KEY_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_.-";

fn random_key(rng: &mut Rng) -> String {
    let len = 1 + rng.below(8) as usize;
    (0..len)
        .map(|_| KEY_CHARS[rng.below(KEY_CHARS.len() as u64) as usize] as char)
        .collect()
}

fn random_finite(rng: &mut Rng) -> f64 {
    loop {
        let bits = rng.next();
        let f = f64::from_bits(bits);
        if f.is_finite() {
            return f;
        }
    }
}

fn random_value(rng: &mut Rng) -> Value {
    match rng.below(4) {
        0 => Value::Num(match rng.below(6) {
            0 => 0.0,
            1 => -0.0,
            2 => 1.0,
            3 => -1.0,
            4 => random_finite(rng),
            _ => rng.below(1000) as f64,
        }),
        1 => {
            let len = match rng.below(4) {
                0 => 0,
                1 => 1 + rng.below(40) as usize,
                2 => 4096,
                _ => 1 + rng.below(200) as usize,
            };
            let mut s = String::new();
            while s.len() < len {
                match rng.below(3) {
                    0 => s.push(KEY_CHARS[rng.below(KEY_CHARS.len() as u64) as usize] as char),
                    1 => s.push('é'),
                    _ => s.push('\u{1f4e6}'),
                }
            }
            while s.len() > len || !s.is_char_boundary(s.len()) {
                s.pop();
            }
            Value::Text(s)
        }
        2 => Value::Bool(rng.below(2) == 0),
        _ => {
            let len = match rng.below(8) {
                0 => 0,
                1 => 511,
                2 => 512,
                3 => 513,
                4 => 1025,
                _ => rng.below(20) as usize,
            };
            let nums: Vec<f64> = (0..len)
                .map(|_| match rng.below(4) {
                    0 => -0.0,
                    1 => 0.0,
                    _ => random_finite(rng),
                })
                .collect();
            Value::Nums(nums.into())
        }
    }
}

fn random_owner(rng: &mut Rng) -> Owner {
    match rng.below(4) {
        0 => Owner::Region,
        1 => Owner::Player(PlayerId(format!("p{}", rng.below(4)))),
        _ => Owner::Capsule(CapsuleId(rng.below(4))),
    }
}

fn verify_world(world: &World, ctx: &str) {
    let json = serde_json::to_string(world).expect("serialize");
    let rebuilt: World = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(
        rebuilt.hash(),
        world.hash(),
        "{ctx}: incremental hash != rebuild"
    );
    assert_eq!(rebuilt, *world, "{ctx}: rebuilt world differs");

    let mut expected: BTreeMap<Owner, Usage> = BTreeMap::new();
    for (_, e) in world.entities() {
        let u = expected.entry(e.owner.clone()).or_default();
        u.entities += 1;
        u.bytes += entity_bytes(e);
    }
    for (owner, u) in &expected {
        assert_eq!(&world.usage(owner), u, "{ctx}: usage for {owner:?}");
    }
    assert_eq!(
        world.usage(&Owner::Player(PlayerId("never-existed".into()))),
        Usage::default(),
        "{ctx}: phantom usage entry"
    );

    let mut keys = BTreeSet::new();
    for (_, e) in world.entities() {
        for key in e.components.keys() {
            keys.insert(key.clone());
        }
    }
    let txn = world.begin();
    for key in keys.iter().take(12) {
        let mut want = Vec::new();
        for (id, e) in world.entities() {
            if e.components.contains_key(key) {
                want.push(id);
            }
        }
        let mut got = Vec::new();
        let mut at = None;
        while let (Some(id), _) = txn.find(world, key, at) {
            got.push(id);
            at = Some(id);
        }
        assert_eq!(got, want, "{ctx}: find({key:?})");
    }
}

#[test]
fn random_histories_match_a_fresh_rebuild() {
    for trial in 0..60u64 {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15u64.wrapping_mul(trial + 1) + 1);
        let mut world = World::new();
        let mut live: Vec<EntityId> = Vec::new();
        let steps = 100 + rng.below(60);
        for step in 0..steps {
            let mut txn = world.begin();
            let ops = 1 + rng.below(5);
            for _ in 0..ops {
                match rng.below(10) {
                    0 | 1 => {
                        let id = txn.spawn(random_owner(&mut rng));
                        live.push(id);
                        for _ in 0..rng.below(4) {
                            let key = random_key(&mut rng);
                            let value = random_value(&mut rng);
                            txn.set(&world, id, &key, value).unwrap();
                        }
                    }
                    2..=4 => {
                        if !live.is_empty() {
                            let id = live[rng.below(live.len() as u64) as usize];
                            let key = random_key(&mut rng);
                            let value = random_value(&mut rng);
                            txn.set(&world, id, &key, value).unwrap();
                            if rng.below(3) == 0 {
                                let value = random_value(&mut rng);
                                txn.set(&world, id, &key, value).unwrap();
                            }
                        }
                    }
                    5 => {
                        if !live.is_empty() {
                            let id = live[rng.below(live.len() as u64) as usize];
                            let key = random_key(&mut rng);
                            txn.remove(&world, id, &key).unwrap();
                        }
                    }
                    6 => {
                        if !live.is_empty() {
                            let at = rng.below(live.len() as u64) as usize;
                            let id = live.remove(at);
                            txn.despawn(&world, id).unwrap();
                        }
                    }
                    7 => {
                        let id = txn.spawn(random_owner(&mut rng));
                        let key = random_key(&mut rng);
                        txn.set(&world, id, &key, random_value(&mut rng)).unwrap();
                        txn.despawn(&world, id).unwrap();
                    }
                    8 => {
                        if !live.is_empty() {
                            let id = live[rng.below(live.len() as u64) as usize];
                            let key = random_key(&mut rng);
                            txn.set(&world, id, &key, random_value(&mut rng)).unwrap();
                            txn.remove(&world, id, &key).unwrap();
                            txn.set(&world, id, &key, random_value(&mut rng)).unwrap();
                        }
                    }
                    _ => {
                        if live.len() >= 2 {
                            let at = rng.below(live.len() as u64) as usize;
                            let id = live.remove(at);
                            txn.despawn(&world, id).unwrap();
                            let fresh = txn.spawn(random_owner(&mut rng));
                            live.push(fresh);
                        }
                    }
                }
            }
            world.commit(txn).unwrap();
            if rng.below(3) == 0 || step == steps - 1 {
                world.settle();
                verify_world(&world, &format!("trial {trial} step {step}"));
            }
        }
        world.settle();
        let first = world.hash();
        world.settle();
        assert_eq!(first, world.hash(), "trial {trial}: settle not idempotent");
    }
}

#[test]
fn large_parallel_settles_match_a_rebuild() {
    let mut rng = Rng(0xdead_beef_cafe_f00d);
    let mut world = World::new();
    let mut live: Vec<EntityId> = Vec::new();
    for round in 0..12 {
        let mut txn = world.begin();
        for _ in 0..400 {
            match rng.below(10) {
                0..=3 => {
                    let id = txn.spawn(random_owner(&mut rng));
                    live.push(id);
                    for _ in 0..1 + rng.below(3) {
                        let key = random_key(&mut rng);
                        txn.set(&world, id, &key, random_value(&mut rng)).unwrap();
                    }
                }
                4..=6 if !live.is_empty() => {
                    let id = live[rng.below(live.len() as u64) as usize];
                    let key = random_key(&mut rng);
                    txn.set(&world, id, &key, random_value(&mut rng)).unwrap();
                }
                7 if !live.is_empty() => {
                    let at = rng.below(live.len() as u64) as usize;
                    let id = live.remove(at);
                    txn.despawn(&world, id).unwrap();
                }
                8 if !live.is_empty() => {
                    let id = live[rng.below(live.len() as u64) as usize];
                    let key = random_key(&mut rng);
                    txn.remove(&world, id, &key).unwrap();
                }
                _ => {
                    let id = txn.spawn(random_owner(&mut rng));
                    txn.despawn(&world, id).unwrap();
                }
            }
        }
        world.commit(txn).unwrap();
        if round % 2 == 0 {
            world.settle();
            verify_world(&world, &format!("round {round}"));
        }
    }
    world.settle();
    verify_world(&world, "final");
    assert!(world.len() > 128, "enough entities to fork threads");
}

#[test]
fn boundary_sized_keys_texts_and_nums_match_a_rebuild() {
    let mut world = World::new();
    let mut txn = world.begin();
    let id = txn.spawn(Owner::Region);
    let key64: String = std::iter::repeat_n('k', 64).collect();
    txn.set(&world, id, &key64, Value::Num(1.0)).unwrap();
    txn.set(&world, id, "k", Value::Text("x".repeat(4096)))
        .unwrap();
    txn.set(&world, id, "empty", Value::Text(String::new()))
        .unwrap();
    for (i, len) in [0usize, 1, 511, 512, 513, 1024, 1025, 2048]
        .iter()
        .enumerate()
    {
        let nums: Vec<f64> = (0..*len).map(|j| (i as f64) * 0.5 + (j as f64)).collect();
        txn.set(&world, id, &format!("n{i}"), Value::Nums(nums.into()))
            .unwrap();
    }
    world.commit(txn).unwrap();
    world.settle();
    verify_world(&world, "boundaries");
}

#[test]
fn signed_zero_hashes_differ_but_each_matches_its_rebuild() {
    let mut world = World::new();
    let mut txn = world.begin();
    let id = txn.spawn(Owner::Region);
    txn.set(&world, id, "x", Value::Num(-0.0)).unwrap();
    world.commit(txn).unwrap();
    world.settle();
    verify_world(&world, "negative zero");
    let neg = world.hash();

    let mut txn = world.begin();
    txn.set(&world, id, "x", Value::Num(0.0)).unwrap();
    world.commit(txn).unwrap();
    world.settle();
    verify_world(&world, "positive zero");
    assert_ne!(neg, world.hash(), "-0.0 and 0.0 must hash differently");
}

#[test]
fn rewrite_then_restore_is_digest_neutral() {
    let mut world = World::new();
    let mut txn = world.begin();
    let id = txn.spawn(Owner::Region);
    txn.set(&world, id, "x", Value::Num(1.0)).unwrap();
    world.commit(txn).unwrap();
    world.settle();
    let baseline = world.hash();

    let mut txn = world.begin();
    txn.set(&world, id, "x", Value::Num(2.0)).unwrap();
    world.commit(txn).unwrap();
    let mut txn = world.begin();
    txn.set(&world, id, "x", Value::Num(1.0)).unwrap();
    world.commit(txn).unwrap();
    world.settle();
    assert_eq!(world.hash(), baseline);
    verify_world(&world, "restored");
}

#[test]
fn spawn_then_despawn_in_one_txn_is_invisible() {
    let mut world = World::new();
    world.settle();
    let before = world.hash();
    let mut txn = world.begin();
    let id = txn.spawn(Owner::Region);
    txn.set(&world, id, "x", Value::Num(9.0)).unwrap();
    txn.despawn(&world, id).unwrap();
    world.commit(txn).unwrap();
    world.settle();
    assert_eq!(world.hash(), before);
    let mut txn = world.begin();
    let reused = txn.spawn(Owner::Region);
    assert_eq!(reused, id, "the never-committed id is reused");
}

#[test]
fn many_rewrites_of_one_entity_before_settling() {
    let mut world = World::new();
    let mut txn = world.begin();
    let id = txn.spawn(Owner::Region);
    world.commit(txn).unwrap();
    for round in 0..50 {
        let mut txn = world.begin();
        txn.set(&world, id, "x", Value::Num(round as f64)).unwrap();
        if round % 2 == 0 {
            txn.set(&world, id, &format!("k{round}"), Value::Bool(true))
                .unwrap();
        } else {
            txn.remove(&world, id, &format!("k{}", round - 1)).unwrap();
        }
        world.commit(txn).unwrap();
    }
    world.settle();
    verify_world(&world, "rewrites");
    assert_eq!(world.entity(id).unwrap().components.len(), 1);
}

#[test]
fn flipping_zero_sign_must_not_desync_the_incremental_hash() {
    let mut world = World::new();
    let mut txn = world.begin();
    let id = txn.spawn(Owner::Region);
    txn.set(&world, id, "x", Value::Num(-0.0)).unwrap();
    world.commit(txn).unwrap();
    world.settle();
    let mut txn = world.begin();
    txn.set(&world, id, "x", Value::Num(0.0)).unwrap();
    world.commit(txn).unwrap();
    world.settle();
    let rebuilt: World = serde_json::from_str(&serde_json::to_string(&world).unwrap()).unwrap();
    assert_eq!(rebuilt.hash(), world.hash(), "incremental digest desynced");
}

#[test]
fn flipping_zero_sign_inside_a_nums_array_must_not_desync() {
    let mut world = World::new();
    let mut txn = world.begin();
    let id = txn.spawn(Owner::Region);
    txn.set(&world, id, "x", Value::Nums(vec![1.0, -0.0].into()))
        .unwrap();
    world.commit(txn).unwrap();
    world.settle();
    let mut txn = world.begin();
    txn.set(&world, id, "x", Value::Nums(vec![1.0, 0.0].into()))
        .unwrap();
    world.commit(txn).unwrap();
    world.settle();
    let rebuilt: World = serde_json::from_str(&serde_json::to_string(&world).unwrap()).unwrap();
    assert_eq!(rebuilt.hash(), world.hash(), "incremental digest desynced");
}
