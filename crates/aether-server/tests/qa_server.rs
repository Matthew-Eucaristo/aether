use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

static WORLD: AtomicU64 = AtomicU64::new(0);

fn b64encode(bytes: &[u8]) -> String {
    const ALPHA: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = ((chunk[0] as u32) << 16)
            | ((*chunk.get(1).unwrap_or(&0) as u32) << 8)
            | (*chunk.get(2).unwrap_or(&0) as u32);
        out.push(ALPHA[(n >> 18) as usize & 63] as char);
        out.push(ALPHA[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHA[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHA[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

struct Server {
    child: Child,
    port: u64,
    token: String,
    dir: String,
    stdout: mpsc::Receiver<String>,
    start_tick: u64,
}

fn drain_pipe(pipe: impl std::io::Read + Send + 'static, tx: mpsc::Sender<String>) {
    std::thread::spawn(move || {
        let mut r = BufReader::new(pipe);
        let mut line = String::new();
        loop {
            line.clear();
            match r.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if tx.send(line.clone()).is_err() {
                        return;
                    }
                }
            }
        }
    });
}

fn spawn_server(dir: &str) -> Server {
    let mut child = Command::new(env!("CARGO_BIN_EXE_aether-server"))
        .args([dir, "0"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("server binary spawns");
    let (tx, rx) = mpsc::channel();
    drain_pipe(child.stdout.take().expect("piped stdout"), tx);
    let log = format!("{dir}.stderr.log");
    let mut file = std::fs::File::create(&log).expect("stderr log");
    let mut stderr = child.stderr.take().expect("piped stderr");
    std::thread::spawn(move || {
        use std::io::copy;
        let _ = copy(&mut stderr, &mut file);
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut token = None;
    let mut start_tick = None;
    let mut port = None;
    while Instant::now() < deadline {
        let line = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("server prints startup lines");
        if let Some(at) = line.find("on tick ") {
            start_tick = Some(
                line[at + 8..]
                    .trim()
                    .parse()
                    .expect("startup line carries a tick number"),
            );
        }
        if let Some(at) = line.find("http://127.0.0.1:") {
            let rest = &line[at + 17..];
            let end = rest.find('/').expect("a url has a path");
            port = Some(rest[..end].parse().expect("the url carries a port"));
        }
        if let Some(at) = line.find("token=") {
            token = Some(line[at + 6..].trim().to_string());
            break;
        }
    }
    Server {
        child,
        port: port.expect("server prints its address"),
        token: token.expect("server prints an admin token"),
        dir: dir.to_string(),
        stdout: rx,
        start_tick: start_tick.expect("server prints its starting tick"),
    }
}

fn fresh_server(tag: &str) -> Server {
    let n = WORLD.fetch_add(1, Ordering::SeqCst);
    let dir = format!(
        "{}/qa-world-{tag}-{}-{n}",
        env!("CARGO_TARGET_TMPDIR"),
        std::process::id()
    );
    let _ = std::fs::remove_dir_all(&dir);
    spawn_server(&dir)
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    fn stop_sigterm(mut self) -> Vec<String> {
        let pid = self.child.id();
        assert!(
            Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .status()
                .expect("kill runs")
                .success()
        );
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => {
                    assert!(status.success(), "clean exit on SIGTERM, got {status}");
                    break;
                }
                Ok(None) => {
                    assert!(
                        Instant::now() < deadline,
                        "server exits within 30s of SIGTERM"
                    );
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => panic!("wait failed: {e}"),
            }
        }
        let mut lines = Vec::new();
        while let Ok(line) = self.stdout.recv_timeout(Duration::from_millis(300)) {
            lines.push(line);
        }
        lines // Drop on self below is a harmless no-op: the child already exited
    }

    fn kill9(&mut self) {
        self.child.kill().expect("SIGKILL via child.kill");
        let _ = self.child.wait();
    }

    fn restart(&self) -> Server {
        spawn_server(&self.dir)
    }
}

struct Ws {
    stream: TcpStream,
    buf: Vec<u8>,
}

impl Ws {
    fn handshake_request(query: &str) -> String {
        let key = b64encode(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
        format!(
            "GET /ws?{query} HTTP/1.1\r\nHost: 127.0.0.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        )
    }

    fn connect(port: u64, query: &str) -> std::io::Result<Ws> {
        let mut stream = TcpStream::connect(("127.0.0.1", port as u16))?;
        stream.set_read_timeout(Some(Duration::from_secs(15)))?;
        stream.set_write_timeout(Some(Duration::from_secs(15)))?;
        stream.write_all(Self::handshake_request(query).as_bytes())?;
        let mut buf = Vec::new();
        let mut byte = [0u8; 1];
        while !buf.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte)?;
            buf.push(byte[0]);
            assert!(buf.len() < 8192, "response headers too long");
        }
        let head = String::from_utf8_lossy(&buf);
        assert!(
            head.starts_with("HTTP/1.1 101"),
            "upgrade accepted, got: {head:?}"
        );
        buf.clear();
        Ok(Ws { stream, buf })
    }

    fn handshake_status(port: u64, query: &str) -> u32 {
        let mut stream = TcpStream::connect(("127.0.0.1", port as u16)).expect("tcp connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream
            .write_all(Self::handshake_request(query).as_bytes())
            .unwrap();
        let mut buf = Vec::new();
        let mut byte = [0u8; 1];
        while !buf.ends_with(b"\r\n\r\n") {
            if stream.read_exact(&mut byte).is_err() {
                return 0;
            }
            buf.push(byte[0]);
        }
        let head = String::from_utf8_lossy(&buf);
        head.split(' ')
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }

    fn send_frame(&mut self, opcode: u8, payload: &[u8]) -> std::io::Result<()> {
        let mask: [u8; 4] = [0x12, 0x34, 0x56, 0x78];
        let mut header = vec![0x80 | opcode];
        let n = payload.len();
        if n < 126 {
            header.push(0x80 | n as u8);
        } else if n < 65536 {
            header.push(0x80 | 126);
            header.extend_from_slice(&(n as u16).to_be_bytes());
        } else {
            header.push(0x80 | 127);
            header.extend_from_slice(&(n as u64).to_be_bytes());
        }
        self.stream.write_all(&header)?;
        self.stream.write_all(&mask)?;
        let masked: Vec<u8> = payload
            .iter()
            .enumerate()
            .map(|(i, b)| b ^ mask[i % 4])
            .collect();
        self.stream.write_all(&masked)?;
        Ok(())
    }

    fn send_text(&mut self, text: &str) -> std::io::Result<()> {
        self.send_frame(1, text.as_bytes())
    }

    fn read_exact_n(&mut self, n: usize) -> std::io::Result<Vec<u8>> {
        while self.buf.len() < n {
            let mut chunk = [0u8; 16384];
            let got = self.stream.read(&mut chunk)?;
            if got == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "eof",
                ));
            }
            self.buf.extend_from_slice(&chunk[..got]);
        }
        Ok(self.buf.drain(..n).collect())
    }

    fn recv_msg(&mut self) -> std::io::Result<(u8, Vec<u8>)> {
        let mut payload = Vec::new();
        let mut opcode = 0xFF;
        loop {
            let hdr = self.read_exact_n(2)?;
            let fin = hdr[0] & 0x80 != 0;
            let op = hdr[0] & 0x0F;
            let mut n = (hdr[1] & 0x7F) as u64;
            if n == 126 {
                let ext = self.read_exact_n(2)?;
                n = u16::from_be_bytes([ext[0], ext[1]]) as u64;
            } else if n == 127 {
                let ext = self.read_exact_n(8)?;
                n = u64::from_be_bytes(ext.try_into().unwrap());
            }
            assert_eq!(hdr[1] & 0x80, 0, "server frames must be unmasked");
            let chunk = self.read_exact_n(n as usize)?;
            match op {
                9 => {
                    self.send_frame(0xA, &chunk)?;
                    continue;
                }
                8 => return Ok((0xFF, chunk)),
                0 => {}
                _ => opcode = op,
            }
            payload.extend_from_slice(&chunk);
            if fin {
                return Ok((opcode, payload));
            }
        }
    }

    fn recv_json(&mut self) -> std::io::Result<serde_json::Value> {
        loop {
            let (op, payload) = self.recv_msg()?;
            if op == 2 {
                continue; // halfs binary frame
            }
            if op == 0xFF {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "close frame",
                ));
            }
            assert_eq!(op, 1, "expected text frame");
            return Ok(serde_json::from_slice(&payload).expect("server sends valid json"));
        }
    }

    fn wait_tick(&mut self) -> serde_json::Value {
        loop {
            let msg = self.recv_json().expect("recv");
            if msg["type"] == "tick" {
                return msg;
            }
        }
    }

    fn spawn_probe(&mut self) -> serde_json::Value {
        self.send_text(
            r#"{"type":"act","action":{"op":"spawn","components":{"probe":{"num":1}}}}"#,
        )
        .unwrap();
        loop {
            let tick = self.wait_tick();
            let inputs = tick["inputs"].as_array().unwrap();
            let outcomes = tick["report"]["inputs"].as_array().unwrap();
            for (inp, out) in inputs.iter().zip(outcomes.iter()) {
                if inp["action"]["op"] == "spawn"
                    && inp["action"]["components"]["probe"]["num"] == 1.0
                {
                    return out.clone();
                }
            }
        }
    }
}

#[test]
fn auth_matrix_is_enforced() {
    let server = fresh_server("auth");
    assert_eq!(Ws::handshake_status(server.port, ""), 403, "no credentials");
    assert_eq!(
        Ws::handshake_status(server.port, "token=deadbeef"),
        403,
        "wrong token"
    );
    assert_eq!(
        Ws::handshake_status(server.port, "token="),
        403,
        "empty token"
    );
    assert_eq!(
        Ws::handshake_status(server.port, &format!("token={}", server.token)),
        101,
        "admin token"
    );
    assert_eq!(
        Ws::handshake_status(server.port, "player=ana"),
        101,
        "plain player"
    );
    assert_eq!(
        Ws::handshake_status(server.port, "player="),
        403,
        "empty player"
    );
    assert_eq!(
        Ws::handshake_status(server.port, &format!("player={}", "a".repeat(33))),
        403,
        "33 char player"
    );
    assert_eq!(
        Ws::handshake_status(server.port, &format!("player={}", "a".repeat(32))),
        101,
        "32 char player"
    );
    assert_eq!(
        Ws::handshake_status(server.port, "player=a%20b"),
        403,
        "space in player"
    );
    assert_eq!(
        Ws::handshake_status(server.port, "player=a%3Ab"),
        403,
        "colon in player"
    );
    assert_eq!(
        Ws::handshake_status(server.port, "token=deadbeef&player=ana"),
        403,
        "a wrong token must not fall back to player identity"
    );
}

#[test]
fn malformed_messages_do_not_break_session_or_world() {
    let server = fresh_server("malformed");
    let mut ws = Ws::connect(server.port, "player=garbage").unwrap();
    let hello = ws.recv_json().unwrap();
    assert_eq!(hello["type"], "hello");
    for garbage in [
        "not json at all",
        "",
        "null",
        "[1,2,3]",
        r#"{"type":"explode"}"#,
        r#"{"type":"act"}"#,
        r#"{"type":"act","action":{"op":"nuke"}}"#,
        r#"{"type":"act","action":{"op":"set","entity":"x","key":"k","value":{"num":1}}}"#,
        r#"{"type":"upload","id":"abc","source":"x"}"#,
    ] {
        ws.send_text(garbage).unwrap();
    }
    ws.send_frame(2, b"\x00\xff\x01binary").unwrap(); // binary frames are ignored
    let out = ws.spawn_probe();
    assert!(out["spawned"].is_u64(), "spawn after garbage works: {out}");
    assert!(
        ws.wait_tick()["report"]["tick"].is_u64(),
        "world still ticks"
    );
}

#[test]
fn cross_player_modification_is_rejected() {
    let server = fresh_server("identity");
    let mut alice = Ws::connect(server.port, "player=alice").unwrap();
    alice.recv_json().unwrap();
    let out = alice.spawn_probe();
    let eid = out["spawned"].as_u64().unwrap();
    let mut bob = Ws::connect(server.port, "player=bob").unwrap();
    bob.recv_json().unwrap();
    bob.send_text(&format!(
        r#"{{"type":"act","action":{{"op":"set","entity":{eid},"key":"probe","value":{{"num":2}}}}}}"#
    ))
    .unwrap();
    loop {
        let tick = bob.wait_tick();
        let inputs = tick["inputs"].as_array().unwrap();
        let outcomes = tick["report"]["inputs"].as_array().unwrap();
        for (inp, out) in inputs.iter().zip(outcomes.iter()) {
            if inp["actor"] == serde_json::json!({"player": "bob"}) && inp["action"]["op"] == "set"
            {
                assert!(
                    out["rejected"].as_str().unwrap().contains("may not modify"),
                    "bob must not modify alice's entity, got {out}"
                );
                return;
            }
        }
    }
}

#[test]
fn install_with_missing_blob_is_rejected_to_the_sender() {
    let server = fresh_server("missingblob");
    let mut ws = Ws::connect(server.port, "player=installer").unwrap();
    ws.recv_json().unwrap();
    let fake = "1".repeat(64);
    ws.send_text(&format!(
        r#"{{"type":"act","action":{{"op":"install","wasm":"{fake}","grants":[],"budget":{{"fuel_per_tick":1000000,"memory_bytes":1048576,"writes_per_tick":64,"spawns_per_tick":4}}}}}}"#
    ))
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        let msg = ws.recv_json().unwrap();
        if msg["type"] == "rejected" {
            assert!(
                msg["error"].as_str().unwrap().contains("never uploaded"),
                "{msg}"
            );
            return;
        }
    }
    panic!("install with a missing blob was dropped without telling the sender");
}

#[test]
fn sigterm_checkpoints_and_restart_resumes_exactly() {
    let server = fresh_server("sigterm");
    let mut ws = Ws::connect(server.port, "player=walker").unwrap();
    ws.recv_json().unwrap();
    let out = ws.spawn_probe();
    let eid = out["spawned"].as_u64().unwrap();
    let mut last_tick = 0;
    for _ in 0..5 {
        last_tick = ws.wait_tick()["report"]["tick"].as_u64().unwrap();
    }
    assert!(last_tick > 0);
    let dir = server.dir.clone();
    let lines = server.stop_sigterm();
    let ck_line = lines.iter().find(|l| l.contains("checkpointed tick"));
    assert!(
        ck_line.is_some(),
        "SIGTERM must produce a checkpoint, lines: {lines:?}"
    );
    let ck_tick: u64 = ck_line
        .unwrap()
        .split("checkpointed tick ")
        .nth(1)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(ck_tick >= last_tick, "checkpoint covers all observed ticks");

    let restarted = spawn_server(&dir);
    assert_eq!(
        restarted.start_tick, ck_tick,
        "restart resumes exactly at the checkpointed tick"
    );
    let mut ws2 = Ws::connect(restarted.port, "player=walker2").unwrap();
    let hello = ws2.recv_json().unwrap();
    let resumed = hello["state"]["world"]["tick"].as_u64().unwrap();
    assert!(resumed >= ck_tick, "state never goes backwards");
    assert!(
        resumed <= ck_tick + 10,
        "restart reconnect happens within a second"
    );
    assert!(
        hello["state"]["world"]["entities"][eid.to_string()].is_object(),
        "probe entity survives the restart"
    );
    let t = ws2.wait_tick()["report"]["tick"].as_u64().unwrap();
    assert!(t > resumed, "world keeps ticking after resume");
}

#[test]
fn sigkill_replays_journal_consistently() {
    let mut server = fresh_server("sigkill");
    let mut ws = Ws::connect(server.port, "player=durable").unwrap();
    ws.recv_json().unwrap();
    let out = ws.spawn_probe();
    let eid = out["spawned"].as_u64().unwrap();
    let mut max_tick = 0;
    for _ in 0..12 {
        max_tick = ws.wait_tick()["report"]["tick"].as_u64().unwrap();
    }
    server.kill9();
    let restarted = server.restart();
    assert!(
        restarted.start_tick >= max_tick,
        "every tick a client saw was journaled before it was sent: resumed {} vs seen {}",
        restarted.start_tick,
        max_tick
    );
    let mut ws2 = Ws::connect(restarted.port, "player=durable2").unwrap();
    let hello = ws2.recv_json().unwrap();
    assert!(
        hello["state"]["world"]["entities"][eid.to_string()].is_object(),
        "probe entity survives SIGKILL + replay"
    );
    let t = ws2.wait_tick()["report"]["tick"].as_u64().unwrap();
    let now = hello["state"]["world"]["tick"].as_u64().unwrap();
    assert!(t > now, "world keeps ticking after replay");
}

#[test]
fn join_burst_does_not_stall_the_world_clock() {
    let server = fresh_server("joinstorm");
    let mut admin = Ws::connect(server.port, &format!("token={}", server.token)).unwrap();
    admin.recv_json().unwrap();
    let pad = "y".repeat(4000);
    for _ in 0..25 {
        for i in 0..32 {
            admin
                .send_text(&format!(
                    r#"{{"type":"act","action":{{"op":"spawn","components":{{"pad":{{"text":"{pad}"}},"pos.x":{{"num":{i}.0}}}}}}}}"#
                ))
                .unwrap();
        }
        let _ = admin.wait_tick();
    }
    let mut watcher = Ws::connect(server.port, "player=watcher").unwrap();
    watcher.recv_json().unwrap();
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let storm_port = server.port;
    let mut handles = Vec::new();
    for t in 0..16u8 {
        let stop = Arc::clone(&stop);
        handles.push(std::thread::spawn(move || {
            let mut i = 0u32;
            while !stop.load(Ordering::Relaxed) {
                if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", storm_port as u16)) {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                    let key = b64encode(&[t; 16]);
                    let req = format!(
                        "GET /ws?player=storm{t}_{i} HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
                    );
                    if stream.write_all(req.as_bytes()).is_ok() {
                        let mut byte = [0u8; 1];
                        let mut buf = Vec::new();
                        loop {
                            if stream.read_exact(&mut byte).is_err() {
                                break;
                            }
                            buf.push(byte[0]);
                            if buf.ends_with(b"\r\n\r\n") {
                                break;
                            }
                        }
                    }
                    let _ = stream.shutdown(std::net::Shutdown::Both);
                }
                i = i.wrapping_add(1);
            }
        }));
    }
    let storm_secs = 6;
    let start = Instant::now();
    let mut ticks = 0u64;
    watcher
        .stream
        .set_read_timeout(Some(Duration::from_secs(storm_secs + 10)))
        .unwrap();
    while start.elapsed() < Duration::from_secs(storm_secs) {
        match watcher.recv_json() {
            Ok(msg) if msg["type"] == "tick" => ticks += 1,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    stop.store(true, Ordering::Relaxed);
    for h in handles {
        let _ = h.join();
    }
    let rate = ticks as f64 / storm_secs as f64;
    assert!(
        rate >= 8.0,
        "world must keep ticking at ~10Hz during a join storm; observed {rate:.1} ticks/s"
    );
}

#[test]
fn a_client_close_is_answered_with_a_close_frame() {
    let server = fresh_server("close");
    let mut ws = Ws::connect(server.port, "player=closer").unwrap();
    ws.recv_json().unwrap();
    ws.send_frame(8, &1000u16.to_be_bytes()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        let (op, _) = ws
            .recv_msg()
            .expect("the server answers before closing the connection");
        if op == 0xFF {
            return;
        }
    }
    panic!("no close frame came back");
}

fn spawn_at(ws: &mut Ws, tag: f64, x: Option<f64>) -> u64 {
    let pos = x.map_or(String::new(), |x| {
        format!(r#","pos.x":{{"num":{x}}},"pos.y":{{"num":0}}"#)
    });
    ws.send_text(&format!(
        r#"{{"type":"act","action":{{"op":"spawn","components":{{"tag":{{"num":{tag}}}{pos}}}}}}}"#
    ))
    .unwrap();
    loop {
        let tick = ws.wait_tick();
        let inputs = tick["inputs"].as_array().unwrap();
        let outcomes = tick["report"]["inputs"].as_array().unwrap();
        for (inp, out) in inputs.iter().zip(outcomes.iter()) {
            if inp["action"]["components"]["tag"]["num"] == tag {
                return out["spawned"].as_u64().unwrap();
            }
        }
    }
}

#[test]
fn viewers_get_what_is_near_their_focus_and_lose_what_leaves_it() {
    let server = fresh_server("interest");
    let mut admin = Ws::connect(server.port, &format!("token={}", server.token)).unwrap();
    admin.recv_json().unwrap();
    let here = spawn_at(&mut admin, 1.0, Some(5.0));
    let far = spawn_at(&mut admin, 2.0, Some(1000.0));
    let global = spawn_at(&mut admin, 3.0, None);
    let mut ws = Ws::connect(server.port, "player=watcher").unwrap();
    let hello = ws.recv_json().unwrap();
    let entities = &hello["state"]["world"]["entities"];
    assert!(
        entities[here.to_string()].is_object(),
        "near entities come in the hello"
    );
    assert!(
        entities[global.to_string()].is_object(),
        "entities without a place go to everyone"
    );
    assert!(
        entities[far.to_string()].is_null(),
        "far entities stay with the server"
    );
    ws.send_text(r#"{"type":"focus","dim":0,"x":1000,"y":0}"#)
        .unwrap();
    let (mut arrived, mut gone) = (false, false);
    for _ in 0..30 {
        let tick = ws.wait_tick();
        arrived |= tick["changed"][far.to_string()].is_object();
        gone |= tick["left"]
            .as_array()
            .unwrap()
            .iter()
            .any(|id| id.as_u64() == Some(here));
        if arrived && gone {
            return;
        }
    }
    panic!("moving the focus must bring the far entity ({arrived}) and drop the near one ({gone})");
}

#[test]
fn focus_dimension_updates_drop_and_restore_positioned_entities() {
    let server = fresh_server("interestdim");
    let mut admin = Ws::connect(server.port, &format!("token={}", server.token)).unwrap();
    admin.recv_json().unwrap();
    admin
        .send_text(
            r#"{"type":"act","action":{"op":"spawn","components":{"probe":{"num":1},"pos.x":{"num":0},"pos.y":{"num":0}}}}"#,
        )
        .unwrap();
    let mut eid = None;
    for _ in 0..10 {
        let tick = admin.wait_tick();
        let inputs = tick["inputs"].as_array().unwrap();
        let outcomes = tick["report"]["inputs"].as_array().unwrap();
        for (inp, out) in inputs.iter().zip(outcomes.iter()) {
            if inp["action"]["op"] == "spawn" && inp["action"]["components"]["probe"]["num"] == 1.0
            {
                eid = out["spawned"].as_u64();
            }
        }
        if eid.is_some() {
            break;
        }
    }
    let eid = eid.expect("probe spawn outcome");
    admin
        .send_text(&format!(
            r#"{{"type":"act","action":{{"op":"set","entity":{eid},"key":"dim","value":{{"num":5}}}}}}"#
        ))
        .unwrap();
    let mut saw_left = false;
    for _ in 0..10 {
        let tick = admin.wait_tick();
        if tick["left"]
            .as_array()
            .is_some_and(|ids| ids.iter().any(|id| id.as_u64() == Some(eid)))
        {
            saw_left = true;
            break;
        }
    }
    assert!(
        saw_left,
        "moving the probe to dimension 5 must remove it from dimension 0 view"
    );
    admin
        .send_text(r#"{"type":"focus","dim":5,"x":0,"y":0}"#)
        .unwrap();
    let key = eid.to_string();
    let mut restored = false;
    for _ in 0..10 {
        let tick = admin.wait_tick();
        if tick["changed"][&key].is_object() {
            restored = true;
            break;
        }
    }
    assert!(
        restored,
        "after focusing dimension 5 the probe is sent again"
    );
}

#[test]
fn a_player_cannot_forge_another_players_body_marker() {
    let server = fresh_server("bodymarker");
    let mut attacker = Ws::connect(server.port, "player=attacker").unwrap();
    attacker.recv_json().unwrap();
    attacker
        .send_text(
            r#"{"type":"act","action":{"op":"spawn","components":{"avatar":{"bool":true},"player":{"text":"player:victim"},"pos.x":{"num":1000},"pos.y":{"num":0},"dim":{"num":999},"secret":{"text":"hidden"}}}}"#,
        )
        .unwrap();
    let mut fake = None;
    for _ in 0..20 {
        let tick = attacker.wait_tick();
        let inputs = tick["inputs"].as_array().unwrap();
        let outcomes = tick["report"]["inputs"].as_array().unwrap();
        for (inp, out) in inputs.iter().zip(outcomes.iter()) {
            if inp["actor"] == serde_json::json!({"player":"attacker"})
                && inp["action"]["op"] == "spawn"
            {
                fake = out["spawned"].as_u64();
            }
        }
        if fake.is_some() {
            break;
        }
    }
    let fake = fake.expect("attacker's spoofed body spawn outcome");
    let mut victim = Ws::connect(server.port, "player=victim").unwrap();
    let hello = victim.recv_json().unwrap();
    let entity = &hello["state"]["world"]["entities"][fake.to_string()];
    assert!(
        entity.is_null(),
        "victim must not receive attacker's hidden dimension entity just because it says player:victim; got {entity}"
    );
}

#[test]
fn resync_after_a_full_outbox_still_includes_the_player_body() {
    let server = fresh_server("resyncbody");
    let mut admin = Ws::connect(server.port, &format!("token={}", server.token)).unwrap();
    let admin_hello = admin.recv_json().unwrap();

    let mut components = serde_json::Map::new();
    components.insert("pos.x".into(), serde_json::json!({"num": 0}));
    components.insert("pos.y".into(), serde_json::json!({"num": 0}));
    for i in 0..60 {
        components.insert(
            format!("pad{i:02}"),
            serde_json::json!({"text": "x".repeat(4000)}),
        );
    }
    let spawn_msg = serde_json::json!({
        "type":"act",
        "action":{"op":"spawn","components":components}
    })
    .to_string();
    admin.send_text(&spawn_msg).unwrap();
    let mut pad_entity = None;
    for _ in 0..20 {
        let tick = admin.wait_tick();
        for out in tick["report"]["inputs"].as_array().unwrap() {
            if let Some(id) = out["spawned"].as_u64() {
                pad_entity = Some(id);
            }
        }
        if pad_entity.is_some() {
            break;
        }
    }
    let pad_entity = pad_entity.expect("large padded entity spawn outcome");

    admin
        .send_text(
            &serde_json::json!({
                "type":"upload",
                "id":1,
                "source":include_str!("../../../capsules/realm.wat")
            })
            .to_string(),
        )
        .unwrap();
    let mut wasm = None;
    for _ in 0..30 {
        let msg = admin.recv_json().unwrap();
        if msg["type"] == "uploaded" && msg["id"] == 1 {
            wasm = msg["result"]["Ok"].as_str().map(str::to_owned);
            break;
        }
    }
    let wasm = wasm.expect("realm upload succeeds");
    let budget = admin_hello["state"]["law"]["ceiling"].clone();
    admin
        .send_text(
            &serde_json::json!({
                "type":"act",
                "action":{"op":"install","wasm":wasm,"grants":[],"budget":budget}
            })
            .to_string(),
        )
        .unwrap();
    let mut hub = None;
    for _ in 0..30 {
        let tick = admin.wait_tick();
        if tick["capsules"].as_object().is_some_and(|c| !c.is_empty()) {
            hub = tick["capsules"]
                .as_object()
                .and_then(|c| c.values().next())
                .and_then(|c| c["self_entity"].as_u64());
        }
        if hub.is_some() {
            break;
        }
    }
    let hub = hub.expect("realm capsule appears");

    let mut victim = Ws::connect(server.port, "player=victim").unwrap();
    let first = victim.recv_json().unwrap();
    assert_eq!(first["type"], "hello");
    victim
        .send_text(
            &serde_json::json!({
                "type":"act",
                "action":{"op":"signal","entity":hub,"name":"join","value":0}
            })
            .to_string(),
        )
        .unwrap();
    let mut body = None;
    for _ in 0..30 {
        let tick = victim.wait_tick();
        for (id, entity) in tick["changed"].as_object().unwrap() {
            if entity["components"]["player"]["text"] == "player:victim" {
                body = id.parse::<u64>().ok();
            }
        }
        if body.is_some() {
            break;
        }
    }
    let body = body.expect("realm body reaches its player before the outage");

    admin
        .send_text(r#"{"type":"focus","dim":999,"x":1000000,"y":0}"#)
        .unwrap();
    for round in 0..80 {
        let pad = format!("{round:04}{}", "y".repeat(3996));
        let msg = serde_json::json!({
            "type":"act",
            "action":{"op":"set","entity":pad_entity,"key":"pad00","value":{"text":pad}}
        })
        .to_string();
        admin.send_text(&msg).unwrap();
        loop {
            let tick = admin.wait_tick();
            let saw = tick["inputs"].as_array().unwrap().iter().any(|input| {
                input["actor"] == serde_json::json!("admin")
                    && input["action"]["op"] == "set"
                    && input["action"]["entity"] == pad_entity
                    && input["action"]["key"] == "pad00"
                    && input["action"]["value"]["text"] == pad
            });
            if saw {
                break;
            }
        }
    }

    let deadline = Instant::now() + Duration::from_secs(20);
    let mut frames = 0;
    loop {
        assert!(
            Instant::now() < deadline,
            "no resync hello after outbox recovery"
        );
        let msg = victim.recv_json().expect("victim drains backed-up frames");
        frames += 1;
        if msg["type"] == "hello" {
            assert!(
                msg["state"]["world"]["entities"][body.to_string()].is_object(),
                "resync hello after {frames} frames omits the player body"
            );
            println!("resync hello after {frames} queued frames includes body {body}");
            break;
        }
    }
}
