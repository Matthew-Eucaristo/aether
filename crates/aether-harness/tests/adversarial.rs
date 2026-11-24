use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use aether_harness::{
    Bench, HarnessError, Message, Model, Stream, ToolCall, ToolSpec, Turn, parse_response, realize,
};
use aether_kernel::{Action, Actor, BlobHash, Input, InputOutcome, Kernel, Law};
use aether_world::{CapsuleId, EntityId, PlayerId, Value};
use serde_json::{Value as Json, json};

const HELIX: &str = include_str!("../../../capsules/helix.wat");
const THIEF: &str = include_str!("../../../capsules/thief.wat");
const GREEDY: &str = include_str!("../../../capsules/greedy.wat");
const MINIMAL: &str = r#"(module (func (export "tick")))"#;
const TRAP: &str = r#"(module (func (export "tick") unreachable))"#;

struct KernelBench(Mutex<Kernel>);

impl Bench for KernelBench {
    async fn read<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Kernel) -> T + Send + 'static,
    ) -> Result<T, HarnessError> {
        Ok(f(&self.0.lock().unwrap()))
    }

    async fn upload(&self, bytes: Vec<u8>) -> Result<BlobHash, HarnessError> {
        Ok(self.0.lock().unwrap().add_blob(bytes))
    }

    async fn act(&self, input: Input) -> Result<InputOutcome, HarnessError> {
        Ok(self.0.lock().unwrap().step(vec![input]).inputs[0].clone())
    }
}

struct Scripted {
    turns: Mutex<VecDeque<Turn>>,
    seen: Mutex<Vec<Vec<Message>>>,
}

impl Scripted {
    fn new(turns: Vec<Turn>) -> Scripted {
        Scripted {
            turns: Mutex::new(turns.into()),
            seen: Mutex::new(Vec::new()),
        }
    }

    fn results(&self) -> Vec<(String, bool)> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .filter_map(|msgs| match msgs.last() {
                Some(Message::ToolResults(r)) => Some((r[0].content.clone(), r[0].is_error)),
                _ => None,
            })
            .collect()
    }
}

impl Model for Scripted {
    fn name(&self) -> &str {
        "scripted"
    }

    async fn turn(
        &self,
        _system: &str,
        _tools: &[ToolSpec],
        messages: &[Message],
    ) -> Result<Turn, HarnessError> {
        self.seen.lock().unwrap().push(messages.to_vec());
        Ok(self
            .turns
            .lock()
            .unwrap()
            .pop_front()
            .expect("script has a turn"))
    }
}

fn call(id: &str, name: &str, input: Json) -> Turn {
    Turn {
        assistant: json!({"role": "assistant", "content": [{"type": "tool_use", "id": id, "name": name, "input": input}]}),
        text: String::new(),
        calls: vec![ToolCall {
            id: id.into(),
            name: name.into(),
            input,
        }],
    }
}

fn say(text: &str) -> Turn {
    Turn {
        assistant: json!({"role": "assistant", "content": [{"type": "text", "text": text}]}),
        text: text.into(),
        calls: Vec::new(),
    }
}

fn ana() -> Actor {
    Actor::Player(PlayerId("ana".into()))
}

fn world_with_capsules() -> (Kernel, EntityId, EntityId) {
    let mut k = Kernel::new(Law::default(), 1);
    let wasm = k.add_blob(HELIX.as_bytes().to_vec());
    let install = |actor: Actor| Input {
        actor,
        action: Action::Install {
            wasm,
            grants: vec![],
            budget: Law::default().ceiling,
        },
    };
    let r = k.step(vec![install(Actor::Admin), install(ana())]);
    assert!(matches!(r.inputs[0], InputOutcome::Installed(CapsuleId(0))));
    assert!(matches!(r.inputs[1], InputOutcome::Installed(CapsuleId(1))));
    let mut marker = BTreeMap::new();
    marker.insert("label".to_owned(), Value::Text("region marker".into()));
    let mut own = BTreeMap::new();
    own.insert("target.x".to_owned(), Value::Num(1.0));
    let r = k.step(vec![
        Input {
            actor: Actor::Admin,
            action: Action::Spawn { components: marker },
        },
        Input {
            actor: ana(),
            action: Action::Spawn { components: own },
        },
    ]);
    let region = match r.inputs[0] {
        InputOutcome::Spawned(id) => id,
        ref other => panic!("{other:?}"),
    };
    let own = match r.inputs[1] {
        InputOutcome::Spawned(id) => id,
        ref other => panic!("{other:?}"),
    };
    k.step(vec![]);
    (k, region, own)
}

