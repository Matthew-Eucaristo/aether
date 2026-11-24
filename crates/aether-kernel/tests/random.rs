use std::collections::BTreeMap;

use aether_kernel::{
    Action, Actor, BlobHash, Budget, Fault, Grant, Input, InputOutcome, Kernel, Law, State,
};
use aether_world::{CapsuleId, EntityId, Owner, PlayerId, Usage, Value};

const CAPSULES: [&str; 9] = [
    include_str!("../../../capsules/bounce.wat"),
    include_str!("../../../capsules/dice.wat"),
    include_str!("../../../capsules/greedy.wat"),
    include_str!("../../../capsules/thief.wat"),
    include_str!("../../../capsules/spawner.wat"),
    include_str!("../../../capsules/hog.wat"),
    include_str!("../../../capsules/rollback.wat"),
    include_str!("../../../capsules/deep.wat"),
    include_str!("../../../capsules/wager.wat"),
];

const KEYS: [&str; 10] = [
    "pos.x", "open", "name", "hp", "bad key", "", "body", "pos.z", "vel.x", "shape",
];

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }
}

fn actor(rng: &mut Rng) -> Actor {
    match rng.below(4) {
        0 | 1 => Actor::Admin,
        2 => Actor::Player(PlayerId("ana".into())),
        _ => Actor::Player(PlayerId("budi".into())),
    }
}

fn value(rng: &mut Rng) -> Value {
    let nums = [
        0.0,
        -0.0,
        1.5,
        -7.25,
        1e308,
        f64::MIN_POSITIVE,
        5e-324,
        f64::NAN,
        f64::INFINITY,
    ];
    match rng.below(4) {
        0 => Value::Bool(rng.chance(50)),
        1 => Value::Text("x".repeat(if rng.chance(5) { 5000 } else { 3 })),
        _ => Value::Num(*rng.pick(&nums)),
    }
}

fn entity(rng: &mut Rng, k: &Kernel) -> EntityId {
    let ids: Vec<EntityId> = k.world().entities().map(|(id, _)| id).collect();
    if ids.is_empty() || rng.chance(10) {
        EntityId(rng.below(64))
    } else {
        *rng.pick(&ids)
    }
}

fn budget(rng: &mut Rng) -> Budget {
    Budget {
        fuel_per_tick: *rng.pick(&[0, 500, 20_000, 2_000_000]),
        memory_bytes: *rng.pick(&[0, 65_536, 1 << 20, 1 << 30]),
        writes_per_tick: *rng.pick(&[0, 1, 4, 5_000]),
        spawns_per_tick: *rng.pick(&[0, 1, 100]),
    }
}

fn law(rng: &mut Rng) -> Law {
    Law {
        max_entities: *rng.pick(&[3, 20, 10_000]),
        max_capsules: *rng.pick(&[1, 4, 256]),
        max_module_bytes: *rng.pick(&[64, 1 << 20]),
        max_clist_len: *rng.pick(&[1, 3, 4096]),
        max_consecutive_faults: *rng.pick(&[1, 3, 50]),
        allow_spawn: rng.chance(70),
        max_inputs_per_actor: *rng.pick(&[1, 2, 32]),
        region_fuel_per_tick: *rng.pick(&[1_000, 300_000, 50_000_000]),
        player_quota: Usage {
            entities: *rng.pick(&[1, 4, 500]),
            bytes: *rng.pick(&[100, 2_000, 512 << 10]),
        },
        ceiling: budget(rng),
        terrain: aether_kernel::Terrain {
            seed: *rng.pick(&[0, 7, u32::MAX]),
            relief: *rng.pick(&[0, 30, 2_000]),
            scale: *rng.pick(&[0, 1, 200]),
            plaza: *rng.pick(&[0, 48]),
        },
        max_bodies: *rng.pick(&[0, 3, 2048]),
    }
}

