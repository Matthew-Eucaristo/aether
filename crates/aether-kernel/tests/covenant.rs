use std::collections::BTreeMap;

use aether_kernel::{Action, Actor, InputOutcome, Kernel, Law, Stake, TickReport};
use aether_world::{CapsuleId, EntityId, Owner, PlayerId, Value};

const WAGER: &str = include_str!("../../../capsules/wager.wat");
const HOUSE: &str = include_str!("../../../capsules/house.wat");

fn player(name: &str) -> Actor {
    Actor::Player(PlayerId(name.into()))
}

fn act(k: &mut Kernel, actor: Actor, action: Action) -> (InputOutcome, TickReport) {
    let r = k.step(vec![aether_kernel::Input { actor, action }]);
    (r.inputs[0].clone(), r)
}

fn install(k: &mut Kernel, actor: Actor, src: &str) -> CapsuleId {
    let wasm = k.add_blob(src.as_bytes().to_vec());
    match act(
        k,
        actor,
        Action::Install {
            wasm,
            grants: vec![],
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

fn pledge(k: &mut Kernel, who: &str, covenant: CapsuleId, stake: Stake) -> InputOutcome {
    act(k, player(who), Action::Pledge { covenant, stake }).0
}

fn owner(k: &Kernel, id: EntityId) -> Owner {
    k.world().entity(id).unwrap().owner.clone()
}

#[test]
fn a_wager_hands_every_stake_to_one_winner() {
    let mut k = Kernel::new(Law::default(), 11);
    let wager = install(&mut k, Actor::Admin, WAGER);
    let a = thing(&mut k, "ana");
    let b = thing(&mut k, "budi");
    assert_eq!(
        pledge(&mut k, "ana", wager, Stake::Entity(a)),
        InputOutcome::Applied
    );
    let (outcome, _) = act(&mut k, player("ana"), Action::Despawn { entity: a });
    assert!(
        matches!(outcome, InputOutcome::Rejected(ref why) if why.contains("pledged")),
        "a pledged stake is frozen: {outcome:?}"
    );
    assert!(
        matches!(
            pledge(&mut k, "budi", wager, Stake::Entity(a)),
            InputOutcome::Rejected(_)
        ),
        "nobody pledges what they do not own"
    );
    let (outcome, r) = act(
        &mut k,
        player("budi"),
        Action::Pledge {
            covenant: wager,
            stake: Stake::Entity(b),
        },
    );
    assert_eq!(outcome, InputOutcome::Applied);
    let mut said: Vec<String> = r.runs.iter().flat_map(|run| run.log.clone()).collect();
    for _ in 0..3 {
        let r = k.step(vec![]);
        said.extend(r.runs.iter().flat_map(|run| run.log.clone()));
    }
    assert!(
        k.state().pledges.is_empty(),
        "the wager settled: {:?}",
        k.state().pledges
    );
    let winner = owner(&k, a);
    assert_eq!(owner(&k, b), winner, "one player took both stakes");
    let Owner::Player(name) = &winner else {
        panic!("a player won: {winner:?}")
    };
    assert!(
        said.iter()
            .any(|l| l == &format!("player:{} wins every stake", name.0)),
        "{said:?}"
    );
}

#[test]
fn a_lonely_pledge_comes_back_after_a_minute() {
    let mut k = Kernel::new(Law::default(), 3);
    let wager = install(&mut k, Actor::Admin, WAGER);
    let a = thing(&mut k, "ana");
    pledge(&mut k, "ana", wager, Stake::Entity(a));
    for _ in 0..590 {
        k.step(vec![]);
    }
    assert_eq!(k.state().pledges.len(), 1, "still waiting for an opponent");
    for _ in 0..20 {
        k.step(vec![]);
    }
    assert!(k.state().pledges.is_empty());
    assert_eq!(owner(&k, a), Owner::Player(PlayerId("ana".into())));
    let (outcome, _) = act(&mut k, player("ana"), Action::Despawn { entity: a });
    assert_eq!(
        outcome,
        InputOutcome::Applied,
        "a released stake is free again"
    );
}

#[test]
fn a_whole_capsule_can_be_the_stake_and_its_covenant_is_locked() {
    let mut k = Kernel::new(Law::default(), 5);
    let wager = install(&mut k, player("ana"), WAGER);
    let house = install(&mut k, player("budi"), HOUSE);
    let gem = thing(&mut k, "ana");
    assert_eq!(
        pledge(&mut k, "budi", wager, Stake::Capsule(house)),
        InputOutcome::Applied
    );
    let (outcome, _) = act(&mut k, player("budi"), Action::Uninstall { capsule: house });
    assert!(
        matches!(outcome, InputOutcome::Rejected(_)),
        "a staked capsule is frozen"
    );
    let (outcome, _) = act(&mut k, player("ana"), Action::Uninstall { capsule: wager });
    assert!(
        matches!(outcome, InputOutcome::Rejected(ref why) if why.contains("holds other players' pledges")),
        "the covenant's own author cannot pull it while others' stakes ride on it: {outcome:?}"
    );
    pledge(&mut k, "ana", wager, Stake::Entity(gem));
    for _ in 0..3 {
        k.step(vec![]);
    }
    assert!(k.state().pledges.is_empty());
    let winner = owner(&k, gem);
    let Owner::Player(name) = winner else {
        panic!()
    };
    assert_eq!(
        k.state().capsules[&house].installer,
        Actor::Player(name),
        "the winner now manages the staked capsule"
    );
}

#[test]
fn removing_a_covenant_returns_every_stake() {
    let mut k = Kernel::new(Law::default(), 8);
    let wager = install(&mut k, Actor::Admin, WAGER);
    let a = thing(&mut k, "ana");
    pledge(&mut k, "ana", wager, Stake::Entity(a));
    let (outcome, _) = act(&mut k, Actor::Admin, Action::Uninstall { capsule: wager });
    assert_eq!(outcome, InputOutcome::Applied);
    assert!(k.state().pledges.is_empty());
    assert_eq!(owner(&k, a), Owner::Player(PlayerId("ana".into())));
}
