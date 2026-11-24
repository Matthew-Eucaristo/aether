use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aether_kernel::{BlobHash, Blobs, Input, Kernel, KernelError, Law, RULES, State, TickReport};
use serde::{Deserialize, Serialize};

const SNAPSHOT: &str = "snapshot.json";
const JOURNAL: &str = "journal.log";
const BLOBS: &str = "blobs";
const TMP: &str = "tmp";
const LOCK: &str = "LOCK";
const HEADER_PREFIX: &str = "aether-journal rules=";

fn header() -> Vec<u8> {
    format!("{HEADER_PREFIX}{RULES}\n").into_bytes()
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error(transparent)]
    Kernel(#[from] KernelError),
    #[error("{path}: {reason}")]
    Corrupt { path: PathBuf, reason: String },
    #[error("tick {tick}: the journal expects prior state {expected} but replay produced {actual}")]
    Diverged {
        tick: u64,
        expected: BlobHash,
        actual: BlobHash,
    },
    #[error(
        "the journal was written under {journal} but this kernel follows {kernel}; \
         open it with the matching version and checkpoint before upgrading"
    )]
    RulesChanged {
        journal: String,
        kernel: &'static str,
    },
    #[error("{0} is already open in another process")]
    Locked(PathBuf),
    #[error("{0} already holds a world")]
    Exists(PathBuf),
    #[error("an earlier journal write failed; reopen the store to recover")]
    Poisoned,
}

trait At<T> {
    fn at(self, path: &Path) -> Result<T, StoreError>;
}

impl<T> At<T> for io::Result<T> {
    fn at(self, path: &Path) -> Result<T, StoreError> {
        self.map_err(|source| StoreError::Io {
            path: path.to_owned(),
            source,
        })
    }
}

fn corrupt(path: &Path, reason: impl ToString) -> StoreError {
    StoreError::Corrupt {
        path: path.to_owned(),
        reason: reason.to_string(),
    }
}

#[derive(Serialize, Deserialize)]
struct Record {
    tick: u64,
    prior: BlobHash,
    inputs: Vec<Input>,
}

fn encode(record: &Record) -> Vec<u8> {
    let json = serde_json::to_vec(record).expect("journal records serialize");
    let mut line = BlobHash::of(&json).to_hex().into_bytes();
    line.push(b' ');
    line.extend_from_slice(&json);
    line.push(b'\n');
    line
}

fn decode(line: &[u8]) -> Result<Record, String> {
    let (sum, json) = line
        .split_at_checked(64)
        .and_then(|(sum, rest)| Some((sum, rest.strip_prefix(b" ")?)))
        .ok_or("record has no checksum")?;
    let sum = std::str::from_utf8(sum)
        .ok()
        .and_then(BlobHash::from_hex)
        .ok_or("record checksum is not hex")?;
    if BlobHash::of(json) != sum {
        return Err("record checksum does not match its contents".into());
    }
    serde_json::from_slice(json).map_err(|e| e.to_string())
}

fn lock(dir: &Path) -> Result<File, StoreError> {
    let path = dir.join(LOCK);
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .at(&path)?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(std::fs::TryLockError::WouldBlock) => Err(StoreError::Locked(dir.to_owned())),
        Err(std::fs::TryLockError::Error(e)) => Err(e).at(&path),
    }
}

fn sync_dir(dir: &Path) -> Result<(), StoreError> {
    File::open(dir).and_then(|d| d.sync_all()).at(dir)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let tmp = path.with_extension(TMP);
    let mut file = File::create(&tmp).at(&tmp)?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .at(&tmp)?;
    fs::rename(&tmp, path).at(path)?;
    sync_dir(path.parent().expect("store paths have a parent"))
}

fn load_blobs(dir: &Path) -> Result<Blobs, StoreError> {
    let mut blobs = Blobs::new();
    for entry in fs::read_dir(dir).at(dir)? {
        let path = entry.at(dir)?.path();
        if path.extension().is_some_and(|e| e == TMP) {
            fs::remove_file(&path).at(&path)?;
            continue;
        }
        let hash = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(BlobHash::from_hex)
            .ok_or_else(|| corrupt(&path, "blob file name is not a blake3 hash"))?;
        let bytes = fs::read(&path).at(&path)?;
        if blobs.insert(bytes) != hash {
            return Err(corrupt(&path, "blob contents do not match its name"));
        }
    }
    Ok(blobs)
}

