use aether_world::{CapsuleId, EntityId};
use serde::{Deserialize, Serialize};

use crate::input::Actor;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sender {
    Actor(Actor),
    Capsule(CapsuleId),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Signal {
    pub from: Sender,
    pub entity: EntityId,
    pub name: String,
    pub value: f64,
}

impl Sender {
    pub fn label(&self) -> String {
        match self {
            Sender::Actor(Actor::Admin) => "admin".into(),
            Sender::Actor(Actor::Player(p)) => format!("player:{}", p.0),
            Sender::Capsule(c) => format!("capsule:{}", c.0),
        }
    }
}
