use std::collections::VecDeque;
use std::sync::Mutex;

use aether_harness::{
    Bench, Event, HarnessError, Message, Model, Stream, ToolCall, ToolSpec, Turn, parse_response,
    realize, request_body,
};
use aether_kernel::{Actor, BlobHash, Input, InputOutcome, Kernel, Law};
use aether_world::{Owner, PlayerId, Value};
use serde_json::{Value as Json, json};

const HELIX: &str = include_str!("../../../capsules/helix.wat");
const THIEF: &str = include_str!("../../../capsules/thief.wat");
const RIFLE: &str = include_str!("../../../capsules/rifle.wat");

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
}

impl Model for Scripted {
    fn name(&self) -> &str {
        "scripted"
    }

    async fn turn(
        &self,
        system: &str,
        tools: &[ToolSpec],
        messages: &[Message],
    ) -> Result<Turn, HarnessError> {
        assert!(system.contains("find"), "the guide documents the ABI");
        assert_eq!(tools.len(), 5);
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

#[tokio::test]
async fn untested_or_failing_code_never_reaches_the_real_world() {
    let bench = KernelBench(Mutex::new(Kernel::new(Law::default(), 1)));
    let ana = Actor::Player(PlayerId("ana".into()));
    let model = Scripted::new(vec![
        call(
            "1",
            "trial",
            json!({"wat": THIEF, "capsule": -1, "grants": [], "ticks": 5}),
        ),
        call("2", "install", json!({})),
        call("3", "upgrade", json!({"capsule": 0})),
        call(
            "4",
            "trial",
            json!({"wat": HELIX, "capsule": -1, "grants": [], "ticks": 20}),
        ),
        call("5", "install", json!({})),
        call("6", "look", json!({"key": "pos.z", "owner": ""})),
        say("Done: capsule 0 spins a helix."),
    ]);
    let mut events = Vec::new();
    let outcome = realize(&model, &bench, ana.clone(), "make a helix", |e| {
        events.push(e)
    })
    .await
    .unwrap();

    let seen = model.seen.lock().unwrap();
    let results: Vec<(String, bool)> = seen
        .iter()
        .filter_map(|msgs| match msgs.last() {
            Some(Message::ToolResults(r)) => Some((r[0].content.clone(), r[0].is_error)),
            _ => None,
        })
        .collect();
    drop(seen);
    assert!(
        results[0].1 && results[0].0.contains("denied"),
        "{}",
        results[0].0
    );
    assert!(
        results[1].1 && results[1].0.contains("has not passed cleanly"),
        "{}",
        results[1].0
    );
    assert!(
        results[2].1
            && results[2]
                .0
                .contains("nothing has passed a trial for capsule 0"),
        "{}",
        results[2].0
    );
    assert!(
        !results[3].1 && results[3].0.contains("moved"),
        "{}",
        results[3].0
    );
    assert!(
        !results[4].1 && results[4].0.contains("capsule 0"),
        "{}",
        results[4].0
    );
    assert!(results[5].0.contains("8 matching"), "{}", results[5].0);

    assert_eq!(outcome.capsules, vec![0]);
    assert_eq!(outcome.turns, 7);
    let k = bench.0.lock().unwrap();
    assert_eq!(k.state().capsules.len(), 1);
    assert_eq!(k.state().capsules.values().next().unwrap().installer, ana);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ToolDone { name, ok: true, .. } if name == "install"))
    );

    let first = &model.seen.lock().unwrap()[0];
    match &first[0] {
        Message::User(text) => {
            assert!(text.contains("You act for player:ana"));
            assert!(text.contains("make a helix"));
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn upgrades_are_trialled_against_the_capsule_they_replace() {
    let mut k = Kernel::new(Law::default(), 1);
    let wasm = k.add_blob(HELIX.as_bytes().to_vec());
    k.step(vec![Input {
        actor: Actor::Admin,
        action: aether_kernel::Action::Install {
            wasm,
            grants: vec![],
            budget: Law::default().ceiling,
        },
    }]);
    let bench = KernelBench(Mutex::new(k));
    let faster = HELIX.replace("(f64.const 0.03)", "(f64.const 0.09)");
    let model = Scripted::new(vec![
        call("1", "read_capsule", json!({"capsule": 0})),
        call("2", "upgrade", json!({"capsule": 0})),
        call(
            "3",
            "trial",
            json!({"wat": faster, "capsule": 0, "grants": [], "ticks": 10}),
        ),
        call("4", "upgrade", json!({"capsule": 0})),
        say("Faster now."),
    ]);
    realize(&model, &bench, Actor::Admin, "make it climb faster", |_| {})
        .await
        .unwrap();
    let seen = model.seen.lock().unwrap();
    let result = |i: usize| match seen[i].last() {
        Some(Message::ToolResults(r)) => (r[0].content.clone(), r[0].is_error),
        _ => panic!(),
    };
    assert!(result(1).0.contains("handle 8: entity"), "{}", result(1).0);
    assert!(result(2).1);
    assert!(
        !result(3).1 && result(3).0.contains("Upgraded capsule 0"),
        "{}",
        result(3).0
    );
    assert!(!result(4).1, "{}", result(4).0);
    drop(seen);
    let k = bench.0.lock().unwrap();
    let z_step = k
        .world()
        .entities()
        .filter(|(_, e)| e.owner == Owner::Capsule(aether_world::CapsuleId(0)))
        .filter_map(|(_, e)| match e.components.get("pos.z") {
            Some(Value::Num(z)) => Some(*z),
            _ => None,
        })
        .count();
    assert_eq!(z_step, 8);
}

#[test]
fn requests_follow_the_messages_api() {
    let tools = vec![ToolSpec {
        name: "look",
        description: "d",
        schema: json!({"type": "object"}),
    }];
    let messages = vec![
        Message::User("hi".into()),
        Message::Assistant(
            json!({"role": "assistant", "content": [{"type": "thinking", "thinking": "", "signature": "s"}, {"type": "tool_use", "id": "t1", "name": "look", "input": {}}]}),
        ),
        Message::ToolResults(vec![aether_harness::ToolResult {
            id: "t1".into(),
            content: "ok".into(),
            is_error: false,
        }]),
    ];
    let haiku = request_body("claude-haiku-5-5", "medium", "guide", &tools, &messages);
    assert_eq!(haiku["model"], "claude-haiku-5-5");
    assert_eq!(haiku["output_config"]["effort"], "medium");
    assert!(
        haiku.get("fallbacks").is_none(),
        "Haiku has no server-side fallback"
    );
    assert_eq!(haiku["tools"][0]["strict"], true);
    assert_eq!(haiku["stream"], true);
    assert_eq!(haiku["max_tokens"], 64000);
    assert_eq!(haiku["tools"][0]["eager_input_streaming"], true);
    assert_eq!(haiku["system"][0]["cache_control"]["type"], "ephemeral");
    assert_eq!(
        haiku["messages"][1]["content"][0]["type"], "thinking",
        "thinking is passed back unchanged"
    );
    assert_eq!(haiku["messages"][2]["content"][0]["type"], "tool_result");
    assert_eq!(haiku["messages"][2]["content"][0]["tool_use_id"], "t1");
    let opus = request_body("claude-opus-5-5", "high", "guide", &tools, &messages);
    assert_eq!(opus["fallbacks"], "default");
}

#[test]
fn responses_are_parsed_with_their_stop_reasons() {
    let ok = json!({
        "stop_reason": "tool_use",
        "content": [
            {"type": "thinking", "thinking": "", "signature": "x"},
            {"type": "text", "text": "Testing it."},
            {"type": "tool_use", "id": "toolu_1", "name": "trial", "input": {"ticks": 3}}
        ]
    });
    let turn = parse_response(&ok).unwrap();
    assert_eq!(turn.text, "Testing it.");
    assert_eq!(turn.calls[0].name, "trial");
    assert_eq!(turn.calls[0].input["ticks"], 3);
    assert_eq!(turn.assistant["content"][0]["type"], "thinking");

    let refused =
        json!({"stop_reason": "refusal", "stop_details": {"category": "cyber"}, "content": []});
    assert!(matches!(parse_response(&refused), Err(HarnessError::Refused(c)) if c == "cyber"));
    let cut = json!({"stop_reason": "max_tokens", "content": []});
    assert!(matches!(parse_response(&cut), Err(HarnessError::Truncated)));
}

#[tokio::test]
async fn trials_can_pull_the_trigger() {
    let mut k = Kernel::new(Law::default(), 5);
    let realm = k.add_blob(include_bytes!("../../../capsules/realm.wat").to_vec());
    k.step(vec![Input {
        actor: Actor::Admin,
        action: aether_kernel::Action::Install {
            wasm: realm,
            grants: vec![],
            budget: Law::default().ceiling,
        },
    }]);
    let hub = k.state().capsules.values().next().unwrap().self_entity;
    let join = |name: &str| Input {
        actor: Actor::Player(PlayerId(name.into())),
        action: aether_kernel::Action::Signal {
            entity: hub,
            name: "join".into(),
            value: 0.0,
        },
    };
    k.step(vec![join("ana"), join("budi")]);
    let body = |k: &Kernel, name: &str| {
        let label = Value::Text(format!("player:{name}"));
        k.world()
            .entities()
            .find(|(_, e)| e.components.get("player") == Some(&label))
            .map(|(id, _)| id)
            .unwrap()
    };
    let (ana, budi) = (body(&k, "ana"), body(&k, "budi"));
    let place = |entity, key: &str, value: f64| Input {
        actor: Actor::Admin,
        action: aether_kernel::Action::Set {
            entity,
            key: key.into(),
            value: Value::Num(value),
        },
    };
    k.step(vec![
        place(ana, "pos.x", 0.0),
        place(ana, "pos.y", 0.0),
        place(budi, "pos.x", 6.0),
        place(budi, "pos.y", 0.0),
    ]);
    k.step(vec![]);
    let ana = ana.0;
    let mut inputs =
        vec![json!({"tick": 1, "op": "signal", "entity": ana, "name": "weapon", "value": 1})];
    for t in 2..12 {
        inputs.push(json!({"tick": t, "op": "signal", "entity": ana, "name": "fire", "value": 1}));
    }
    let bench = KernelBench(Mutex::new(k));
    let model = Scripted::new(vec![
        call(
            "1",
            "trial",
            json!({"wat": RIFLE, "capsule": -1, "grants": [{"entity": ana, "write": false}], "ticks": 20, "inputs": inputs}),
        ),
        say("done"),
    ]);
    realize(
        &model,
        &bench,
        Actor::Player(PlayerId("ana".into())),
        "rifle",
        |_| {},
    )
    .await
    .unwrap();
    let seen = model.seen.lock().unwrap();
    let Some(Message::ToolResults(r)) = seen[1].last() else {
        panic!()
    };
    let report = &r[0].content;
    assert!(!r[0].is_error, "{report}");
    assert!(report.contains("received: hit x"), "{report}");
    assert!(report.contains("health="), "{report}");
}

#[test]
fn streamed_turns_are_reassembled() {
    let sse = concat!(
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"m\",\"role\":\"assistant\",\"content\":[],\"stop_reason\":null}}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\",\"signature\":\"\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"plan\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"sig\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"Testing \"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"it.\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":2,\"content_block\":{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"trial\",\"input\":{}}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"ticks\\\": \"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"30}\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":2}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":9}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    );
    let mut stream = Stream::default();
    for piece in sse.as_bytes().chunks(7) {
        stream.push(std::str::from_utf8(piece).unwrap()).unwrap();
    }
    let turn = parse_response(&stream.finish().unwrap()).unwrap();
    assert_eq!(turn.text, "Testing it.");
    assert_eq!(turn.calls[0].name, "trial");
    assert_eq!(turn.calls[0].input["ticks"], 30);
    assert_eq!(turn.assistant["content"][0]["thinking"], "plan");
    assert_eq!(turn.assistant["content"][0]["signature"], "sig");

    let mut cut = Stream::default();
    cut.push(&sse.replace("\"tool_use\"},\"usage", "\"max_tokens\"},\"usage"))
        .unwrap();
    assert!(matches!(
        parse_response(&cut.finish().unwrap()),
        Err(HarnessError::Truncated)
    ));
}