#[tokio::test]
async fn trials_leave_the_real_world_bit_identical() {
    let (k, region, own) = world_with_capsules();
    let before_hash = k.state_hash();
    let before_tick = k.state().world.tick();
    let before_blobs = k.blobs().iter().count();
    let huge_wat = format!("{MINIMAL}\n;;{}", "x".repeat(2 << 20));
    let mut input_list = vec![
        json!({"tick": 1, "op": "set", "entity": region.0, "name": "stolen", "value": 1}),
        json!({"tick": 2, "op": "set", "entity": own.0, "name": "target.x", "value": 42}),
        json!({"tick": 3, "op": "signal", "entity": region.0, "name": "fire", "value": 1}),
        json!({"tick": 4, "op": "signal", "entity": 999999, "name": "void", "value": 0}),
        json!({"tick": 0, "op": "signal", "entity": own.0, "name": "never", "value": 0}),
        json!({"tick": 500, "op": "signal", "entity": own.0, "name": "never", "value": 0}),
    ];
    if let Ok(v) = serde_json::from_str::<Json>(&format!(
        r#"{{"tick": 3, "op": "signal", "entity": {}, "name": "fire", "value": 1e999}}"#,
        region.0
    )) {
        input_list.push(v);
    }
    let inputs = Json::Array(input_list);
    let faster = HELIX.replace("(f64.const 0.03)", "(f64.const 0.09)");
    let bench = KernelBench(Mutex::new(k));
    let model = Scripted::new(vec![
        call(
            "1",
            "trial",
            json!({"wat": HELIX, "capsule": -1, "grants": [], "ticks": 20}),
        ),
        call(
            "2",
            "trial",
            json!({"wat": THIEF, "capsule": -1, "grants": [], "ticks": 5}),
        ),
        call(
            "3",
            "trial",
            json!({"wat": GREEDY, "capsule": -1, "grants": [], "ticks": 5}),
        ),
        call(
            "4",
            "trial",
            json!({"wat": TRAP, "capsule": -1, "grants": [], "ticks": 3}),
        ),
        call(
            "5",
            "trial",
            json!({"wat": "this is not wat", "capsule": -1, "grants": [], "ticks": 5}),
        ),
        call(
            "6",
            "trial",
            json!({"wat": huge_wat, "capsule": -1, "grants": [], "ticks": 5}),
        ),
        call(
            "7",
            "trial",
            json!({"wat": MINIMAL, "capsule": -1, "grants": [], "ticks": 8, "inputs": inputs}),
        ),
        call(
            "8",
            "trial",
            json!({"wat": faster, "capsule": 1, "grants": [], "ticks": 10}),
        ),
        call(
            "9",
            "trial",
            json!({"wat": MINIMAL, "capsule": -1, "grants": [], "ticks": 0}),
        ),
        call(
            "10",
            "trial",
            json!({"wat": MINIMAL, "capsule": -1, "grants": [], "ticks": 4294967295u32}),
        ),
        say("done"),
    ]);
    realize(&model, &bench, ana(), "probe everything", |_| {})
        .await
        .unwrap();

    let results = model.results();
    assert_eq!(results.len(), 10);
    assert!(!results[0].1, "clean helix trial: {}", results[0].0);
    assert!(
        results[1].1 && results[1].0.contains("denied"),
        "thief faults: {}",
        results[1].0
    );
    assert!(
        results[2].1 && results[2].0.contains("out of fuel"),
        "greedy faults: {}",
        results[2].0
    );
    assert!(
        results[3].1 && results[3].0.contains("trap"),
        "trap faults: {}",
        results[3].0
    );
    assert!(
        results[4].1 && results[4].0.contains("rejected before running"),
        "garbage wat: {}",
        results[4].0
    );
    assert!(
        results[5].1 && results[5].0.contains("rejected before running"),
        "huge wat: {}",
        results[5].0
    );
    assert!(
        !results[6].1 && results[6].0.contains("Rejected input"),
        "foreign inputs are rejected inside the fork: {}",
        results[6].0
    );
    assert!(
        !results[7].1 && results[7].0.contains("Upgraded capsule 1"),
        "upgrade trial: {}",
        results[7].0
    );
    assert!(!results[8].1, "ticks=0 clamps: {}", results[8].0);
    assert!(!results[9].1, "ticks=u32::MAX clamps: {}", results[9].0);

    let k = bench.0.lock().unwrap();
    assert_eq!(
        k.state().world.tick(),
        before_tick,
        "trials must not advance the real world's tick"
    );
    assert_eq!(
        k.state_hash(),
        before_hash,
        "trials must not change the real world's state hash"
    );
    assert_eq!(
        k.blobs().iter().count(),
        before_blobs,
        "trials must not add blobs to the real blob store"
    );
}

