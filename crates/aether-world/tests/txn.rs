use aether_world::{
    ENTITY_OVERHEAD_BYTES, EntityId, Owner, PlayerId, Usage, Value, World, WorldError,
};

#[test]
fn reads_see_uncommitted_writes_but_world_does_not() {
    let world = World::new();
    let mut txn = world.begin();
    let id = txn.spawn(Owner::Region);
    txn.set(&world, id, "hp", Value::Num(3.0)).unwrap();
    assert_eq!(txn.get(&world, id, "hp").unwrap(), Some(&Value::Num(3.0)));
    assert!(world.entity(id).is_none());
}

#[test]
fn dropped_txn_leaves_no_trace() {
    let mut world = World::new();
    let before = world.hash();
    {
        let mut txn = world.begin();
        txn.spawn(Owner::Region);
    }
    let empty = world.begin();
    world.commit(empty).unwrap();
    assert_eq!(world.hash(), before);
}

#[test]
fn stale_txn_cannot_commit() {
    let mut world = World::new();
    let mut first = world.begin();
    let mut second = world.begin();
    first.spawn(Owner::Region);
    second.spawn(Owner::Region);
    world.commit(first).unwrap();
    assert_eq!(world.commit(second), Err(WorldError::StaleTxn));
    assert_eq!(world.len(), 1);
}

#[test]
fn invalid_keys_and_values_are_rejected() {
    let world = World::new();
    let mut txn = world.begin();
    let id = txn.spawn(Owner::Region);
    assert_eq!(
        txn.set(&world, id, "", Value::Bool(true)),
        Err(WorldError::InvalidKey)
    );
    assert_eq!(
        txn.set(&world, id, "has space", Value::Bool(true)),
        Err(WorldError::InvalidKey)
    );
    assert_eq!(
        txn.set(&world, id, "x", Value::Num(f64::INFINITY)),
        Err(WorldError::NonFinite)
    );
    assert_eq!(
        txn.set(&world, EntityId(99), "x", Value::Num(1.0)),
        Err(WorldError::NoSuchEntity(EntityId(99)))
    );
}

#[test]
fn hash_depends_on_content_not_history() {
    let mut a = World::new();
    let mut txn = a.begin();
    let id = txn.spawn(Owner::Region);
    txn.set(&a, id, "x", Value::Num(1.0)).unwrap();
    a.commit(txn).unwrap();

    let mut b = World::new();
    let mut txn = b.begin();
    let id = txn.spawn(Owner::Region);
    b.commit(txn).unwrap();
    let mut txn = b.begin();
    txn.set(&b, id, "x", Value::Num(1.0)).unwrap();
    b.commit(txn).unwrap();

    assert_eq!(a.hash(), b.hash());

    let mut txn = b.begin();
    txn.set(&b, id, "x", Value::Num(2.0)).unwrap();
    b.commit(txn).unwrap();
    assert_ne!(a.hash(), b.hash());
}

#[test]
fn incremental_hash_matches_a_fresh_rebuild_after_every_commit() {
    let mut world = World::new();
    let mut ids = Vec::new();
    let mut x = 0x2545_f491_4f6c_dd1d_u64;
    for step in 0..400 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let mut txn = world.begin();
        match x % 5 {
            0 | 1 => ids.push(txn.spawn(Owner::Region)),
            2 | 3 if !ids.is_empty() => {
                let id = ids[(x as usize / 7) % ids.len()];
                let key = ["a", "b", "c"][(x as usize / 3) % 3];
                txn.set(&world, id, key, Value::Num(step as f64)).unwrap();
            }
            _ if !ids.is_empty() => {
                let id = ids.remove((x as usize / 11) % ids.len());
                txn.despawn(&world, id).unwrap();
            }
            _ => {}
        }
        world.commit(txn).unwrap();
        let rebuilt: World = serde_json::from_str(&serde_json::to_string(&world).unwrap()).unwrap();
        assert_eq!(rebuilt.hash(), world.hash(), "step {step}");
        assert_eq!(rebuilt, world);
    }
    assert!(world.len() > 10);
}

#[test]
fn despawning_everything_returns_to_the_empty_digest() {
    let mut world = World::new();
    let mut txn = world.begin();
    let a = txn.spawn(Owner::Region);
    let b = txn.spawn(Owner::Region);
    txn.set(&world, a, "x", Value::Num(1.0)).unwrap();
    world.commit(txn).unwrap();
    let mut txn = world.begin();
    txn.despawn(&world, a).unwrap();
    txn.despawn(&world, b).unwrap();
    world.commit(txn).unwrap();
    let empty: World = serde_json::from_str(
        &serde_json::to_string(&World::new())
            .unwrap()
            .replace("\"next_entity\":0", "\"next_entity\":2"),
    )
    .unwrap();
    assert_eq!(world.hash(), empty.hash());
}

#[test]
fn find_walks_entities_with_a_key_in_id_order_and_sees_the_transaction() {
    let mut world = World::new();
    let mut txn = world.begin();
    let e0 = txn.spawn(Owner::Region);
    let e1 = txn.spawn(Owner::Region);
    let e2 = txn.spawn(Owner::Region);
    txn.set(&world, e0, "a", Value::Num(0.0)).unwrap();
    txn.set(&world, e1, "b", Value::Num(1.0)).unwrap();
    txn.set(&world, e2, "a", Value::Num(2.0)).unwrap();
    world.commit(txn).unwrap();

    let walk = |txn: &aether_world::Txn, world: &World| {
        let mut out = Vec::new();
        let mut at = None;
        while let (Some(id), _) = txn.find(world, "a", at) {
            out.push(id);
            at = Some(id);
        }
        out
    };
    let txn = world.begin();
    assert_eq!(walk(&txn, &world), vec![e0, e2]);

    let mut txn = world.begin();
    txn.despawn(&world, e0).unwrap();
    txn.set(&world, e1, "a", Value::Bool(true)).unwrap();
    txn.remove(&world, e2, "a").unwrap();
    let e3 = txn.spawn(Owner::Region);
    txn.set(&world, e3, "a", Value::Num(3.0)).unwrap();
    assert_eq!(walk(&txn, &world), vec![e1, e3]);
    world.commit(txn).unwrap();
    assert_eq!(walk(&world.begin(), &world), vec![e1, e3]);
}

#[test]
fn usage_tracks_entities_and_bytes_per_owner() {
    let ana = Owner::Player(PlayerId("ana".into()));
    let mut world = World::new();
    let mut txn = world.begin();
    let id = txn.spawn(ana.clone());
    txn.set(&world, id, "name", Value::Text("abcd".into()))
        .unwrap();
    assert_eq!(
        txn.usage_delta(&world).get(&ana),
        Some(&(1, (ENTITY_OVERHEAD_BYTES + 4 + 4) as i64))
    );
    world.commit(txn).unwrap();
    assert_eq!(
        world.usage(&ana),
        Usage {
            entities: 1,
            bytes: ENTITY_OVERHEAD_BYTES + 8
        }
    );
    let mut txn = world.begin();
    txn.despawn(&world, id).unwrap();
    world.commit(txn).unwrap();
    assert_eq!(world.usage(&ana), Usage::default());
}
