use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use aether_kernel::{Actor, Input, Kernel, TickReport};
use aether_world::{Entity, EntityId, Place, Value};
use axum::body::Bytes;
use tokio::sync::mpsc;

pub const VIEW_RADIUS: f64 = 192.0;
pub const OUTBOX: usize = 64;

pub struct Frame {
    pub text: String,
    pub halfs: Option<Bytes>,
}

pub type Update = Arc<Frame>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Focus {
    pub dim: i64,
    pub x: f64,
    pub y: f64,
}

pub struct Viewer {
    pub actor: Actor,
    pub focus: Focus,
    pub tx: mpsc::Sender<Update>,
    known: BTreeSet<EntityId>,
    fresh: bool,
}

impl Viewer {
    pub fn new(actor: Actor, tx: mpsc::Sender<Update>) -> Viewer {
        Viewer {
            actor,
            focus: Focus {
                dim: 0,
                x: 0.0,
                y: 0.0,
            },
            tx,
            known: BTreeSet::new(),
            fresh: true,
        }
    }
}

pub enum Sent {
    Delivered,
    Dropped,
    Gone,
}

fn half(x: f32) -> u16 {
    let bits = x.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mant = bits & 0x7f_ffff;
    if exp >= 31 {
        return sign | 0x7bff;
    }
    if exp <= 0 {
        if exp < -10 {
            return sign;
        }
        let m = (mant | 0x80_0000) >> (1 - exp);
        return sign | ((m + 0x1000) >> 13) as u16;
    }
    let rounded = (((exp as u32) << 10) | (mant >> 13)) + ((mant >> 12) & 1);
    sign | rounded.min(0x7bff) as u16
}

fn encode(e: &Entity) -> (serde_json::Value, Vec<u16>) {
    let mut halfs = Vec::new();
    let components: serde_json::Map<String, serde_json::Value> = e
        .components
        .iter()
        .map(|(k, v)| {
            let encoded = match v {
                Value::Nums(n) => {
                    let at = halfs.len();
                    halfs.extend(n.iter().map(|x| half(*x as f32)));
                    serde_json::json!({ "halfs": [at, n.len()] })
                }
                other => serde_json::to_value(other).expect("values serialize"),
            };
            (k.clone(), encoded)
        })
        .collect();
    (
        serde_json::json!({ "owner": e.owner, "components": components }),
        halfs,
    )
}

fn shifted(mut v: serde_json::Value, base: usize) -> serde_json::Value {
    if base > 0
        && let Some(components) = v["components"].as_object_mut()
    {
        for value in components.values_mut() {
            if let Some(at) = value
                .get_mut("halfs")
                .and_then(|h| h.get_mut(0))
                .filter(|a| a.is_u64())
            {
                *at = (at.as_u64().expect("checked") + base as u64).into();
            }
        }
    }
    v
}

#[derive(Default)]
pub struct Cache {
    entities: HashMap<EntityId, (serde_json::Value, Vec<u16>)>,
    globals: Option<Vec<EntityId>>,
    shared: Option<Shared>,
}

struct Shared {
    capsules: String,
    law: String,
    head: String,
}

impl Cache {
    fn shared(&mut self, k: &Kernel) -> &Shared {
        self.shared.get_or_insert_with(|| Shared {
            capsules: serde_json::to_string(&k.state().capsules).expect("capsules serialize"),
            law: serde_json::to_string(&k.state().law).expect("law serializes"),
            head: serde_json::to_string(&k.state_hash()).expect("hashes serialize"),
        })
    }

    fn globals(&mut self, k: &Kernel) -> &[EntityId] {
        self.globals.get_or_insert_with(|| {
            k.world()
                .entities()
                .filter(|(_, e)| Place::of(e).is_none())
                .map(|(id, _)| id)
                .collect()
        })
    }

    fn entities(&mut self, k: &Kernel, ids: &BTreeMap<EntityId, bool>) -> (String, Vec<u16>) {
        let mut halfs = Vec::new();
        let mut out = serde_json::Map::new();
        for (id, present) in ids {
            let value = match k.world().entity(*id).filter(|_| *present) {
                None => serde_json::Value::Null,
                Some(e) => {
                    let (v, hs) = self.entities.entry(*id).or_insert_with(|| encode(e));
                    let v = shifted(v.clone(), halfs.len());
                    halfs.extend_from_slice(hs);
                    v
                }
            };
            out.insert(id.0.to_string(), value);
        }
        (
            serde_json::to_string(&out).expect("entities serialize"),
            halfs,
        )
    }
}