fn rules_number(rules: &str) -> Option<u32> {
    rules.strip_prefix("aether-rules/")?.parse().ok()
}

fn upgrade_snapshot(json: &mut serde_json::Value, from: &str) -> Result<(), String> {
    let number = rules_number(from).ok_or_else(|| format!("unknown rules {from:?}"))?;
    let current = rules_number(RULES).expect("the kernel's rules are numbered");
    if number > current {
        return Err(format!(
            "rules {from:?} are newer than this kernel's {RULES:?}"
        ));
    }
    let state = json
        .as_object_mut()
        .ok_or("the snapshot is not an object")?;
    state
        .entry("signals")
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    let law = state
        .get_mut("law")
        .and_then(|l| l.as_object_mut())
        .ok_or("the snapshot has no law")?;
    law.entry("region_fuel_per_tick")
        .or_insert_with(|| Law::default().region_fuel_per_tick.into());
    law.entry("max_bodies")
        .or_insert_with(|| Law::default().max_bodies.into());
    law.entry("terrain").or_insert_with(|| {
        serde_json::to_value(Law::default().terrain).expect("terrain serializes")
    });
    Ok(())
}

fn holds_a_world(dir: &Path) -> bool {
    if dir.join(SNAPSHOT).exists() {
        return true;
    }
    if let Ok(bytes) = fs::read(dir.join(JOURNAL))
        && bytes.iter().filter(|b| **b == b'\n').count() > 1
    {
        return true;
    }
    fs::read_dir(dir.join(BLOBS))
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false)
}

pub struct Store {
    _lock: File,
    dir: PathBuf,
    kernel: Kernel,
    journal: File,
    head: BlobHash,
    poisoned: bool,
}

impl Store {
    pub fn create(dir: impl Into<PathBuf>, law: Law, seed: u64) -> Result<Store, StoreError> {
        let dir = dir.into();
        let snapshot = dir.join(SNAPSHOT);
        if holds_a_world(&dir) {
            return Err(StoreError::Exists(dir));
        }
        let blobs = dir.join(BLOBS);
        fs::create_dir_all(&blobs).at(&blobs)?;
        sync_dir(&dir)?;
        let held = lock(&dir)?;
        if holds_a_world(&dir) {
            return Err(StoreError::Exists(dir));
        }
        write_atomic(&dir.join(JOURNAL), &header())?;
        let kernel = Kernel::new(law, seed);
        write_atomic(&snapshot, &encode_state(kernel.state()))?;
        Store::open_locked(dir, held)
    }

    pub fn open(dir: impl Into<PathBuf>) -> Result<Store, StoreError> {
        let dir = dir.into();
        let held = lock(&dir)?;
        Store::open_locked(dir, held)
    }

