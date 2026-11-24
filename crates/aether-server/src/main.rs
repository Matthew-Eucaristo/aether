use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use aether_harness::{Bench, Claude, Event, HarnessError, realize};
use aether_kernel::{Action, Actor, BlobHash, Input, InputOutcome, Kernel, Law};
use aether_store::Store;
use aether_world::{Entity, EntityId, PlayerId, Value};
use axum::Router;
use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, oneshot};

const TICK: Duration = Duration::from_millis(100);
const CHECKPOINT_EVERY: u64 = 600;
const MAX_MESSAGE_BYTES: usize = 2 << 20;
const QUEUE: usize = 4096;
const INDEX: &str = include_str!("../web/index.html");
const EXAMPLES: [(&str, &str); 12] = [
    ("house", include_str!("../../../capsules/house.wat")),
    ("blackhole", include_str!("../../../capsules/blackhole.wat")),
    ("helix", include_str!("../../../capsules/helix.wat")),
    ("orbit", include_str!("../../../capsules/orbit.wat")),
    ("bounce", include_str!("../../../capsules/bounce.wat")),
    ("dice", include_str!("../../../capsules/dice.wat")),
    ("spawner", include_str!("../../../capsules/spawner.wat")),
    ("thief", include_str!("../../../capsules/thief.wat")),
    ("greedy", include_str!("../../../capsules/greedy.wat")),
    ("deep", include_str!("../../../capsules/deep.wat")),
    ("hog", include_str!("../../../capsules/hog.wat")),
    ("rollback", include_str!("../../../capsules/rollback.wat")),
];

struct Frame {
    text: String,
    halfs: Option<Bytes>,
}

type Update = Arc<Frame>;

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

fn wire_entity(e: &Entity, halfs: &mut Vec<u16>) -> serde_json::Value {
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
    serde_json::json!({ "owner": e.owner, "components": components })
}

enum Request {
    Join {
        actor: Actor,
        reply: oneshot::Sender<(String, broadcast::Receiver<Update>)>,
    },
    Act {
        input: Input,
        reply: Option<oneshot::Sender<InputOutcome>>,
    },
    Inspect(Box<dyn FnOnce(&Kernel) + Send>),
    Upload {
        bytes: Vec<u8>,
        reply: oneshot::Sender<Result<BlobHash, String>>,
    },
    Blob {
        hash: BlobHash,
        reply: oneshot::Sender<Option<Arc<[u8]>>>,
    },
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerMsg<'a> {
    Hello {
        you: &'a Actor,
        state: &'a aether_kernel::State,
        head: BlobHash,
    },
    Tick {
        report: &'a aether_kernel::TickReport,
        inputs: &'a [Input],
        changed: BTreeMap<EntityId, Option<serde_json::Value>>,
        halfs: usize,
        capsules: &'a BTreeMap<aether_world::CapsuleId, aether_kernel::Capsule>,
        law: &'a Law,
    },
    Uploaded {
        id: u64,
        result: Result<BlobHash, String>,
    },
    Busy,
    Harness {
        event: &'a Event,
    },
    HarnessDone {
        reply: &'a str,
        capsules: &'a [u64],
    },
    HarnessFailed {
        error: String,
    },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMsg {
    Act { action: Action },
    Upload { id: u64, source: String },
    Intent { text: String },
}

#[derive(Clone)]
struct App {
    requests: mpsc::SyncSender<Request>,
    token: Arc<str>,
    model: Option<Arc<Claude>>,
}

struct WorldBench(mpsc::SyncSender<Request>);

fn unavailable<T>(_: T) -> HarnessError {
    HarnessError::World("the world loop is busy or stopped".into())
}

impl Bench for WorldBench {
    async fn read<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Kernel) -> T + Send + 'static,
    ) -> Result<T, HarnessError> {
        let (tx, rx) = oneshot::channel();
        self.0
            .try_send(Request::Inspect(Box::new(move |k| {
                let _ = tx.send(f(k));
            })))
            .map_err(unavailable)?;
        rx.await.map_err(unavailable)
    }

    async fn upload(&self, bytes: Vec<u8>) -> Result<BlobHash, HarnessError> {
        let (reply, rx) = oneshot::channel();
        self.0
            .try_send(Request::Upload { bytes, reply })
            .map_err(unavailable)?;
        rx.await.map_err(unavailable)?.map_err(HarnessError::World)
    }

    async fn act(&self, input: Input) -> Result<InputOutcome, HarnessError> {
        let (reply, rx) = oneshot::channel();
        self.0
            .try_send(Request::Act {
                input,
                reply: Some(reply),
            })
            .map_err(unavailable)?;
        rx.await.map_err(unavailable)
    }
}