#[tokio::test]
async fn install_and_upgrade_promote_only_the_matching_clean_trial() {
    let (k, _, _) = world_with_capsules();
    let before_hash = k.state_hash();
    let faster = HELIX.replace("(f64.const 0.03)", "(f64.const 0.09)");
    let bench = KernelBench(Mutex::new(k));
    let model = Scripted::new(vec![
        call(
            "1",
            "trial",
            json!({"wat": HELIX, "capsule": -1, "grants": [], "ticks": 20}),
        ),
        call(
            "2",
            "trial",
            json!({"wat": THIEF, "capsule": -1, "grants": [], "ticks": 5}),
        ),
        call("3", "install", json!({})),
        call(
            "4",
            "trial",
            json!({"wat": HELIX, "capsule": -1, "grants": [], "ticks": 20}),
        ),
        call("5", "install", json!({})),
        call("6", "upgrade", json!({"capsule": 2})),
        call(
            "7",
            "trial",
            json!({"wat": faster, "capsule": 2, "grants": [], "ticks": 10}),
        ),
        call("8", "upgrade", json!({"capsule": 2})),
        call(
            "9",
            "trial",
            json!({"wat": faster, "capsule": 77, "grants": [], "ticks": 5}),
        ),
        call("10", "install", json!({})),
        say("done"),
    ]);
    let outcome = realize(&model, &bench, ana(), "build helixes", |_| {})
        .await
        .unwrap();
    let results = model.results();
    assert!(!results[0].1, "{}", results[0].0);
    assert!(results[1].1, "thief faults: {}", results[1].0);
    assert!(
        results[2].1 && results[2].0.contains("has not passed cleanly"),
        "install after a failing trial refuses: {}",
        results[2].0
    );
    assert!(!results[3].1, "{}", results[3].0);
    assert!(
        !results[4].1 && results[4].0.contains("capsule 2"),
        "install promotes the clean trial: {}",
        results[4].0
    );
    assert!(
        results[5].1
            && results[5]
                .0
                .contains("nothing has passed a trial for capsule 2"),
        "upgrade without a trial refuses: {}",
        results[5].0
    );
    assert!(
        !results[6].1 && results[6].0.contains("Upgraded capsule 2"),
        "{}",
        results[6].0
    );
    assert!(
        !results[7].1 && results[7].0.contains("now runs the new code"),
        "{}",
        results[7].0
    );
    assert!(
        results[8].1 && results[8].0.contains("rejected before running"),
        "upgrade trial of a nonexistent capsule fails in the fork: {}",
        results[8].0
    );
    assert!(
        !results[9].1 && results[9].0.contains("capsule 3"),
        "a failed trial on another slot must not block install: {}",
        results[9].0
    );
    assert_eq!(outcome.capsules, vec![2, 2, 3]);
    let k = bench.0.lock().unwrap();
    assert_eq!(
        k.state().capsules.len(),
        4,
        "0, 1 preinstalled; 2 and 3 added"
    );
    assert_ne!(
        k.state_hash(),
        before_hash,
        "real installs/upgrades do change the world"
    );
}

