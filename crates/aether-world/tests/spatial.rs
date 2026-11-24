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

    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (self.next() >> 11) as f64 / (1u64 << 53) as f64 * (hi - lo)
    }
}

fn brute(world: &World, dim: i64, x: f64, y: f64, r: f64) -> Vec<EntityId> {
    world
        .entities()
        .filter(|(_, e)| Place::of(e).is_some_and(|p| p.within(dim, x, y, r)))
        .map(|(id, _)| id)
        .collect()
}

fn place(world: &World, rng: &mut Rng, ids: &[EntityId], txn: &mut aether_world::Txn) {
    for id in ids {
        match rng.next() % 6 {
            0 => {
                txn.remove(world, *id, "pos.x").unwrap();
            }
            1 => {
                txn.set(world, *id, "dim", Value::Num((rng.next() % 3) as f64))
                    .unwrap();
            }
            _ => {
                txn.set(world, *id, "pos.x", Value::Num(rng.range(-200.0, 200.0)))
                    .unwrap();
                txn.set(world, *id, "pos.y", Value::Num(rng.range(-200.0, 200.0)))
                    .unwrap();
            }
        }
    }
}

#[test]
fn near_matches_a_brute_force_scan_through_moves_dims_and_despawns() {
    let mut rng = Rng(7);
    let mut world = World::new();
    let mut live = Vec::new();
    for round in 0..40 {
        let mut txn = world.begin();
        for _ in 0..30 {
            live.push(txn.spawn(Owner::Region));
        }
        let moving: Vec<EntityId> = live
            .iter()
            .copied()
            .filter(|_| rng.next().is_multiple_of(3))
            .collect();
        place(&world, &mut rng, &moving, &mut txn);
        if round % 4 == 3 {
            for id in live.drain(..10) {
                txn.despawn(&world, id).unwrap();
            }
        }
        world.commit(txn).unwrap();
        world.settle();
        for _ in 0..20 {
            let (dim, x, y) = (
                (rng.next() % 3) as i64,
                rng.range(-220.0, 220.0),
                rng.range(-220.0, 220.0),
            );
            let r = rng.range(0.0, 120.0);
            assert_eq!(world.near(dim, x, y, r), brute(&world, dim, x, y, r));
        }
    }
    let json = serde_json::to_string(&world).unwrap();
    let rebuilt: World = serde_json::from_str(&json).unwrap();
    assert_eq!(rebuilt, world, "the cell index rebuilds to the same state");
}

#[test]
fn transactions_see_their_own_moves_when_searching_nearby() {
    let mut rng = Rng(11);
    let mut world = World::new();
    let mut txn = world.begin();
    let ids: Vec<EntityId> = (0..200).map(|_| txn.spawn(Owner::Region)).collect();
    place(&world, &mut rng, &ids, &mut txn);
    world.commit(txn).unwrap();
    world.settle();
    let mut txn = world.begin();
    let fresh: Vec<EntityId> = (0..20).map(|_| txn.spawn(Owner::Region)).collect();
    place(&world, &mut rng, &ids[..60], &mut txn);
    place(&world, &mut rng, &fresh, &mut txn);
    for id in &ids[60..70] {
        txn.despawn(&world, *id).unwrap();
    }
    for _ in 0..30 {
        let (x, y, r) = (
            rng.range(-200.0, 200.0),
            rng.range(-200.0, 200.0),
            rng.range(0.0, 150.0),
        );
        let (seen, _) = txn.near(&world, "", 0, x, y, r);
        let mut expected: Vec<EntityId> = ids
            .iter()
            .chain(&fresh)
            .copied()
            .filter(|id| {
                txn.entity(&world, *id)
                    .and_then(Place::of)
                    .is_some_and(|p| p.within(0, x, y, r))
            })
            .collect();
        expected.sort();
        assert_eq!(seen, expected);
    }
}

#[test]
fn searches_are_bounded_by_the_largest_radius() {
    let mut world = World::new();
    let mut txn = world.begin();
    let near = txn.spawn(Owner::Region);
    txn.set(&world, near, "pos.x", Value::Num(500.0)).unwrap();
    let far = txn.spawn(Owner::Region);
    txn.set(&world, far, "pos.x", Value::Num(5000.0)).unwrap();
    world.commit(txn).unwrap();
    world.settle();
    assert_eq!(world.near(0, 0.0, 0.0, 1e9), vec![near]);
    assert!(world.near(0, f64::NAN, 0.0, 10.0).is_empty());
}
