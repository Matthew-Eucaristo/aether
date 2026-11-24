mod claude;
mod guide;
mod model;
mod tools;

use std::collections::BTreeMap;
use std::future::Future;

use aether_kernel::{Action, Actor, BlobHash, Input, InputOutcome, Kernel};
use aether_world::CapsuleId;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub use claude::{Claude, Stream, parse_response, request_body};
pub use guide::GUIDE;
pub use model::{Message, Model, ToolCall, ToolResult, ToolSpec, Turn};

const MAX_TURNS: usize = 24;

#[derive(Debug, thiserror::Error)]
pub enum HarnessError {
    #[error("could not reach the model: {0}")]
    Network(String),
    #[error("the model API answered {0}: {1}")]
    Api(u16, String),
    #[error("the model declined the request ({0})")]
    Refused(String),
    #[error("the model's answer was cut off at its token limit")]
    Truncated,
    #[error("the model did not finish within {MAX_TURNS} turns")]
    TooManyTurns,
    #[error("the world is unavailable: {0}")]
    World(String),
}

pub trait Bench: Send + Sync {
    fn read<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Kernel) -> T + Send + 'static,
    ) -> impl Future<Output = Result<T, HarnessError>> + Send;

    fn upload(&self, bytes: Vec<u8>)
    -> impl Future<Output = Result<BlobHash, HarnessError>> + Send;

    fn act(&self, input: Input) -> impl Future<Output = Result<InputOutcome, HarnessError>> + Send;
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Turn {
        turn: usize,
        model: String,
    },
    Said {
        text: String,
    },
    Tool {
        name: String,
        summary: String,
    },
    ToolDone {
        name: String,
        ok: bool,
        detail: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Outcome {
    pub reply: String,
    pub capsules: Vec<u64>,
    pub turns: usize,
}

fn parse<T: DeserializeOwned>(input: &Value) -> Result<T, String> {
    serde_json::from_value(input.clone()).map_err(|e| format!("invalid tool input: {e}"))
}

fn summary(call: &ToolCall) -> String {
    let i = &call.input;
    match call.name.as_str() {
        "look" => format!("key {:?}, owner {:?}", i["key"], i["owner"]),
        "read_capsule" => format!("capsule {}", i["capsule"]),
        "trial" => format!(
            "{} lines of code, {} ticks, {}",
            i["wat"].as_str().map_or(0, |w| w.lines().count()),
            i["ticks"],
            if i["capsule"].as_i64().unwrap_or(-1) < 0 {
                "new capsule".to_owned()
            } else {
                format!("upgrade of capsule {}", i["capsule"])
            }
        ),
        "install" => "new capsule".into(),
        "upgrade" => format!("capsule {}", i["capsule"]),
        _ => String::new(),
    }
}

struct Tried {
    wat: String,
    grants: Vec<aether_kernel::Grant>,
    clean: bool,
}

struct Session<'a, B: Bench> {
    bench: &'a B,
    actor: Actor,
    trials: BTreeMap<i64, Tried>,
    created: Vec<u64>,
}