fn input(rng: &mut Rng, k: &Kernel, wasm: &[BlobHash]) -> Input {
    let capsule = CapsuleId(rng.below(k.state().next_capsule + 2));
    match rng.below(14) {
        0 | 1 => Input {
            actor: actor(rng),
            action: Action::Spawn {
                components: (0..rng.below(4))
                    .map(|_| ((*rng.pick(&KEYS)).to_owned(), value(rng)))
                    .collect::<BTreeMap<_, _>>(),
            },
        },
        2 => Input {
            actor: actor(rng),
            action: Action::Set {
                entity: entity(rng, k),
                key: (*rng.pick(&KEYS)).to_owned(),
                value: value(rng),
            },
        },
        3 => Input {
            actor: actor(rng),
            action: Action::Remove {
                entity: entity(rng, k),
                key: (*rng.pick(&KEYS)).to_owned(),
            },
        },
        4 => Input {
            actor: actor(rng),
            action: Action::Despawn {
                entity: entity(rng, k),
            },
        },
        5 | 6 => Input {
            actor: actor(rng),
            action: Action::Install {
                wasm: *rng.pick(wasm),
                grants: (0..rng.below(3))
                    .map(|_| Grant {
                        entity: entity(rng, k),
                        write: rng.chance(50),
                    })
                    .collect(),
                budget: budget(rng),
            },
        },
        7 => {
            if rng.chance(50) {
                Input {
                    actor: actor(rng),
                    action: Action::Uninstall { capsule },
                }
            } else {
                Input {
                    actor: actor(rng),
                    action: Action::Resume { capsule },
                }
            }
        }
        9 => Input {
            actor: actor(rng),
            action: Action::Upgrade {
                capsule,
                wasm: *rng.pick(wasm),
            },
        },
        10 => Input {
            actor: actor(rng),
            action: Action::Grant {
                capsule,
                grant: Grant {
                    entity: entity(rng, k),
                    write: rng.chance(50),
                },
            },
        },
        13 => Input {
            actor: actor(rng),
            action: Action::Pledge {
                covenant: capsule,
                stake: if rng.chance(50) {
                    aether_kernel::Stake::Entity(entity(rng, k))
                } else {
                    aether_kernel::Stake::Capsule(CapsuleId(rng.below(k.state().next_capsule + 2)))
                },
            },
        },
        12 => Input {
            actor: actor(rng),
            action: Action::Signal {
                entity: entity(rng, k),
                name: (*rng.pick(&KEYS)).to_owned(),
                value: *rng.pick(&[0.0, 1.0, f64::NAN]),
            },
        },
        11 => Input {
            actor: actor(rng),
            action: Action::Revoke {
                capsule,
                handle: rng.below(4) as u32,
            },
        },
        _ => Input {
            actor: actor(rng),
            action: Action::SetLaw {
                law: if rng.chance(50) {
                    Law::default()
                } else {
                    law(rng)
                },
            },
        },
    }
}

fn check_invariants(k: &Kernel, entity_limit: usize) {
    let state = k.state();
    state.world.validate().expect("world stays valid");
    assert!(
        state.world.len() <= entity_limit,
        "entity count grew past every region law in force this tick"
    );
    assert!(state.capsules.len() <= state.next_capsule as usize);
    for (id, capsule) in &state.capsules {
        assert!(id.0 < state.next_capsule);
        let own = state
            .world
            .entity(capsule.self_entity)
            .unwrap_or_else(|| panic!("capsule {} lost its own entity", id.0));
        assert_eq!(own.owner, Owner::Capsule(*id));
    }
    for (_, e) in state.world.entities() {
        if let Owner::Capsule(c) = e.owner {
            assert!(
                state.capsules.contains_key(&c),
                "entity owned by uninstalled capsule {}",
                c.0
            );
        }
    }
}

fn restore(k: &Kernel) -> Kernel {
    let json = serde_json::to_string(k.state()).expect("state serializes");
    let state: State = serde_json::from_str(&json).expect("state deserializes");
    assert_eq!(&state, k.state(), "snapshot round-trips exactly");
    Kernel::from_state(state, k.blobs().clone()).expect("snapshot restores")
}

#[derive(Default)]
struct Coverage {
    installed: u32,
    rejected: u32,
    clean_runs: u32,
    out_of_fuel: u32,
    depth_limit: u32,
    denied: u32,
    traps: u32,
    restored_ticks: u32,
}

