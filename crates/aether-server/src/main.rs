use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use aether_harness::{Bench, Claude, Event, HarnessError, realize};
use aether_kernel::{Action, Actor, BlobHash, Input, InputOutcome, Kernel, Law};
use aether_store::Store;
use aether_world::PlayerId;
use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

mod view;

use view::{Cache, Focus, OUTBOX, Sent, TickText, Update, Viewer};

const TICK: Duration = Duration::from_millis(100);
const CHECKPOINT_EVERY: u64 = 600;
const MAX_MESSAGE_BYTES: usize = 2 << 20;
const QUEUE: usize = 4096;
const INDEX: &str = include_str!("../web/index.html");
const EXAMPLES: [(&str, &str); 16] = [
    ("house", include_str!("../../../capsules/house.wat")),
    ("realm", include_str!("../../../capsules/realm.wat")),
    ("rifle", include_str!("../../../capsules/rifle.wat")),
    ("launcher", include_str!("../../../capsules/launcher.wat")),
    ("wager", include_str!("../../../capsules/wager.wat")),
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

enum Request {
    Stop(oneshot::Sender<()>),
    Join {
        actor: Actor,
        reply: oneshot::Sender<(u64, tokio::sync::mpsc::Receiver<Update>)>,
    },
    Focus {
        viewer: u64,
        focus: Focus,
    },
    Leave {
        viewer: u64,
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
    Rejected {
        error: String,
    },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMsg {
    Act { action: Action },
    Upload { id: u64, source: String },
    Intent { text: String },
    Focus { dim: i64, x: f64, y: f64 },
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

fn world_loop(mut store: Store, requests: mpsc::Receiver<Request>) {
    let mut next = Instant::now() + TICK;
    let mut viewers: BTreeMap<u64, Viewer> = BTreeMap::new();
    let mut next_viewer = 0u64;
    let mut cache = Cache::default();
    loop {
        let mut inputs = Vec::new();
        let mut waiting: Vec<(usize, oneshot::Sender<InputOutcome>)> = Vec::new();
        let mut served = 0usize;
        loop {
            let timeout = next.saturating_duration_since(Instant::now());
            if timeout.is_zero() && served > 0 {
                break;
            }
            let request = match requests.recv_timeout(timeout) {
                Ok(r) => r,
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            };
            served += 1;
            match request {
                Request::Stop(done) => {
                    match store.checkpoint() {
                        Ok(()) => println!("checkpointed tick {}", store.kernel().world().tick()),
                        Err(e) => eprintln!("checkpoint failed: {e}"),
                    }
                    let _ = done.send(());
                    return;
                }
                Request::Join { actor, reply } => {
                    let (tx, rx) = tokio::sync::mpsc::channel(OUTBOX);
                    let mut viewer = Viewer::new(actor, tx);
                    if let Sent::Gone = view::hello(store.kernel(), &mut viewer, &mut cache) {
                        continue;
                    }
                    next_viewer += 1;
                    viewers.insert(next_viewer, viewer);
                    let _ = reply.send((next_viewer, rx));
                }
                Request::Focus { viewer, focus } => {
                    if let Some(v) = viewers.get_mut(&viewer)
                        && focus.x.is_finite()
                        && focus.y.is_finite()
                    {
                        v.focus = focus;
                    }
                }
                Request::Leave { viewer } => {
                    viewers.remove(&viewer);
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
        cache = Cache::default();
        let text = TickText::new(&report, &inputs);
        let k = store.kernel();
        viewers.retain(|_, viewer| {
            !matches!(
                view::tick(k, &report, &text, viewer, &mut cache),
                Sent::Gone
            )
        });
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

async fn session(socket: WebSocket, actor: Actor, app: App) {
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
    let Ok((viewer, updates)) = joined.await else {
        return;
    };
    serve(socket, actor, viewer, updates, &app).await;
    let _ = app.requests.try_send(Request::Leave { viewer });
}

async fn serve(
    mut socket: WebSocket,
    actor: Actor,
    viewer: u64,
    mut updates: tokio::sync::mpsc::Receiver<Update>,
    app: &App,
) {
    let (harness_tx, mut harness_rx) = tokio::sync::mpsc::unbounded_channel::<(String, bool)>();
    let mut running = false;
    loop {
        tokio::select! {
            incoming = socket.recv() => {
                let text = match incoming {
                    Some(Ok(Message::Text(text))) => text,
                    Some(Ok(Message::Close(_))) => {
                        let _ = socket.recv().await;
                        return;
                    }
                    Some(Err(_)) | None => return,
                    Some(Ok(_)) => continue,
                };
                match serde_json::from_str::<ClientMsg>(text.as_str()) {
                    Ok(ClientMsg::Act { action }) => {
                        let (reply, outcome) = match action.blob() {
                            Some(_) => {
                                let (reply, outcome) = oneshot::channel();
                                (Some(reply), Some(outcome))
                            }
                            None => (None, None),
                        };
                        let input = Input { actor: actor.clone(), action };
                        match app.requests.try_send(Request::Act { input, reply }) {
                            Ok(()) => {
                                if let Some(outcome) = outcome {
                                    let tx = harness_tx.clone();
                                    tokio::spawn(async move {
                                        if let Ok(InputOutcome::Rejected(error)) = outcome.await {
                                            let msg = serde_json::to_string(&ServerMsg::Rejected { error })
                                                .expect("rejections serialize");
                                            let _ = tx.send((msg, false));
                                        }
                                    });
                                }
                            }
                            Err(mpsc::TrySendError::Full(_)) => {
                                if send_busy(&mut socket).await.is_err() {
                                    return;
                                }
                            }
                            Err(mpsc::TrySendError::Disconnected(_)) => return,
                        }
                    }
                    Ok(ClientMsg::Focus { dim, x, y }) => {
                        let _ = app.requests.try_send(Request::Focus { viewer, focus: Focus { dim, x, y } });
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
                    Err(e) => {
                        let msg = serde_json::to_string(&ServerMsg::Rejected { error: format!("malformed message: {e}") })
                            .expect("rejections serialize");
                        if socket.send(Message::text(msg)).await.is_err() {
                            return;
                        }
                    }
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
                let Some(update) = update else { return };
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
    let port = listener
        .local_addr()
        .expect("a bound listener has an address")
        .port();

    let store = if dir.join("snapshot.json").exists() {
        Store::open(&dir)
    } else {
        Store::create(
            &dir,
            Law {
                terrain: aether_kernel::Terrain {
                    seed: random_u64() as u32,
                    relief: 24,
                    scale: 180,
                    plaza: 40,
                },
                ..Law::default()
            },
            random_u64(),
        )
    }
    .unwrap_or_else(|e| panic!("cannot open the world in {}: {e}", dir.display()));
    println!(
        "world at {} is on tick {}",
        dir.display(),
        store.kernel().world().tick()
    );

    let (requests, inbox) = mpsc::sync_channel(QUEUE);
    std::thread::spawn(move || world_loop(store, inbox));

    let token: Arc<str> = format!("{:016x}{:016x}", random_u64(), random_u64()).into();
    let examples: BTreeMap<&str, &str> = EXAMPLES.into_iter().collect();
    let examples = serde_json::to_string(&examples).expect("examples serialize");
    let model = Claude::from_env().map(Arc::new);
    match &model {
        Some(m) => println!("AI harness: {}", aether_harness::Model::name(&**m)),
        None => println!("AI harness: off (set ANTHROPIC_API_KEY in .env or the environment)"),
    }
    let stopper = requests.clone();
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
    tokio::select! {
        served = axum::serve(listener, router).into_future() => served.expect("server runs"),
        () = stop_requested() => {}
    }
    let (done, stopped) = oneshot::channel();
    let _ = tokio::task::spawn_blocking(move || stopper.send(Request::Stop(done)).is_ok()).await;
    let _ = stopped.await;
}

async fn stop_requested() {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("SIGTERM can be watched");
    tokio::select! {
        r = tokio::signal::ctrl_c() => r.expect("Ctrl-C can be watched"),
        _ = term.recv() => {}
    }
}