    fn open_locked(dir: PathBuf, held: File) -> Result<Store, StoreError> {
        let blobs = load_blobs(&dir.join(BLOBS))?;
        let path = dir.join(JOURNAL);
        let bytes = fs::read(&path).at(&path)?;
        let lines: Vec<&[u8]> = bytes.split_inclusive(|b| *b == b'\n').collect();
        let rules = match lines.first().and_then(|l| l.strip_suffix(b"\n")) {
            None => None,
            Some(line) => Some(
                std::str::from_utf8(line)
                    .ok()
                    .and_then(|l| l.strip_prefix(HEADER_PREFIX))
                    .ok_or_else(|| corrupt(&path, "journal has no header line"))?
                    .to_owned(),
            ),
        };

        let snapshot = dir.join(SNAPSHOT);
        let mut json: serde_json::Value =
            serde_json::from_slice(&fs::read(&snapshot).at(&snapshot)?)
                .map_err(|e| corrupt(&snapshot, e))?;
        if let Some(older) = rules.as_deref().filter(|r| *r != RULES) {
            upgrade_snapshot(&mut json, older).map_err(|e| corrupt(&snapshot, e))?;
        }
        let state: State = serde_json::from_value(json).map_err(|e| corrupt(&snapshot, e))?;
        let mut kernel = Kernel::from_state(state, blobs)?;
        let mut head = kernel.state_hash();
        let mut valid = if rules.is_some() { lines[0].len() } else { 0 };
        for (n, line) in lines.iter().enumerate().skip(1) {
            let record = line
                .strip_suffix(b"\n")
                .ok_or_else(|| "record is incomplete".to_owned())
                .and_then(decode);
            let record = match record {
                Ok(record) => record,
                Err(_) if n + 1 == lines.len() => break,
                Err(reason) => return Err(corrupt(&path, format!("record {n}: {reason}"))),
            };
            valid += line.len();
            let tick = kernel.world().tick();
            if record.tick <= tick {
                continue;
            }
            if rules.as_deref() != Some(RULES) {
                return Err(StoreError::RulesChanged {
                    journal: rules.unwrap_or_default(),
                    kernel: RULES,
                });
            }
            if record.tick != tick + 1 {
                return Err(corrupt(
                    &path,
                    format!("record {n} is for tick {} after tick {tick}", record.tick),
                ));
            }
            if record.prior != head {
                return Err(StoreError::Diverged {
                    tick: record.tick,
                    expected: record.prior,
                    actual: head,
                });
            }
            if let Some(hash) = missing_blob(&kernel, &record.inputs) {
                return Err(KernelError::MissingBlob(hash).into());
            }
            head = kernel.step(record.inputs).state_hash;
        }

        let mut journal = OpenOptions::new().append(true).open(&path).at(&path)?;
        if rules.as_deref() != Some(RULES) {
            journal
                .set_len(0)
                .and_then(|_| journal.write_all(&header()))
                .and_then(|_| journal.sync_all())
                .at(&path)?;
        } else if valid < bytes.len() {
            journal
                .set_len(valid as u64)
                .and_then(|_| journal.sync_all())
                .at(&path)?;
        }
        Ok(Store {
            _lock: held,
            dir,
            kernel,
            journal,
            head,
            poisoned: false,
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn kernel(&self) -> &Kernel {
        &self.kernel
    }

    pub fn add_blob(&mut self, bytes: impl Into<Arc<[u8]>>) -> Result<BlobHash, StoreError> {
        let bytes = bytes.into();
        let hash = BlobHash::of(&bytes);
        let path = self.dir.join(BLOBS).join(hash.to_hex());
        if !path.exists() {
            write_atomic(&path, &bytes)?;
        }
        Ok(self.kernel.add_blob(bytes))
    }

    pub fn step(&mut self, inputs: Vec<Input>) -> Result<TickReport, StoreError> {
        if self.poisoned {
            return Err(StoreError::Poisoned);
        }
        if let Some(hash) = missing_blob(&self.kernel, &inputs) {
            return Err(KernelError::MissingBlob(hash).into());
        }
        let record = Record {
            tick: self.kernel.world().tick() + 1,
            prior: self.head,
            inputs,
        };
        let line = encode(&record);
        if let Err(source) = self
            .journal
            .write_all(&line)
            .and_then(|_| self.journal.sync_data())
        {
            self.poisoned = true;
            return Err(StoreError::Io {
                path: self.dir.join(JOURNAL),
                source,
            });
        }
        let report = self.kernel.step(record.inputs);
        self.head = report.state_hash;
        Ok(report)
    }

    pub fn checkpoint(&mut self) -> Result<(), StoreError> {
        if self.poisoned {
            return Err(StoreError::Poisoned);
        }
        write_atomic(&self.dir.join(SNAPSHOT), &encode_state(self.kernel.state()))?;
        self.kernel.checkpoint();
        let path = self.dir.join(JOURNAL);
        self.journal
            .set_len(0)
            .and_then(|_| self.journal.write_all(&header()))
            .and_then(|_| self.journal.sync_all())
            .at(&path)?;
        let blobs = self.dir.join(BLOBS);
        for hash in self.kernel.prune_blobs() {
            let file = blobs.join(hash.to_hex());
            fs::remove_file(&file).at(&file)?;
        }
        sync_dir(&blobs)
    }
}

fn missing_blob(kernel: &Kernel, inputs: &[Input]) -> Option<BlobHash> {
    inputs
        .iter()
        .filter_map(|input| input.action.blob())
        .find(|wasm| kernel.blobs().get(wasm).is_none())
        .copied()
}

fn encode_state(state: &State) -> Vec<u8> {
    serde_json::to_vec(state).expect("kernel state serializes")
}