#[derive(Deserialize)]
struct Who {
    token: Option<String>,
    player: Option<String>,
}

fn random_u64() -> u64 {
    let mut bytes = [0u8; 8];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .expect("the OS provides randomness");
    u64::from_le_bytes(bytes)
}

fn world_loop(
    mut store: Store,
    requests: mpsc::Receiver<Request>,
    updates: broadcast::Sender<Update>,
) {
    let mut next = Instant::now() + TICK;
    loop {
        let mut inputs = Vec::new();
        let mut waiting: Vec<(usize, oneshot::Sender<InputOutcome>)> = Vec::new();
        loop {
            let timeout = next.saturating_duration_since(Instant::now());
            let request = match requests.recv_timeout(timeout) {
                Ok(r) => r,
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            };
            match request {
                Request::Join { actor, reply } => {
                    let receiver = updates.subscribe();
                    let k = store.kernel();
                    let hello = serde_json::to_string(&ServerMsg::Hello {
                        you: &actor,
                        state: k.state(),
                        head: k.state_hash(),
                    })
                    .expect("state serializes");
                    let _ = reply.send((hello, receiver));
                }
                Request::Blob { hash, reply } => {
                    let _ = reply.send(store.kernel().blobs().get(&hash).cloned());
                }
                Request::Inspect(f) => f(store.kernel()),
                Request::Upload { bytes, reply } => {
                    let max = store.kernel().state().law.max_module_bytes;
                    let result = if bytes.len() as u64 > max {
                        Err(format!("module source is over {max} bytes"))
                    } else {
                        store.add_blob(bytes).map_err(|e| e.to_string())
                    };
                    let _ = reply.send(result);
                }
                Request::Act { input, reply } => {
                    if let Some(wasm) = input.action.blob()
                        && store.kernel().blobs().get(wasm).is_none()
                    {
                        if let Some(reply) = reply {
                            let _ = reply.send(InputOutcome::Rejected(format!(
                                "blob {wasm} was never uploaded"
                            )));
                        }
                        continue;
                    }
                    if let Some(reply) = reply {
                        waiting.push((inputs.len(), reply));
                    }
                    inputs.push(input);
                }
            }
        }
        next = (next + TICK).max(Instant::now());
        let report = match store.step(inputs.clone()) {
            Ok(report) => report,
            Err(e) => {
                eprintln!("world stopped: {e}");
                return;
            }
        };
        for (index, reply) in waiting {
            let _ = reply.send(report.inputs[index].clone());
        }
        let k = store.kernel();
        let mut halfs = Vec::new();
        let changed = report
            .touched
            .iter()
            .map(|id| {
                (
                    *id,
                    k.world().entity(*id).map(|e| wire_entity(e, &mut halfs)),
                )
            })
            .collect();
        let text = serde_json::to_string(&ServerMsg::Tick {
            report: &report,
            inputs: &inputs,
            changed,
            halfs: halfs.len(),
            capsules: &k.state().capsules,
            law: &k.state().law,
        })
        .expect("tick serializes");
        let halfs = (!halfs.is_empty()).then(|| {
            Bytes::from(
                halfs
                    .iter()
                    .flat_map(|h| h.to_le_bytes())
                    .collect::<Vec<u8>>(),
            )
        });
        let _ = updates.send(Arc::new(Frame { text, halfs }));
        if report.tick % CHECKPOINT_EVERY == 0
            && let Err(e) = store.checkpoint()
        {
            eprintln!("checkpoint failed: {e}");
        }
    }
}