impl<B: Bench> Session<'_, B> {
    async fn call(&mut self, call: &ToolCall) -> Result<Result<String, String>, HarnessError> {
        let actor = self.actor.clone();
        Ok(match call.name.as_str() {
            "look" => match parse::<tools::LookIn>(&call.input) {
                Ok(input) => Ok(self.bench.read(move |k| tools::look(k, &input)).await?),
                Err(e) => Err(e),
            },
            "read_capsule" => match parse::<tools::ReadIn>(&call.input) {
                Ok(input) => {
                    self.bench
                        .read(move |k| tools::read_capsule(k, CapsuleId(input.capsule)))
                        .await?
                }
                Err(e) => Err(e),
            },
            "trial" => match parse::<tools::TrialIn>(&call.input) {
                Ok(input) => {
                    let key = input.capsule.max(-1);
                    let wat = input.wat.clone();
                    let grants = tools::grants(&input.grants);
                    let trial = self
                        .bench
                        .read(move |k| tools::trial(k, &actor, &input))
                        .await?;
                    self.trials.insert(
                        key,
                        Tried {
                            wat,
                            grants,
                            clean: trial.clean,
                        },
                    );
                    if trial.clean {
                        Ok(trial.report)
                    } else {
                        Err(trial.report)
                    }
                }
                Err(e) => Err(e),
            },
            "install" => self.install().await?,
            "upgrade" => match parse::<tools::UpgradeIn>(&call.input) {
                Ok(input) => self.upgrade(input).await?,
                Err(e) => Err(e),
            },
            other => Err(format!("there is no tool named {other}")),
        })
    }

    fn tried(&self, key: i64) -> Result<&Tried, String> {
        let what = if key < 0 {
            "a new capsule".to_owned()
        } else {
            format!("capsule {key}")
        };
        match self.trials.get(&key) {
            None => Err(format!(
                "nothing has passed a trial for {what} yet: run trial first"
            )),
            Some(t) if !t.clean => Err(format!(
                "your most recent trial for {what} has not passed cleanly: fix it and run trial again"
            )),
            Some(t) => Ok(t),
        }
    }

    async fn install(&mut self) -> Result<Result<String, String>, HarnessError> {
        let (wat, grants) = match self.tried(-1) {
            Ok(t) => (t.wat.clone(), t.grants.clone()),
            Err(e) => return Ok(Err(e)),
        };
        let wasm = self.bench.upload(wat.into_bytes()).await?;
        let budget = self.bench.read(|k| k.state().law.ceiling).await?;
        let outcome = self
            .bench
            .act(Input {
                actor: self.actor.clone(),
                action: Action::Install {
                    wasm,
                    grants,
                    budget,
                },
            })
            .await?;
        Ok(match outcome {
            InputOutcome::Installed(id) => {
                self.created.push(id.0);
                Ok(format!("installed in the real world as capsule {}", id.0))
            }
            InputOutcome::Rejected(r) => Err(format!("the world rejected the install: {r}")),
            other => Err(format!("unexpected outcome {other:?}")),
        })
    }

    async fn upgrade(
        &mut self,
        input: tools::UpgradeIn,
    ) -> Result<Result<String, String>, HarnessError> {
        let wat = match self.tried(input.capsule as i64) {
            Ok(t) => t.wat.clone(),
            Err(e) => return Ok(Err(e)),
        };
        let wasm = self.bench.upload(wat.into_bytes()).await?;
        let outcome = self
            .bench
            .act(Input {
                actor: self.actor.clone(),
                action: Action::Upgrade {
                    capsule: CapsuleId(input.capsule),
                    wasm,
                },
            })
            .await?;
        Ok(match outcome {
            InputOutcome::Applied => {
                self.created.push(input.capsule);
                Ok(format!("capsule {} now runs the new code", input.capsule))
            }
            InputOutcome::Rejected(r) => Err(format!("the world rejected the upgrade: {r}")),
            other => Err(format!("unexpected outcome {other:?}")),
        })
    }
}

pub async fn realize<M: Model, B: Bench>(
    model: &M,
    bench: &B,
    actor: Actor,
    intent: &str,
    mut on: impl FnMut(Event) + Send,
) -> Result<Outcome, HarnessError> {
    let context = {
        let actor = actor.clone();
        bench.read(move |k| tools::context(k, &actor)).await?
    };
    let specs = tools::specs();
    let mut messages = vec![Message::User(format!("{context}\n# Intent\n{intent}"))];
    let mut session = Session {
        bench,
        actor,
        trials: BTreeMap::new(),
        created: Vec::new(),
    };
    for turn in 1..=MAX_TURNS {
        on(Event::Turn {
            turn,
            model: model.name().to_owned(),
        });
        let t = model.turn(GUIDE, &specs, &messages).await?;
        messages.push(Message::Assistant(t.assistant));
        if t.calls.is_empty() {
            return Ok(Outcome {
                reply: t.text,
                capsules: session.created,
                turns: turn,
            });
        }
        if !t.text.trim().is_empty() {
            on(Event::Said { text: t.text });
        }
        let mut results = Vec::new();
        for call in &t.calls {
            on(Event::Tool {
                name: call.name.clone(),
                summary: summary(call),
            });
            let result = session.call(call).await?;
            let (content, is_error) = match result {
                Ok(c) => (c, false),
                Err(c) => (c, true),
            };
            on(Event::ToolDone {
                name: call.name.clone(),
                ok: !is_error,
                detail: content.clone(),
            });
            results.push(ToolResult {
                id: call.id.clone(),
                content,
                is_error,
            });
        }
        messages.push(Message::ToolResults(results));
    }
    Err(HarnessError::TooManyTurns)
}