#[tokio::test]
async fn foreign_capsules_and_grants_are_refused_without_touching_the_world() {
    let (k, region, _) = world_with_capsules();
    let helix_hash = k.state().capsules[&CapsuleId(0)].wasm;
    let faster = HELIX.replace("(f64.const 0.03)", "(f64.const 0.09)");
    let bench = KernelBench(Mutex::new(k));
    let model = Scripted::new(vec![
        call(
            "1",
            "trial",
            json!({"wat": faster, "capsule": 0, "grants": [], "ticks": 5}),
        ),
        call("2", "upgrade", json!({"capsule": 0})),
        call(
            "3",
            "trial",
            json!({"wat": HELIX, "capsule": -1, "grants": [{"entity": region.0, "write": true}], "ticks": 5}),
        ),
        call("4", "install", json!({})),
        call(
            "5",
            "trial",
            json!({"wat": HELIX, "capsule": -1, "grants": [{"entity": region.0, "write": false}], "ticks": 5}),
        ),
        call("6", "install", json!({})),
        say("done"),
    ]);
    realize(&model, &bench, ana(), "inspect capsule 0", |_| {})
        .await
        .unwrap();
    let results = model.results();
    assert!(
        results[0].1 && results[0].0.contains("may not manage capsule 0"),
        "ana cannot trial an upgrade of admin's capsule: {}",
        results[0].0
    );
    assert!(
        results[1].1 && results[1].0.contains("has not passed cleanly"),
        "{}",
        results[1].0
    );
    assert!(
        results[2].1 && results[2].0.contains("may not grant write access"),
        "write grants on region entities are denied: {}",
        results[2].0
    );
    assert!(
        results[3].1 && results[3].0.contains("has not passed cleanly"),
        "{}",
        results[3].0
    );
    assert!(
        !results[4].1,
        "a read grant on a region entity is fine: {}",
        results[4].0
    );
    assert!(!results[5].1, "{}", results[5].0);
    let k = bench.0.lock().unwrap();
    assert_eq!(
        k.state().capsules[&CapsuleId(0)].wasm,
        helix_hash,
        "capsule 0 still runs its original code"
    );
}

#[tokio::test]
async fn malformed_tool_inputs_return_errors_never_panic() {
    let bench = KernelBench(Mutex::new(Kernel::new(Law::default(), 1)));
    let trial = |extra: Json| {
        let mut base = json!({"wat": MINIMAL, "capsule": -1, "grants": [], "ticks": 5});
        base.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        base
    };
    let model = Scripted::new(vec![
        call("1", "look", json!({})),
        call("2", "look", json!({"key": 5, "owner": ""})),
        call("3", "look", json!({"key": "", "owner": "", "extra": 1})),
        call("4", "look", json!("just a string")),
        call("5", "read_capsule", json!({"capsule": -1})),
        call("6", "read_capsule", json!({"capsule": "zero"})),
        call("7", "read_capsule", json!({"capsule": 999})),
        call(
            "8",
            "trial",
            json!({"capsule": -1, "grants": [], "ticks": 5}),
        ),
        call("9", "trial", trial(json!({"ticks": -5}))),
        call("10", "trial", trial(json!({"ticks": 1e100}))),
        call("11", "trial", trial(json!({"wat": 42}))),
        call("12", "trial", trial(json!({"inputs": {"tick": 1}}))),
        call(
            "13",
            "trial",
            trial(json!({"inputs": [{"tick": 1, "op": "signal", "entity": 0, "name": "x"}]})),
        ),
        call(
            "14",
            "trial",
            trial(
                json!({"inputs": [{"tick": 1, "op": "explode", "entity": 0, "name": "x", "value": 1}]}),
            ),
        ),
        call(
            "15",
            "trial",
            trial(json!({"capsule": 9223372036854775807u64})),
        ),
        call("16", "upgrade", json!({})),
        call("17", "upgrade", json!({"capsule": -3})),
        call("18", "upgrade", json!({"capsule": "zero"})),
        call("19", "nuke_world", json!({})),
        call("20", "install", json!({"capsule": 5, "wat": "ignored"})),
        say("done"),
    ]);
    let outcome = realize(&model, &bench, ana(), "fuzz the tools", |_| {})
        .await
        .unwrap();
    assert_eq!(outcome.reply, "done");
    let results = model.results();
    let expect_error = |i: usize, what: &str| {
        assert!(
            results[i].1 && !results[i].0.is_empty(),
            "{what}: expected a nonempty tool error, got: {}",
            results[i].0
        );
    };
    expect_error(0, "look missing fields");
    expect_error(1, "look wrong types");
    assert!(
        !results[2].1 && results[2].0.contains("matching entities"),
        "serde ignores undeclared fields although the schema forbids them: {}",
        results[2].0
    );
    expect_error(3, "look input not an object");
    expect_error(4, "read_capsule negative");
    expect_error(5, "read_capsule wrong type");
    assert!(
        results[6].1 && results[6].0.contains("does not exist"),
        "{}",
        results[6].0
    );
    expect_error(7, "trial missing wat");
    expect_error(8, "trial negative ticks");
    expect_error(9, "trial ticks out of range");
    expect_error(10, "trial wat not a string");
    expect_error(11, "trial inputs not an array");
    expect_error(12, "trial input missing value");
    expect_error(13, "trial input unknown op");
    assert!(
        results[14].1 && results[14].0.contains("rejected before running"),
        "trial of capsule i64::MAX: {}",
        results[14].0
    );
    expect_error(15, "upgrade missing capsule");
    expect_error(16, "upgrade negative capsule");
    expect_error(17, "upgrade wrong type");
    assert!(
        results[18].1 && results[18].0.contains("there is no tool named nuke_world"),
        "{}",
        results[18].0
    );
    assert!(
        results[19].1 && results[19].0.contains("nothing has passed a trial"),
        "install ignores extra input fields and still requires a trial: {}",
        results[19].0
    );
}

