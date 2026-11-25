use aether_world::{EntityId, Owner, Place, Value, World};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.unit() * (hi - lo)
    }

    fn pick(&mut self, values: &[f64]) -> f64 {
        values[self.next() as usize % values.len()]
    }
}

fn brute(world: &World, dim: i64, x: f64, y: f64, r: f64) -> Vec<EntityId> {
    let r = r.clamp(0.0, 512.0);
    world
        .entities()
        .filter(|(_, e)| Place::of(e).is_some_and(|p| p.within(dim, x, y, r)))
        .map(|(id, _)| id)
        .collect()
}

fn overlay_brute(
    txn: &aether_world::Txn,
    world: &World,
    key: &str,
    dim: i64,
    x: f64,
    y: f64,
    r: f64,
) -> Vec<EntityId> {
    let r = r.clamp(0.0, 512.0);
    let mut out: Vec<EntityId> = world
        .entities()
        .map(|(id, _)| id)
        .chain((0..4096).map(EntityId))
        .filter(|id| {
            txn.entity(world, *id).is_some_and(|e| {
                (key.is_empty() || e.components.contains_key(key))
                    && Place::of(e).is_some_and(|p| p.within(dim, x, y, r))
            })
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

fn put_random_place(world: &World, txn: &mut aether_world::Txn, rng: &mut Rng, id: EntityId) {
    let positions = [
        -4096.0,
        -512.0,
        -32.0,
        -16.0,
        -15.999999999999998,
        -0.0,
        0.0,
        15.999999999999998,
        16.0,
        32.0,
        512.0,
        4096.0,
        1e100,
        -1e100,
    ];
    let scales = [
        -1e300,
        -40.0,
        -16.0,
        -0.0,
        0.0,
        0.5,
        15.999999999999998,
        16.0,
        16.000000000000004,
        40.0,
        400.0,
        1e300,
    ];
    for (key, value) in [
        ("pos.x", rng.pick(&positions) + rng.range(-0.25, 0.25)),
        ("pos.y", rng.pick(&positions) + rng.range(-0.25, 0.25)),
        ("dim", (rng.next() % 5) as f64 - 2.0),
        ("scale.x", rng.pick(&scales)),
        ("scale.y", rng.pick(&scales)),
    ] {
        txn.set(world, id, key, Value::Num(value)).unwrap();
    }
    if rng.next().is_multiple_of(2) {
        txn.set(world, id, "mark", Value::Bool(true)).unwrap();
    }
}

#[test]
fn near_matches_brute_force_across_wide_transitions_and_transaction_overlay() {
    let mut rng = Rng(0x5eed);
    let mut world = World::new();
    let mut live = Vec::new();
    for round in 0..24 {
        let mut txn = world.begin();
        while live.len() < 90 {
            live.push(txn.spawn(Owner::Region));
        }
        for id in live.iter().copied() {
            if rng.next().is_multiple_of(3) {
                put_random_place(&world, &mut txn, &mut rng, id);
            }
            match rng.next() % 8 {
                0 => {
                    txn.remove(&world, id, "scale.x").unwrap();
                }
                1 => {
                    txn.remove(&world, id, "scale.y").unwrap();
                }
                2 => {
                    txn.remove(&world, id, "pos.x").unwrap();
                }
                3 => {
                    txn.set(&world, id, "dim", Value::Num((rng.next() % 5) as f64 - 2.0))
                        .unwrap();
                }
                _ => {}
            }
        }
        if round % 5 == 4 && live.len() > 20 {
            for id in live.drain(..8) {
                txn.despawn(&world, id).unwrap();
            }
        }
        for _ in 0..24 {
            let dim = (rng.next() % 5) as i64 - 2;
            let x = rng.pick(&[-600.0, -16.0, 0.0, 16.0, 600.0]) + rng.range(-40.0, 40.0);
            let y = rng.pick(&[-600.0, -16.0, 0.0, 16.0, 600.0]) + rng.range(-40.0, 40.0);
            let r = rng.pick(&[-3.0, 0.0, 1.0, 15.0, 16.0, 64.0, 512.0, 1e9]);
            let key = if rng.next().is_multiple_of(2) {
                "mark"
            } else {
                ""
            };
            let (got, _) = txn.near(&world, key, dim, x, y, r);
            let want = overlay_brute(&txn, &world, key, dim, x, y, r);
            assert_eq!(
                got, want,
                "round {round} uncommitted dim={dim} x={x:e} y={y:e} r={r}"
            );
        }
        world.commit(txn).unwrap();
        world.settle();
        for _ in 0..24 {
            let dim = (rng.next() % 5) as i64 - 2;
            let x = rng.pick(&[-600.0, -16.0, 0.0, 16.0, 600.0]) + rng.range(-40.0, 40.0);
            let y = rng.pick(&[-600.0, -16.0, 0.0, 16.0, 600.0]) + rng.range(-40.0, 40.0);
            let r = rng.pick(&[-3.0, 0.0, 1.0, 15.0, 16.0, 64.0, 512.0, 1e9]);
            assert_eq!(
                world.near(dim, x, y, r),
                brute(&world, dim, x, y, r),
                "round {round} committed dim={dim} x={x:e} y={y:e} r={r}"
            );
        }
    }
    let json = serde_json::to_string(&world).unwrap();
    let rebuilt: World = serde_json::from_str(&json).unwrap();
    assert_eq!(rebuilt, world, "the wide/cell index rebuilds exactly");
    assert_eq!(rebuilt.hash(), world.hash());
}

#[test]
fn an_entity_reaches_as_far_as_its_documented_scale() {
    let mut world = World::new();
    let mut txn = world.begin();
    let wall = txn.spawn(Owner::Region);
    for (key, value) in [("pos.x", 4500.0), ("pos.y", 0.0), ("scale.x", 5000.0)] {
        txn.set(&world, wall, key, Value::Num(value)).unwrap();
    }
    world.commit(txn).unwrap();
    world.settle();
    assert_eq!(
        world.near(0, 0.0, 0.0, 0.0),
        vec![wall],
        "scale.x=5000 centred at x=4500 reaches the origin"
    );
}
