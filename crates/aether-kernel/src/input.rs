use std::collections::BTreeMap;

use aether_world::{CapsuleId, EntityId, Owner, PlayerId, Value};
use serde::{Deserialize, Serialize};

use crate::blob::BlobHash;
use crate::capsule::{Budget, Grant};
use crate::law::Law;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Actor {
    Admin,
    Player(PlayerId),
}

impl Actor {
    pub fn owner(&self) -> Owner {
        match self {
            Actor::Admin => Owner::Region,
            Actor::Player(p) => Owner::Player(p.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Input {
    pub actor: Actor,
    pub action: Action,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Action {
    Spawn {
        components: BTreeMap<String, Value>,
    },
    Set {
        entity: EntityId,
        key: String,
        value: Value,
    },
    Remove {
        entity: EntityId,
        key: String,
    },
    Despawn {
        entity: EntityId,
    },
    Install {
        wasm: BlobHash,
        grants: Vec<Grant>,
        budget: Budget,
    },
    Uninstall {
        capsule: CapsuleId,
    },
    Upgrade {
        capsule: CapsuleId,
        wasm: BlobHash,
    },
    Grant {
        capsule: CapsuleId,
        grant: Grant,
    },
    Revoke {
        capsule: CapsuleId,
        handle: u32,
    },
    Resume {
        capsule: CapsuleId,
    },
    Signal {
        entity: EntityId,
        name: String,
        value: f64,
    },
    SetLaw {
        law: Law,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputOutcome {
    Applied,
    Spawned(EntityId),
    Installed(CapsuleId),
    Granted(u32),
    Rejected(String),
}

impl Action {
    pub fn blob(&self) -> Option<&BlobHash> {
        match self {
            Action::Install { wasm, .. } | Action::Upgrade { wasm, .. } => Some(wasm),
            _ => None,
        }
    }
}