#[tokio::test]
async fn upgrade_capsule_ids_above_i64_max_do_not_wrap_into_the_new_capsule_slot() {
    let bench = KernelBench(Mutex::new(Kernel::new(Law::default(), 1)));
    let model = Scripted::new(vec![
        call(
            "1",
            "trial",
            json!({"wat": HELIX, "capsule": -1, "grants": [], "ticks": 20}),
        ),
        call("2", "upgrade", json!({"capsule": 18446744073709551615u64})),
        say("done"),
    ]);
    realize(&model, &bench, ana(), "wrap", |_| {})
        .await
        .unwrap();
    let results = model.results();
    assert!(
        results[1].1
            && results[1]
                .0
                .contains("capsule 18446744073709551615 does not exist")
            && !results[1].0.contains("the world rejected"),
        "capsule u64::MAX must be refused without consulting the new-capsule trial or the world: {}",
        results[1].0
    );
    let k = bench.0.lock().unwrap();
    assert!(k.state().capsules.is_empty());
}

#[tokio::test]
async fn the_loop_stops_after_max_turns() {
    let bench = KernelBench(Mutex::new(Kernel::new(Law::default(), 1)));
    let turns: Vec<Turn> = (0..24)
        .map(|i| call(&i.to_string(), "look", json!({"key": "", "owner": ""})))
        .collect();
    let model = Scripted::new(turns);
    let err = realize(&model, &bench, ana(), "never finish", |_| {})
        .await
        .unwrap_err();
    assert!(matches!(err, HarnessError::TooManyTurns), "{err:?}");
}

const SSE: &str = concat!(
    "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"m\",\"role\":\"assistant\",\"content\":[],\"stop_reason\":null}}\n\n",
    "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Testing \"}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"it.\"}}\n\n",
    "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
    "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"trial\",\"input\":{}}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"ticks\\\": \"}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"30}\"}}\n\n",
    "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
    "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":9}}\n\n",
    "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
);

fn assemble(chunks: &[&str]) -> Result<Turn, HarnessError> {
    let mut stream = Stream::default();
    for chunk in chunks {
        stream.push(chunk)?;
    }
    parse_response(&stream.finish()?)
}

#[test]
fn lf_stream_reassembled_byte_by_byte() {
    let whole = assemble(&[SSE]).unwrap();
    let bytes: Vec<&str> = SSE
        .as_bytes()
        .iter()
        .map(|b| std::str::from_utf8(std::slice::from_ref(b)).unwrap())
        .collect();
    let drip = assemble(&bytes).unwrap();
    assert_eq!(whole, drip);
    assert_eq!(drip.text, "Testing it.");
    assert_eq!(drip.calls[0].input["ticks"], 30);
}

#[test]
fn stream_reassembled_at_every_two_chunk_split() {
    for sse in &[SSE.to_owned(), SSE.replace('\n', "\r\n")] {
        let want = assemble(&[sse]).unwrap();
        for i in 1..sse.len() {
            let got = assemble(&[&sse[..i], &sse[i..]])
                .unwrap_or_else(|e| panic!("split at byte {i}: {e:?}"));
            assert_eq!(want, got, "split at byte {i}");
        }
    }
}

#[test]
fn crlf_stream_reassembled_byte_by_byte() {
    let crlf = SSE.replace('\n', "\r\n");
    let bytes: Vec<&str> = crlf
        .as_bytes()
        .iter()
        .map(|b| std::str::from_utf8(std::slice::from_ref(b)).unwrap())
        .collect();
    let turn = assemble(&bytes).unwrap();
    assert_eq!(turn.text, "Testing it.");
    assert!(
        !turn.calls.is_empty() && turn.calls[0].input["ticks"] == 30,
        "the tool call must survive CRLF chunking: {:?}",
        turn.calls
    );
}

