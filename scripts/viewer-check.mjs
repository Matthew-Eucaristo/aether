import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";

const server = process.argv[2];
const chrome = process.env.CHROME || "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
if (!server) {
  console.error("usage: node scripts/viewer-check.mjs <path to aether-server>");
  process.exit(2);
}

const scratch = mkdtempSync(join(tmpdir(), "aether-viewer-"));
const children = [];
const cleanup = () => {
  for (const c of children) c.kill("SIGKILL");
  rmSync(scratch, { recursive: true, force: true });
};
const fail = (why) => {
  console.error(`viewer check failed: ${why}`);
  cleanup();
  process.exit(1);
};
setTimeout(() => fail("timed out after 90 s"), 90_000).unref();

function lineMatching(stream, pattern) {
  return new Promise((resolve, reject) => {
    const lines = createInterface({ input: stream });
    lines.on("line", (line) => {
      const m = line.match(pattern);
      if (m) resolve(m);
    });
    lines.on("close", () => reject(new Error(`stream ended before ${pattern}`)));
  });
}

const srv = spawn(server, [join(scratch, "world"), "0"], { stdio: ["ignore", "pipe", "inherit"] });
children.push(srv);
const [, port, token] = await lineMatching(srv.stdout, /admin:\s+http:\/\/127\.0\.0\.1:(\d+)\/\?token=(\w+)/);
const base = `http://127.0.0.1:${port}`;

const browser = spawn(chrome, [
  "--headless=new",
  "--remote-debugging-port=0",
  `--user-data-dir=${join(scratch, "chrome")}`,
  "--use-angle=swiftshader",
  "--enable-unsafe-swiftshader",
  "--window-size=1200,800",
  "--no-first-run",
  "about:blank",
], { stdio: ["ignore", "ignore", "pipe"] });
children.push(browser);
const [, devtools] = await lineMatching(browser.stderr, /DevTools listening on (ws:\/\/\S+)/);

const ws = new WebSocket(devtools);
await new Promise((ok, err) => { ws.onopen = ok; ws.onerror = err; });
let nextId = 0;
const pending = new Map();
const problems = [];
ws.onmessage = (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.id !== undefined && pending.has(msg.id)) {
    const { resolve, reject } = pending.get(msg.id);
    pending.delete(msg.id);
    if (msg.error) reject(new Error(JSON.stringify(msg.error)));
    else resolve(msg.result);
    return;
  }
  if (msg.method === "Runtime.exceptionThrown") {
    const d = msg.params.exceptionDetails;
    const text = d.exception?.description || d.text;
    if (!/pointer lock/i.test(text)) problems.push(`exception: ${text}`);
  }
  if (msg.method === "Runtime.consoleAPICalled" && msg.params.type === "error") {
    problems.push(`console.error: ${msg.params.args.map((a) => a.value ?? a.description).join(" ")}`);
  }
};
const send = (method, params = {}, sessionId) => new Promise((resolve, reject) => {
  const id = ++nextId;
  pending.set(id, { resolve, reject });
  ws.send(JSON.stringify({ id, method, params, sessionId }));
});

const { targetId } = await send("Target.createTarget", { url: "about:blank" });
const { sessionId } = await send("Target.attachToTarget", { targetId, flatten: true });
await send("Runtime.enable", {}, sessionId);
await send("Page.enable", {}, sessionId);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function evaluate(expression) {
  const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true }, sessionId);
  if (r.exceptionDetails) fail(`evaluating ${expression.slice(0, 60)}: ${r.exceptionDetails.exception?.description}`);
  return r.result.value;
}
async function open(query) {
  await send("Page.navigate", { url: `${base}/?${query}` }, sessionId);
  for (let i = 0; i < 100; i++) {
    await sleep(100);
    const ready = await send("Runtime.evaluate", { expression: "typeof world === 'object' && world.you !== null", returnByValue: true }, sessionId);
    if (ready.result.value === true) return;
  }
  fail(`the viewer at ?${query} never received its hello`);
}

await open(`token=${token}`);
const installed = await evaluate(`(async () => {
  const ex = await fetch("/capsules").then((r) => r.json());
  const wasm = await upload(ex.realm);
  act({ op: "install", wasm, grants: [], budget: world.law.ceiling });
  for (let i = 0; i < 50 && !Object.keys(world.capsules).length; i++) await new Promise((r) => setTimeout(r, 100));
  return Object.keys(world.capsules).length;
})()`);
if (installed !== 1) fail(`the realm did not install (capsules: ${installed})`);

await open("player=probe");
const played = await evaluate(`(async () => {
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const start = world.tick;
  document.getElementById("play").click();
  for (let i = 0; i < 50 && play.avatar === null; i++) await wait(100);
  if (play.avatar === null) return "no avatar after joining";
  play.locked = true;
  play.keys.add("KeyW");
  await wait(1500);
  play.keys.clear();
  window.dispatchEvent(new KeyboardEvent("keydown", { code: "Period" }));
  for (let i = 0; i < 50 && num(world.entities.get(play.avatar) || { components: {} }, "dim") !== 1; i++) await wait(100);
  const body = world.entities.get(play.avatar);
  if (!body || num(body, "dim") !== 1) return "the body never entered dimension 1: " + JSON.stringify({ locked: play.locked, dim: body && body.components.dim, events: document.getElementById("events").innerText.slice(-400) });
  if (currentDim() !== 1) return "the viewer did not follow the body into dimension 1";
  await wait(500);
  if (world.tick <= start) return "the world did not tick";
  return "ok";
})()`);
if (played !== "ok") fail(played);
await sleep(500);
if (problems.length) fail(problems.join("\n"));
console.log("viewer check passed: loaded, joined, walked and changed dimension without page errors");
cleanup();
process.exit(0);