fn valid_player(name: &str) -> bool {
    (1..=32).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

async fn ws(ws: WebSocketUpgrade, Query(who): Query<Who>, State(app): State<App>) -> Response {
    let actor = match (who.token, who.player) {
        (Some(token), _) if *token == *app.token => Actor::Admin,
        (None, Some(name)) if valid_player(&name) => Actor::Player(PlayerId(name)),
        _ => return (StatusCode::FORBIDDEN, "bad token or player name").into_response(),
    };
    ws.max_message_size(MAX_MESSAGE_BYTES)
        .max_frame_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| session(socket, actor, app))
}

async fn blob(Path(hex): Path<String>, State(app): State<App>) -> Response {
    let Some(hash) = BlobHash::from_hex(&hex) else {
        return (StatusCode::BAD_REQUEST, "expected a 64 character hex hash").into_response();
    };
    let (reply, done) = oneshot::channel();
    if app
        .requests
        .try_send(Request::Blob { hash, reply })
        .is_err()
    {
        return (StatusCode::SERVICE_UNAVAILABLE, "the world is busy").into_response();
    }
    match done.await {
        Ok(Some(bytes)) => (
            [("content-type", "text/plain; charset=utf-8")],
            bytes.to_vec(),
        )
            .into_response(),
        _ => (StatusCode::NOT_FOUND, "no such blob").into_response(),
    }
}

async fn send_busy(socket: &mut WebSocket) -> Result<(), axum::Error> {
    let msg = serde_json::to_string(&ServerMsg::Busy).expect("busy serializes");
    socket.send(Message::text(msg)).await
}