fn run(seed: u64, ticks: u64, cov: &mut Coverage) {
    let mut rng = Rng(seed);
    let mut k = Kernel::new(Law::default(), seed);
    let wasm: Vec<BlobHash> = CAPSULES
        .iter()
        .map(|src| k.add_blob(src.as_bytes().to_vec()))
        .collect();
    let restore_at = 1 + rng.below(ticks);
    let mut twin: Option<Kernel> = None;
    let mut hashes = Vec::new();
    for tick in 0..ticks {
        if tick == restore_at {
            twin = Some(restore(&k));
        }
        let inputs: Vec<Input> = (0..rng.below(4))
            .map(|_| input(&mut rng, &k, &wasm))
            .collect();
        let before = k.world().len();
        let players = [PlayerId("ana".into()), PlayerId("budi".into())];
        let usage_before: Vec<Usage> = players.iter().map(|p| k.player_usage(p)).collect();
        let quota_limit = inputs
            .iter()
            .filter_map(|i| match i {
                Input {
                    action: Action::SetLaw { law },
                    ..
                } => Some(law.player_quota),
                _ => None,
            })
            .fold(k.state().law.player_quota, |a, b| Usage {
                entities: a.entities.max(b.entities),
                bytes: a.bytes.max(b.bytes),
            });
        let limit = inputs
            .iter()
            .filter_map(|i| match i {
                Input {
                    action: Action::SetLaw { law },
                    ..
                } => Some(law.max_entities),
                _ => None,
            })
            .fold(k.state().law.max_entities, u64::max);
        let report = k.step(inputs.clone());
        assert_eq!(report.state_hash, k.state_hash());
        check_invariants(&k, before.max(limit as usize));
        for (p, was) in players.iter().zip(&usage_before) {
            let now = k.player_usage(p);
            assert!(
                now.entities <= was.entities.max(quota_limit.entities),
                "{} exceeded the entity quota",
                p.0
            );
            assert!(
                now.bytes <= was.bytes.max(quota_limit.bytes),
                "{} exceeded the byte quota",
                p.0
            );
        }
        for outcome in &report.inputs {
            match outcome {
                InputOutcome::Installed(_) => cov.installed += 1,
                InputOutcome::Rejected(_) => cov.rejected += 1,
                _ => {}
            }
        }
        for r in &report.runs {
            match &r.fault {
                None => cov.clean_runs += 1,
                Some(Fault::OutOfFuel) => cov.out_of_fuel += 1,
                Some(Fault::DepthLimit) => cov.depth_limit += 1,
                Some(Fault::Denied(_)) => cov.denied += 1,
                Some(Fault::Trap(_)) => cov.traps += 1,
            }
        }
        if let Some(t) = twin.as_mut() {
            let r = t.step(inputs);
            assert_eq!(r, report, "restored twin diverged at tick {}", report.tick);
            cov.restored_ticks += 1;
        }
        hashes.push(report.state_hash);
    }
    let (replayed, replay_hashes) =
        Kernel::replay(k.journal(), k.blobs().clone()).expect("journal replays");
    assert_eq!(replay_hashes, hashes, "replay diverged for seed {seed}");
    assert_eq!(replayed.state(), k.state());
}

#[test]
fn random_histories_replay_and_restore_identically() {
    let mut cov = Coverage::default();
    for seed in 0..48 {
        run(seed, 60, &mut cov);
    }
    let Coverage {
        installed,
        rejected,
        clean_runs,
        out_of_fuel,
        depth_limit,
        denied,
        traps,
        restored_ticks,
    } = cov;
    eprintln!(
        "installed {installed}, rejected {rejected}, clean {clean_runs}, out_of_fuel {out_of_fuel}, depth_limit {depth_limit}, denied {denied}, traps {traps}, restored ticks {restored_ticks}"
    );
    for (name, n) in [
        ("installed", installed),
        ("rejected", rejected),
        ("clean runs", clean_runs),
        ("out of fuel", out_of_fuel),
        ("depth limits", depth_limit),
        ("denied", denied),
        ("traps", traps),
        ("restored ticks", restored_ticks),
    ] {
        assert!(n >= 20, "generator exercised too few {name}: {n}");
    }
}
