use aether_world::{CapsuleId, EntityId, PlayerId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stake {
    Entity(EntityId),
    Capsule(CapsuleId),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pledge {
    pub stake: Stake,
    pub covenant: CapsuleId,
    pub from: PlayerId,
}

pub fn of(pledges: &[Pledge], stake: Stake) -> Option<&Pledge> {
    pledges
        .binary_search_by(|p| p.stake.cmp(&stake))
        .ok()
        .map(|i| &pledges[i])
}

pub fn stake(kind: i32, id: i64) -> Option<Stake> {
    let id = u64::try_from(id).ok()?;
    match kind {
        0 => Some(Stake::Entity(EntityId(id))),
        1 => Some(Stake::Capsule(CapsuleId(id))),
        _ => None,
    }
}