async fn session(mut socket: WebSocket, actor: Actor, app: App) {
    let (reply, joined) = oneshot::channel();
    if app
        .requests
        .try_send(Request::Join {
            actor: actor.clone(),
            reply,
        })
        .is_err()
    {
        return;
    }
    let Ok((hello, mut updates)) = joined.await else {
        return;
    };
    if socket.send(Message::text(hello)).await.is_err() {
        return;
    }
    let (harness_tx, mut harness_rx) = tokio::sync::mpsc::unbounded_channel::<(String, bool)>();
    let mut running = false;
    loop {
        tokio::select! {
            incoming = socket.recv() => {
                let text = match incoming {
                    Some(Ok(Message::Text(text))) => text,
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return,
                    Some(Ok(_)) => continue,
                };
                match serde_json::from_str::<ClientMsg>(text.as_str()) {
                    Ok(ClientMsg::Act { action }) => {
                        let input = Input { actor: actor.clone(), action };
                        match app.requests.try_send(Request::Act { input, reply: None }) {
                            Ok(()) => {}
                            Err(mpsc::TrySendError::Full(_)) => {
                                if send_busy(&mut socket).await.is_err() {
                                    return;
                                }
                            }
                            Err(mpsc::TrySendError::Disconnected(_)) => return,
                        }
                    }
                    Ok(ClientMsg::Upload { id, source }) => {
                        let (reply, done) = oneshot::channel();
                        match app.requests.try_send(Request::Upload { bytes: source.into_bytes(), reply }) {
                            Ok(()) => {}
                            Err(mpsc::TrySendError::Full(_)) => {
                                if send_busy(&mut socket).await.is_err() {
                                    return;
                                }
                                continue;
                            }
                            Err(mpsc::TrySendError::Disconnected(_)) => return,
                        }
                        let result = done.await.unwrap_or_else(|_| Err("world stopped".into()));
                        let msg = serde_json::to_string(&ServerMsg::Uploaded { id, result })
                            .expect("upload reply serializes");
                        if socket.send(Message::text(msg)).await.is_err() {
                            return;
                        }
                    }
                    Ok(ClientMsg::Intent { text }) => {
                        let error = match (&app.model, running) {
                            (None, _) => Some("set ANTHROPIC_API_KEY in .env or the environment, then restart the server, to enable the AI harness".to_owned()),
                            (_, true) => Some("an intent from this session is still being worked on".to_owned()),
                            (Some(model), false) => {
                                running = true;
                                let model = model.clone();
                                let bench = WorldBench(app.requests.clone());
                                let actor = actor.clone();
                                let tx = harness_tx.clone();
                                eprintln!("intent from {actor:?}: {text}");
                                tokio::spawn(async move {
                                    let progress = tx.clone();
                                    let result = realize(&*model, &bench, actor, &text, move |event| {
                                        let msg = serde_json::to_string(&ServerMsg::Harness { event: &event })
                                            .expect("harness events serialize");
                                        let _ = progress.send((msg, false));
                                    })
                                    .await;
                                    let msg = match &result {
                                        Ok(outcome) => serde_json::to_string(&ServerMsg::HarnessDone {
                                            reply: &outcome.reply,
                                            capsules: &outcome.capsules,
                                        }),
                                        Err(e) => serde_json::to_string(&ServerMsg::HarnessFailed { error: e.to_string() }),
                                    }
                                    .expect("harness results serialize");
                                    eprintln!("intent finished: {}", match &result { Ok(o) => format!("{} turns, capsules {:?}", o.turns, o.capsules), Err(e) => e.to_string() });
                                    let _ = tx.send((msg, true));
                                });
                                None
                            }
                        };
                        if let Some(error) = error {
                            let msg = serde_json::to_string(&ServerMsg::HarnessFailed { error }).expect("serializes");
                            if socket.send(Message::text(msg)).await.is_err() {
                                return;
                            }
                        }
                    }
                    Err(e) => eprintln!("ignored a malformed client message: {e}"),
                }
            }
            Some((msg, finished)) = harness_rx.recv() => {
                if finished {
                    running = false;
                }
                if socket.send(Message::text(msg)).await.is_err() {
                    return;
                }
            }
            update = updates.recv() => {
                let Ok(update) = update else { return };
                if socket.send(Message::text(update.text.clone())).await.is_err() {
                    return;
                }
                if let Some(halfs) = &update.halfs
                    && socket.send(Message::Binary(halfs.clone())).await.is_err()
                {
                    return;
                }
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().unwrap_or_else(|| "world-data".into()));
    let port: u16 = args
        .next()
        .map(|p| p.parse().expect("port must be a number"))
        .unwrap_or(7878);

    match dotenvy::dotenv() {
        Ok(path) => println!("loaded settings from {}", path.display()),
        Err(e) if e.not_found() => {}
        Err(e) => panic!("cannot read .env: {e}"),
    }
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .unwrap_or_else(|e| panic!("cannot listen on port {port}: {e}"));

    let store = if dir.join("snapshot.json").exists() {
        Store::open(&dir)
    } else {
        Store::create(&dir, Law::default(), random_u64())
    }
    .unwrap_or_else(|e| panic!("cannot open the world in {}: {e}", dir.display()));
    println!(
        "world at {} is on tick {}",
        dir.display(),
        store.kernel().world().tick()
    );

    let (requests, inbox) = mpsc::sync_channel(QUEUE);
    let (updates, _) = broadcast::channel(256);
    std::thread::spawn(move || world_loop(store, inbox, updates));

    let token: Arc<str> = format!("{:016x}{:016x}", random_u64(), random_u64()).into();
    let examples: BTreeMap<&str, &str> = EXAMPLES.into_iter().collect();
    let examples = serde_json::to_string(&examples).expect("examples serialize");
    let model = Claude::from_env().map(Arc::new);
    match &model {
        Some(m) => println!("AI harness: {}", aether_harness::Model::name(&**m)),
        None => println!("AI harness: off (set ANTHROPIC_API_KEY in .env or the environment)"),
    }
    let app = App {
        requests,
        token: token.clone(),
        model,
    };
    let router = Router::new()
        .route("/", get(|| async { Html(INDEX) }))
        .route(
            "/capsules",
            get(move || async move { ([("content-type", "application/json")], examples) }),
        )
        .route("/ws", get(ws))
        .route("/blob/{hash}", get(blob))
        .with_state(app);

    println!("admin:  http://127.0.0.1:{port}/?token={token}");
    println!("player: http://127.0.0.1:{port}/?player=ana");
    axum::serve(listener, router).await.expect("server runs");
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