#[tokio::test]
async fn trials_warn_about_parts_without_a_shape() {
    let blob = r#"(module
  (import "world" "spawn" (func $spawn (result i32)))
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (import "world" "set_bool" (func $set_bool (param i32 i32 i32 i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "scale.x")
  (data (i32.const 8) "solid")
  (func (export "tick")
    (local $h i32)
    (local.set $h (call $spawn))
    (call $set (local.get $h) (i32.const 0) (i32.const 7) (f64.const 4))
    (call $set_bool (local.get $h) (i32.const 8) (i32.const 5) (i32.const 1))))"#;
    let bench = KernelBench(Mutex::new(Kernel::new(Law::default(), 5)));
    let model = Scripted::new(vec![
        call(
            "1",
            "trial",
            json!({"wat": blob, "capsule": -1, "grants": [], "ticks": 1}),
        ),
        say("done"),
    ]);
    realize(
        &model,
        &bench,
        Actor::Player(PlayerId("ana".into())),
        "wall",
        |_| {},
    )
    .await
    .unwrap();
    let seen = model.seen.lock().unwrap();
    let Some(Message::ToolResults(r)) = seen[1].last() else {
        panic!()
    };
    let report = &r[0].content;
    assert!(!r[0].is_error, "{report}");
    assert!(
        report.contains("Warning: 1 entities set scale or solid but no shape"),
        "{report}"
    );
}