#[test]
fn crlf_separator_split_across_two_chunks_is_one_boundary() {
    let first = "data: {\"type\":\"message_start\",\"message\":{\"content\":[]}}\r\n\r";
    let second = concat!(
        "\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\r\n\r\n",
        "data: {\"type\":\"message_stop\"}\r\n\r\n"
    );
    let mut stream = Stream::default();
    stream.push(first).unwrap();
    stream.push(second).unwrap();
    let message = stream.finish().unwrap();
    assert_eq!(
        message["stop_reason"], "end_turn",
        "the split CRLF must still terminate the first event; got {message:?}"
    );
}

#[test]
fn stream_cut_before_message_stop_is_not_a_complete_turn() {
    let cut = SSE.replace(
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        "",
    );
    let result = assemble(&[&cut]);
    assert!(
        result.is_err(),
        "a stream without message_stop must not parse as a complete turn: {result:?}"
    );
}

#[test]
fn tool_input_fully_received_but_block_stop_cut_is_not_dropped() {
    let cut = SSE.replace(
        concat!(
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":9}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        ),
        "",
    );
    assert!(
        assemble(&[&cut]).is_err(),
        "a stream cut before content_block_stop must be rejected, not assembled with an empty input"
    );
}

#[test]
fn delta_before_block_start_is_an_error_not_a_panic() {
    let mut stream = Stream::default();
    let result = stream.push(concat!(
        "data: {\"type\":\"message_start\",\"message\":{\"content\":[]}}\n\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n",
    ));
    assert!(
        result.is_err(),
        "a delta for a block that never started must be a stream error, not silence or a panic"
    );
}

#[test]
fn block_stop_before_block_start_is_an_error_not_a_panic() {
    let mut stream = Stream::default();
    let result = stream.push(concat!(
        "data: {\"type\":\"message_start\",\"message\":{\"content\":[]}}\n\n",
        "data: {\"type\":\"content_block_stop\",\"index\":3}\n\n",
    ));
    assert!(
        result.is_err(),
        "a stop for a block that never started must be a stream error, not silence or a panic"
    );
}

#[test]
fn interleaved_blocks_and_split_escapes_assemble() {
    let event = |v: Json| format!("data: {}\n\n", serde_json::to_string(&v).unwrap());
    let p1 = "{\"key\": \"a\\u004";
    let p2 = "1b\", \"owner\": \"\"}";
    let sse = [
        event(json!({"type":"message_start","message":{"content":[]}})),
        event(json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}})),
        event(json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"t1","name":"look","input":{}}})),
        event(json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":p1}})),
        event(json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"one "}})),
        event(json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":p2}})),
        event(json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"two"}})),
        event(json!({"type":"content_block_stop","index":1})),
        event(json!({"type":"content_block_stop","index":0})),
        event(json!({"type":"message_delta","delta":{"stop_reason":"tool_use"}})),
        event(json!({"type":"message_stop"})),
    ]
    .concat();
    let bytes: Vec<&str> = sse
        .as_bytes()
        .iter()
        .map(|b| std::str::from_utf8(std::slice::from_ref(b)).unwrap())
        .collect();
    let turn = assemble(&bytes).unwrap();
    assert_eq!(turn.text, "one two");
    assert_eq!(turn.calls[0].input["key"], "aAb", "split mid-escape");
    assert_eq!(turn.calls[0].input["owner"], "");
}

#[test]
fn unknown_events_are_ignored_and_error_events_fail() {
    let sse = concat!(
        "data: {\"type\":\"message_start\",\"message\":{\"content\":[]}}\n\n",
        "data: {\"type\":\"ping\"}\n\n",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"ok\"}}\n\n",
        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n",
        "data: {\"type\":\"message_stop\"}\n\n",
    );
    let turn = assemble(&[sse]).unwrap();
    assert_eq!(turn.text, "ok");

    let mut stream = Stream::default();
    let err = stream
        .push("data: {\"type\":\"error\",\"error\":{\"message\":\"overloaded\"}}\n\n")
        .unwrap_err();
    assert!(
        matches!(err, HarnessError::Api(0, ref m) if m == "overloaded"),
        "{err:?}"
    );
}

#[test]
fn content_blocks_must_start_in_order() {
    let mut s = Stream::default();
    s.push(
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"content\":[]}}\n\n",
    )
    .unwrap();
    let far = s.push(
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":10000000,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
    );
    assert!(far.is_err(), "{far:?}");
}