fn visible(k: &Kernel, focus: Focus) -> BTreeSet<EntityId> {
    k.world()
        .near(focus.dim, focus.x, focus.y, VIEW_RADIUS)
        .into_iter()
        .collect()
}

fn frame(text: String, halfs: Vec<u16>) -> Update {
    let halfs = (!halfs.is_empty()).then(|| {
        Bytes::from(
            halfs
                .iter()
                .flat_map(|h| h.to_le_bytes())
                .collect::<Vec<u8>>(),
        )
    });
    Arc::new(Frame { text, halfs })
}

fn deliver(viewer: &mut Viewer, update: Update) -> Sent {
    match viewer.tx.try_send(update) {
        Ok(()) => Sent::Delivered,
        Err(mpsc::error::TrySendError::Full(_)) => {
            viewer.fresh = true;
            Sent::Dropped
        }
        Err(mpsc::error::TrySendError::Closed(_)) => Sent::Gone,
    }
}

pub fn hello(k: &Kernel, viewer: &mut Viewer, cache: &mut Cache) -> Sent {
    let near = visible(k, viewer.focus);
    let mut ids: BTreeMap<EntityId, bool> = near.iter().map(|id| (*id, true)).collect();
    ids.extend(cache.globals(k).iter().map(|id| (*id, true)));
    let (entities, halfs) = cache.entities(k, &ids);
    let shared = cache.shared(k);
    let you = serde_json::to_string(&viewer.actor).expect("actors serialize");
    let text = format!(
        "{{\"type\":\"hello\",\"you\":{you},\"head\":{},\"halfs\":{},\"state\":{{\"law\":{},\"capsules\":{},\"world\":{{\"tick\":{},\"entities\":{entities}}}}}}}",
        shared.head,
        halfs.len(),
        shared.law,
        shared.capsules,
        k.world().tick(),
    );
    viewer.known = near;
    viewer.fresh = false;
    deliver(viewer, frame(text, halfs))
}

pub struct TickText {
    report: String,
    inputs: String,
}

impl TickText {
    pub fn new(report: &TickReport, inputs: &[Input]) -> TickText {
        TickText {
            report: serde_json::to_string(report).expect("reports serialize"),
            inputs: serde_json::to_string(inputs).expect("inputs serialize"),
        }
    }
}

pub fn tick(
    k: &Kernel,
    report: &TickReport,
    text: &TickText,
    viewer: &mut Viewer,
    cache: &mut Cache,
) -> Sent {
    if viewer.fresh {
        if viewer.tx.capacity() < OUTBOX / 2 {
            return Sent::Dropped;
        }
        return hello(k, viewer, cache);
    }
    let near = visible(k, viewer.focus);
    let mut ids: BTreeMap<EntityId, bool> = BTreeMap::new();
    let mut left: BTreeSet<EntityId> = BTreeSet::new();
    for id in &report.touched {
        match k.world().entity(*id) {
            None => {
                ids.insert(*id, false);
            }
            Some(e) if Place::of(e).is_none() || near.contains(id) => {
                ids.insert(*id, true);
            }
            Some(_) => {
                left.insert(*id);
            }
        }
    }
    for id in near.difference(&viewer.known) {
        ids.insert(*id, true);
    }
    left.extend(
        viewer
            .known
            .difference(&near)
            .filter(|id| !ids.contains_key(id)),
    );
    let left: Vec<u64> = left.into_iter().map(|id| id.0).collect();
    let (changed, halfs) = cache.entities(k, &ids);
    let shared = cache.shared(k);
    let text = format!(
        "{{\"type\":\"tick\",\"report\":{},\"inputs\":{},\"capsules\":{},\"law\":{},\"halfs\":{},\"changed\":{changed},\"left\":{}}}",
        text.report,
        text.inputs,
        shared.capsules,
        shared.law,
        halfs.len(),
        serde_json::to_string(&left).expect("ids serialize"),
    );
    viewer.known = near;
    deliver(viewer, frame(text, halfs))
}

#[cfg(test)]
mod tests {
    use super::half;

    #[test]
    fn half_floats_round_to_nearest_and_saturate() {
        assert_eq!(half(0.0), 0x0000);
        assert_eq!(half(-0.0), 0x8000);
        assert_eq!(half(1.0), 0x3c00);
        assert_eq!(half(-2.5), 0xc100);
        assert_eq!(half(0.1), 0x2e66);
        assert_eq!(half(65504.0), 0x7bff);
        assert_eq!(half(1.0e9), 0x7bff);
        assert_eq!(half(6.0e-8), 0x0001);
        assert_eq!(half(1.0e-12), 0x0000);
    }
}
