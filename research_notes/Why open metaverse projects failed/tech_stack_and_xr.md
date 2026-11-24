# Technical building blocks and engineering lessons for a shared, user-scriptable, multiplayer world kernel, and the XR platform landscape (as of 24 Nov 2026)

Method note: these notes come from web searches and page fetches run on 24 Nov 2026. Items marked "(prior knowledge, not re-fetched)" are well-established facts I did not re-verify this session; their URLs are the canonical locations as I remember them. Several 2026 facts come only from secondary trade sites (vr.org, shattered.io, tech-insider.org) and are flagged where used.

## 1. Simulation and networking: which model, how others scaled, and what numbers they reached

### Takeaway
Every system that shipped at scale uses an authoritative server holding state in memory, splits it into regions or shards, and caps the number of avatars per simulation unit: about 100 to 200 on Roblox (700 only in a closed beta), one region per process in Second Life, 20 ticks/s in Minecraft. The distributed "seamless world" middleware vendors (Improbable SpatialOS, Hadean) produced impressive demos but few lasting customers. The 2025–26 trend, led by SpacetimeDB/BitCraft, is the opposite: run game logic as sandboxed modules inside a single-node in-memory transactional database and shard by region. Croquet-style replicated computation suits small collaborative sessions, not large untrusted worlds.

### Cited Findings
**SpacetimeDB / BitCraft (Clockwork Labs)**
- SpacetimeDB runs server logic inside the database process ("server and database form one integrated system"). Modules compile to Wasm, and JS/TypeScript modules run on V8. All tables are currently held in RAM, with disk-backed tables only planned. The post claims strong serializability but says no Jepsen analysis has been done yet. — [SpacetimeDB blog, "Let's talk benchmarks", 14 May 2026](https://spacetimedb.com/blog/benchmarking)
- SpacetimeDB 2.0 launched in March 2026 with a claim of ">100,000" end-to-end transfer TPS. On 14 May 2026 the team corrected the methodology: the original run used a Rust client with 16,384-deep pipelining on the 5-node closed-source Cloud. The corrected single-node Standalone results are 279,025 ± 4,763 TPS uncontended and 303,920 ± 4,712 TPS contended (α=1.5), with p50/p99 latency of 8.08/12.9 ms. For comparison, Node+Postgres reached 9,905 TPS uncontended and 961 contended, and Convex 1,140/127. The original SQLite comparison numbers were invalid because the queries never executed. — [SpacetimeDB blog](https://spacetimedb.com/blog/benchmarking)
- Lessons the post states itself: server-to-DB round trips cap throughput (Little's Law), hot rows serialize under lock contention, and benchmark disclosure (regions, replication, client) matters. — [SpacetimeDB blog](https://spacetimedb.com/blog/benchmarking)
- Criticism on Hacker News: the database does not scale horizontally beyond one node. A SpacetimeDB developer replied that reliability and correctness come before raw performance. This is an informal source. — [HN thread](https://hn.svelte.dev/item/49378933)
- All BitCraft logic runs through SpacetimeDB, from chat to real-time player movement. A podcast recounts a launch-day cascading failure caused by logging while holding a lock. The podcast is a secondary source. — [BitCraft news](https://bitcraftonline.com/news/spacetimedb-and-bitcraft); [podcast](https://www.podbean.com/ew/dir-dan9i-2ad561da)
- The BitCraft server was open-sourced (reported as Apache-licensed). It is one SpacetimeDB module with "all data stored in tables and all logic running inside reducers". Its config has a `region_count` field that must be a square number, so the world is a grid of regions. The client is not open because Clockwork lacks rights to some assets. Phase 2 (sidecar services) was announced in January 2026. The repo docs restrict running competing servers. — [BitCraft open-sourcing update](https://bitcraftonline.com/news/bitcraft-open-sourcing-update); [mmohuts](https://mmohuts.com/news/bitcraft-online-starts-open-sourcing-its-code-with-an-apache-licensed-server-release); [BitCraftPublic repo](https://github.com/clockworklabs/BitCraftPublic)

**Improbable SpatialOS and Hadean**
- SpatialOS was pitched as a platform for "massive, persistent and highly populated" cloud worlds. Its first big customer was Bossa's Worlds Adrift. Bloomberg reported that Worlds Adrift was one of at least three SpatialOS games pulled within a year, and that developers paid 8% to 35% more per hour on even the smallest US templates. — [Yahoo/Bloomberg](https://au.finance.yahoo.com/news/softbank-backed-effort-revolutionize-gaming-060025319.html); [PC Gamer](https://www.pcgamer.com/worlds-adrifts-persistent-world-building-tech-being-made-free-to-qualified-game-developers)
- Hadean/CCP EVE: Aether Wars at GDC 2019: 3,852 human players, 14,274 total ships, 10,412 concurrent including AI-driven clients. The login servers could not keep up, delaying the start about 30 minutes, and the simulation could not hold a steady 30 Hz tick. — [CCP Games](https://www.ccpgames.com/news/2019/eve-aether-wars-successfully-achieves-groundbreaking-14-000-ship-battle); [GamesRadar](https://www.gamesradar.com/eve-aether-wars-preview-e3-2019)

**Roblox**
- Roblox tested 700 players in one server in a 2020 beta. The public cap was later 200 per server, and enrollment in the 700 beta closed. Developers report instability well below 700 and suspect a per-server memory ceiling around 4.5 GB; that is a user guess. — [DevForum 2020](https://devforum.roblox.com/t/max-player-count-increased-in-beta/722045); [DevForum "Increasing server size to 700"](https://devforum.roblox.com/t/increasing-server-size-to-700/3669484); [DevForum "Server Player Limit"](https://devforum.roblox.com/t/server-player-limit/2800295)
- Roblox's scale comes from matchmaking across many small servers: up to 4 billion join combinations evaluated per second at peak, routed to on-prem and cloud edge "cells" (June 2025). — [Roblox newsroom](https://about.roblox.com/newsroom/2025/06/roblox-infrastructure-supporting-record-breaking-games)

**Second Life**
- A region simulator nominally runs at 45 frames/s, about 22 ms per frame, and that budget is shared by everything in the region, scripts included. "Spare time" is the unused remainder. — [Linden Lab support](https://lindenlab.freshdesk.com/support/solutions/articles/31000147177/thumbs_up); [SL forums](https://community.secondlife.com/forums/topic/424030-modern-sim-lag-times/)
- Anecdotal and old (2005): each avatar crossing into a region briefly cut time dilation by about 5% and the sim frame rate from 45 to 39. — [SL forum archive](https://forums-archive.secondlife.com/54/aa/296012/1.html)

**Minecraft** (prior knowledge, not re-fetched)
- The server game loop is fixed at 20 ticks/s (50 ms). — [Minecraft Wiki: Tick](https://minecraft.wiki/w/Tick)
- PaperMC's Folia fork adds "regionized multithreading": independent groups of chunks tick on separate threads, which breaks many plugins. — [PaperMC Folia docs](https://docs.papermc.io/folia)

**Croquet / Multisynq (replicated computation)**
- The reflector only orders and timestamps messages. Every client runs the same deterministic computation ("bit identical"), and the reflector knows nothing about application state. Croquet library 2.0 (8 Apr 2025) moved to Apache-2.0 and runs by default on the Multisynq DePIN reflector network; self-hosted reflectors are allowed. Reported reflector latency is as low as 10–15 ms (2021 claim). — [Croquet 2.0 README](https://app.unpkg.com/@croquet/croquet@2.0.4/files/README.md); [Clojureverse discussion](https://clojureverse.org/t/croquet-io-fascinating-approach-to-build-real-time-multiuser-apps/8178); [EdgeIR 2021](https://www.edgeir.com/croquet-aims-to-change-edge-compute-game-with-collaboration-platform-offering-new-capabilities-for-interactive-experiences-20210907)

### Inferences
- An authoritative server with regions as the unit of simulation, law, and failure is the proven pattern: Second Life regions, BitCraft's `region_count` grid, Roblox servers. The founder's "per-region laws as code" maps naturally onto this. Make a region one transactional state machine on one core or node, and handle cross-region effects as asynchronous messages, not shared transactions.
- Croquet-style lockstep needs bit-exact determinism on every client and gives every client the full state. That rules it out as the primary model for a world with untrusted user code, private state, and AI agents. It may still suit small private sub-sessions such as a shared whiteboard.
- SpacetimeDB is the closest existing match to "authoritative world state, sandboxed Wasm behaviors, transactional reducers". Prototype on it, or copy its model (reducers as transactions over tables, subscription queries for interest management). Before committing, check its license (BSL for the core, as I recall; not verified this session) and its single-node ceiling.
- Practical capacity targets from these data points: roughly 50 to 200 interacting avatars per region at 20 to 30 Hz. Above that, everyone in the record cases (EVE time dilation, Aether Wars) degraded tick rate rather than holding it. Plan graceful degradation (time dilation, LOD of simulation) from day one.
- SpatialOS lesson: middleware that charges per-hour cloud fees and requires deep engine integration was too costly and too risky for small studios. An open-source, self-hostable kernel avoids this but must still keep hosting cheap.

### Gaps
- No primary source found for BitCraft's live players per region or tick rate.
- SpatialOS's current status (deprecated or wound down) is not confirmed. I recall the January 2019 Unity terms-of-service dispute that cut off SpatialOS games and Improbable's later exit from games, but did not verify either this session.
- No authoritative current figure for the Second Life per-region avatar cap. I recall about 100 on full regions, unverified.
- No hard 2026 numbers on interest-management schemes (spatial hashing, AOI grids) from primary engineering posts.

## 2. ECS engines and frameworks: which suits a kernel decoupled from the renderer

### Takeaway
For a headless, authoritative kernel, a library ECS (Bevy's `bevy_ecs` used headless, or Flecs) is a better base than a full engine. Clients can then be any renderer: Godot for OpenXR/Android XR/Quest, Three.js or Babylon for WebXR. Bevy is still pre-1.0 (0.19, June 2026) with no official editor. Godot has the strongest open-source XR story but is not an ECS. Unity DOTS is converging with GameObjects slowly. Ambient, the closest prior attempt at a "WASM-ECS multiplayer engine", paused indefinitely.

### Cited Findings
- Bevy 0.18 was released 13 Jan 2026 (174 contributors, 659 PRs), with 0.18.1 on 1 Mar 2026. "Bevy Feathers" is an experimental widget library built for the upcoming Bevy Editor. — [Bevy 0.18 release notes](https://bevy.org/news/bevy-0-18/); [Bevy GitLab tags](https://gitlab.com/bevyengine/bevy/-/tags)
- Bevy 0.19 (about 22 Jun 2026) introduced BSN (Bevy Scene Notation) "Next Generation Scenes" via the `bsn!` macro: "composable, patchable, and dependency aware". Asset-defined BSN is promised for a later release, and Feathers was ported to BSN. I found no confirmation that an official Bevy Editor has shipped. — [GameFromScratch](https://gamefromscratch.com/bevy-0-19-released/)
- Godot's lead explained why the engine is node/scene-tree based rather than ECS (2021). (prior knowledge, not re-fetched) — [godotengine.org](https://godotengine.org/article/why-isnt-godot-ecs-based-game-engine/)
- Godot has official Android XR support from Godot 4.6.2 via OpenXR plus the Godot OpenXR Vendors plugin v5.1, itself a GDExtension. Godot 4.6 bundles the Khronos OpenXR loader for Android. — [Android XR Godot docs](https://developer.android.com/develop/xr/godot); [Godot XR update May 2026](https://godotengine.org/article/godot-xr-update-may-2026/); [Android Developers Blog May 2026](https://android-developers.googleblog.com/2026/05/android-xr-updates-unity-unreal-godot.html)
- Google's "Android XR Engine Hub" became stable for Unity, Godot, and Unreal (Nov 2026). Godot and Unity can be embedded as libraries inside Jetpack Compose apps using OpenXR Spatial Containers. — [Android Developers Blog, 6 Nov 2026](https://developer.android.com/blog/posts/build-android-xr-devices-tools)
- Unity is moving ECS toward the GameObject world step by step: ECS packages became core packages in Unity 6.4, a shared "EntityId" replaces instance IDs in 6.5, and the unified Hierarchy is the default in 6.6. The December 2025 status update switched from a big-bang release to incremental delivery, and later roadmap language points at "Unity 7" and a CoreCLR transition. — [Unity Dec 2025 ECS status](https://discussions.unity.com/t/ecs-development-status-december-2025/1699284/1); [Unity 6.6 status](https://discussions.unity.com/t/ecs-development-status-for-unity-6-6/1724480); [Digital Production on 2026 roadmap](https://digitalproduction.com/2025/11/26/unitys-2026-roadmap-coreclr-verified-packages-fewer-surprises/)
- Flecs v4 unified filters, queries, and rules in one query engine. Flecs was the first open-source ECS with full entity relationships, and the Rust binding README says it has been used in AAA games. — [Flecs releases](https://github.com/SanderMertens/flecs/releases); [flecs_ecs docs.rs](https://docs.rs/crate/flecs_ecs/0.2.2)
- Cautionary tale: Hytale picked Flecs for a C++ engine rewrite (2024 technical explainer). After Riot cancelled the game in June 2025 and the founders bought it back, the team dropped the rewrite. It shipped early access on 13 Jan 2026 on the legacy engine (Java server with its own server-side ECS), because the new engine trailed in gameplay features and needed about 2 more years. — [Hytale ECS explainer](https://hytale.com/news/2024/6/summer-2024-technical-explainer-hytale-s-entity-component-system-oPwpCAMdI); [Windows Central](https://www.windowscentral.com/gaming/cancelled-by-riot-hytales-creator-revives-the-game-fans-thought-was-dead); [Wccftech](https://wccftech.com/hytale-minecraft-like-mmo-resurrected-will-launch-early-access-soon/)
- Ambient (WASM + ECS multiplayer engine): the README says "Work on the Ambient runtime is paused indefinitely"; the team moved to a product called Braindump. At its last release the ECS was single-threaded and save/load was unfinished. In its design, the ECS was the data backbone and every package ran inside WebAssembly. — [Ambient README](https://raw.githubusercontent.com/AmbientRun/Ambient/main/README.md); [Ambient ECS docs](https://ambientrun.github.io/Ambient/reference/ecs.html)

### Inferences
- Recommended split: write the kernel in Rust on a headless ECS (`bevy_ecs` as a standalone crate, or Flecs through its Rust binding) or directly on SpacetimeDB tables. Keep the network schema renderer-neutral. Build the first XR client in Godot (OpenXR plus Vendors plugin covers Quest, Android XR, PC VR) and the web client in Three.js or Babylon with WebXR. This avoids tying the kernel to Bevy's pre-1.0 churn; Bevy has broken APIs on every release so far.
- Ambient is the most direct precedent for this founder (WASM-ECS, multiplayer by default, Rust). It stalled before finding users even with venture backing. Ship a narrow, usable thing early rather than a general runtime.
- Hytale shows that rewriting the engine is the biggest schedule risk. Avoid writing a renderer at all.

### Gaps
- I did not verify the current status of the Bevy OpenXR crates (`bevy_mod_openxr`, community-maintained) or of Bevy WebGPU/WASM performance numbers.
- No official reason found for Ambient's pause.
- No confirmed Flecs v4 release date or current list of games using it.

## 3. Sandboxing user code: WASM runtimes, metering, determinism, Lua/Luau, V8 isolates, incidents

### Takeaway
Wasmtime with the component model (WASI 0.3 stable since June 2026) is the default choice for sandboxed behavior capsules. Fuel gives deterministic per-tick budgets at roughly 15–40% overhead; epoch interruption costs about 10% but is not deterministic. Determinism needs NaN canonicalization and no relaxed SIMD. 2026 produced the largest batches of Wasmtime sandbox-escape CVEs so far, several found by LLM tooling, so treat the runtime as one defense layer and patch it fast. Luau, used by Roblox and now by Second Life's SLua, is the proven alternative for a scripting language aimed at end users.

### Cited Findings
**Metering and interruption (Wasmtime)**
- Epoch-based interruption measured at "around a 10% slowdown", and faster than fuel. Fuel is "completely deterministic": the same program with the same fuel always stops at the same point. Epoch interruption is not deterministic. — [Wasmtime docs: Interrupting Wasm](https://docs.wasmtime.dev/examples-interrupting-wasm.html)
- A contributor benchmark put fuel metering overhead at 15–40%, with a proposed "slacked" fuel scheme at 5–20%. This is an experiment, not an official figure. — [Wasmtime issue #4109 (Zulip archive)](https://bytecodealliance.github.io/zulip-archive/stream/225357-git-wasmtime/topic/wasmtime.20.2F.20issue.20.234109.20Slacked.20fuel.20metering.html)

**WASI and the component model**
- WASI 0.3.0 was released 11 Jun 2026 with native async in the component model's Canonical ABI. `wasi:io` is removed in favor of `async func`, `stream<T>`, and `future<T>`. Minimums are Wasmtime 46+ and wit-bindgen 0.46+. WASI 0.3.1 adds `map<K,V>` (adopted 6 Aug 2026). — [wasi.dev releases](https://wasi.dev/releases/wasi-p3); [Bytecode Alliance WASI 0.3](https://bytecodealliance.org/articles/WASI-0.3); [Migration guide](https://component-model.bytecodealliance.org/design/migrating-to-p3.html)

**Security incidents (2026)**
- 9 Apr 2026: 12 Wasmtime advisories at once, "the largest set Wasmtime has ever published". Two were Critical (CVSS 9.0): a Cranelift aarch64 sandbox escape (CVE-2026-34971) and a Winch aarch64 escape (CVE-2026-34987). Others covered component-model string handling and a pooling-allocator data leak between instances. 11 of the 12 were found by the team's LLM-based multi-agent harness during a 3-week sprint with Mozilla, UCSD, Akamai, and F5. Stated lessons: no continuous fuzzing of Cranelift or Winch on aarch64 ("a significant shortcoming"); a lagging formal model would have caught the Cranelift bug; LLM-based vulnerability discovery becomes a tier-1 requirement for new features. — [Bytecode Alliance, 9 Apr 2026](https://bytecodealliance.org/articles/wasmtime-security-advisories)
- October 2026: two more Critical (CVSS 9.9) Cranelift sandbox escapes were reported: a type confusion in very large functions on all architectures, and an aarch64 bug that still affected hosts patched in April. The CVE numbers reported (CVE-2026-115889/-115893) look unusual and I could not verify them on NVD; check the GHSA entries. — [The Hacker News, Oct 2026](https://thehackernews.com/2026/10/two-critical-wasmtime-flaws-could-let.html); [SOCRadar](https://socradar.io/blog/wasmtime-sandbox-escape-cranelift/)
- Also in 2026: a WASI filesystem sandbox escape via trailing slashes in paths and symlinks (HIGH 8.8). — [RUSTSEC-2026-0269](https://rustsec.org/advisories/RUSTSEC-2026-0269)

**Luau (Roblox) and SLua (Second Life)**
- Luau sandbox: `io` and `package` are removed, `os` and `debug` are cut down, and globals and library tables are read-only. Each Roblox script gets its own global table. Bytecode loading is disabled ("bytecode should be encrypted/signed" if embedded). The host interrupt handler is "guaranteed to call this handler eventually", but a long-running C call can delay it. No default memory limits. Untrusted and trusted code should run in separate VMs. Sandboxing is "not formally proven"; memory safety relies on fuzzing. — [luau.org/sandbox](https://luau.org/sandbox)
- Second Life's Luau-based SLua entered beta in December 2025 on "SLua Beta" regions of the main grid, alongside LSL. A September 2026 user-group meeting expected the Lua Editor viewer to become the 26.4 beta/RC and move to Luau 0.732. A VS Code extension exists. I found no general-availability date. — [SL Wiki: Lua FAQ / Try SLua](https://wiki.secondlife.com/wiki/Try_SLua); [modemworld SUG summary, 1 Sep 2026](https://modemworld.me/2026/09/01/); [SLua repo](https://github.com/secondlife/slua)

**Other runtimes** (prior knowledge, not re-fetched)
- Extism is a cross-language plugin framework on top of Wasm runtimes, with host SDKs in many languages. — [extism.org](https://extism.org)
- V8 isolates (Cloudflare Workers): many tenants per process, with isolate start-up quoted at about 5 ms (2018). — [Cloudflare blog](https://blog.cloudflare.com/cloud-computing-without-containers/)
- Wasm float nondeterminism: NaN bit patterns are nondeterministic in the spec. Wasmtime exposes a Cranelift NaN-canonicalization option, and relaxed-SIMD results vary by platform unless forced deterministic. — [Wasm spec numerics](https://webassembly.github.io/spec/core/exec/numerics.html); [Wasmtime Config docs](https://docs.rs/wasmtime/latest/wasmtime/struct.Config.html)
- SpacetimeDB now runs TypeScript modules on V8 as well as Rust/C# compiled to Wasm, and states that it worked on improving V8's threading model. — [SpacetimeDB blog](https://spacetimedb.com/blog/benchmarking)

### Inferences
- Budgets per tick: use Wasmtime fuel when replays and audits must be deterministic, which a region whose laws are code will want. Use epochs as a wall-clock backstop. Set store memory limits through `ResourceLimiter`. Bill host calls separately, because Luau's docs warn that host APIs are the hard part.
- Determinism checklist: enable NaN canonicalization, disable or force deterministic relaxed SIMD, use no threads inside capsules, and route time and randomness only through host capabilities. Fixed-point math for physics-critical "laws" is an option.
- Defense in depth: run untrusted capsules in a separate process per region or tenant, prefer x86-64 hosts until aarch64 fuzzing coverage catches up (both April Critical escapes were aarch64-only), keep the runtime easy to upgrade, and fuzz the host API surface yourself.
- Authoring language: the end-user language matters more than the VM. Roblox and Second Life both chose Luau. A Wasm kernel can host Luau compiled to Wasm, or ship a JS or TS path (as SpacetimeDB does), so creators and AI agents get one well-documented language.

### Gaps
- No independent 2026 benchmark found comparing Wasmtime, Wasmer, WAMR, and V8 overhead for game-tick workloads.
- WAMR and Wasmer 2026 status and security history were not checked.
- No published numbers for Luau per-script CPU budgets on Roblox servers.

## 4. Capability security: ocap model, Hardened JS, Spritely Goblins, Habitat, and untrusted code in worlds

### Takeaway
Object capabilities (authority equals holding an unforgeable reference) are the only security model built for mutually suspicious code in a shared world. Its lineage runs straight from virtual worlds: Lucasfilm Habitat, then Electric Communities, then E, then Agoric's Hardened JS and Spritely's Goblins and OCapN. OCapN is still a moving draft (Goblins 0.18 broke wire compatibility).

### Cited Findings
- Goblins 0.18.0 added "sleepy actors" (a caching layer), changed OCapN to merge `op:deliver-only` into `op:deliver` (incompatible with earlier releases), and fixed a signing-oracle vulnerability in WebSocket netlayer authentication. — [Spritely: Goblins v0.18.0](https://spritely.institute/news/spritely-goblins-v0-18-0-sleepy-actors.html)
- The Spritely Institute's stated mission is "networked user freedom": standards and base implementations for decentralized communities. Goblins is a "distributed programming environment"; OCapN is the protocol suite for its "distributed networked capability architecture". The current about page does not mention virtual worlds. — [Spritely about](https://spritely.institute/about/)
- Christine Lemmer-Webber's founding Spritely post (2018) framed the project as secure social spaces in the form of virtual worlds. (prior knowledge, not re-fetched) — [dustycloud.org](https://dustycloud.org/blog/spritely/)
- Habitat (Lucasfilm, 1986–88) lessons: a cyberspace is defined by its communications protocol more than its graphics, and designers cannot centrally plan player behavior. (prior knowledge, not re-fetched) — [Morningstar & Farmer, "The Lessons of Lucasfilm's Habitat"](http://www.fudco.com/chip/lessons.html)
- E and Mark Miller's thesis "Robust Composition" (2006) set out the object-capability model and promise pipelining. These ideas fed into SES and Hardened JavaScript, which Agoric uses for smart contracts. (prior knowledge, not re-fetched) — [erights.org thesis](http://www.erights.org/talks/thesis/); [hardenedjs.org](https://hardenedjs.org)
- Luau's guidance shows the ambient-authority alternative is weaker: per-script global tables give isolation, but `getfenv`/`setfenv` complicate it, and the docs recommend separate VMs for trusted and untrusted code. — [luau.org/sandbox](https://luau.org/sandbox)

### Inferences
- Map ocaps onto the planned kernel this way: a capsule receives only handles (an entity handle with specific verbs, a region-scoped "emit sound" capability, a rate-limited "spawn" capability) as component-model resources. The WIT `resource` type is a natural unforgeable-handle carrier inside Wasm.
- Region "laws" then become capability attenuators: membranes that wrap handles passed into a region and enforce or revoke them, rather than checks scattered through code. Revocation via membranes is a standard ocap pattern.
- For federation, OCapN (CapTP-style references with promise pipelining) fits inter-server calls. It is still a draft with breaking changes, so isolate it behind an adapter.
- Electric Communities' lesson (prior knowledge, not re-verified): its secure distributed world shipped technology ahead of a product and the company collapsed. The founder should avoid "secure distributed object system first, world later".

### Gaps
- No primary source fetched on Electric Communities Habitat or EC history.
- OCapN standardization status (any formal standards body, test suite, interoperating implementations beyond Goblins and Agoric) was not checked for 2026.
- No examples found of a shipped multi-user 3D world enforcing ocaps end to end.

## 5. Asset and scene interop: glTF, OpenUSD, VRM, MaterialX, and portable behaviors

### Takeaway
glTF 2.0 is the runtime delivery format: Android XR SceneCore loads and animates glTF directly, and the web stack is built on it. OpenUSD is the authoring and composition format, and its Core Specification 1.0 was ratified in December 2025. Portable behaviors are not solved: the Core Spec explicitly excludes interactivity, and the glTF interactivity work and the AOUSD interactivity interest group are still in progress.

### Cited Findings
- AOUSD Core Specification 1.0 (17 Dec 2025) gives the first normative definition of OpenUSD's data models, composition algorithm, and value resolution. It does not cover materials, animation semantics, physics, or rendering. Version 1.1 (in progress) adds animation splines (parity with OpenUSD 25.08) and variable and path expressions. — [Linux Foundation press release](https://www.linuxfoundation.org/press/alliance-for-openusd-announces-core-specification-1.0-the-universal-language-for-building-3d-worlds); [Digital Production](https://digitalproduction.com/2026/01/05/aousd-releases-openusd-core-specification-1-0/)
- In March 2026 AOUSD formed a "Characters, Motion, and Interactivity Interest Group" and shifted focus to scalability and performance. — [LF press, Mar 2026](https://www.linuxfoundation.org/press/aousd_prmarch2026)
- USD and glTF alignment remains at the level of talks and discussion, such as a 2025 GTC panel with Autodesk, Bentley, Khronos, NVIDIA, and Pixar. — [GamesBeat](https://gamesbeat.com/alliance-for-openusd-unveils-core-specification-1-0-for-industrial-metavese-standard/)
- Android XR's stable Jetpack SceneCore animates glTF models, manipulates sub-nodes, applies custom PBR materials, and generates procedural meshes at runtime. — [Android Developers Blog, 6 Nov 2026](https://developer.android.com/blog/posts/build-android-xr-devices-tools)
- Prior knowledge, not re-fetched: Khronos has been developing `KHR_interactivity`, a node-graph behavior extension for glTF (with KHR_audio and KHR_physics drafts alongside) — [Khronos glTF repo](https://github.com/KhronosGroup/glTF). VRM 1.0 is a glTF-based humanoid avatar format from the VRM Consortium — [vrm.dev](https://vrm.dev/en/). MaterialX is the ASWF open material-graph standard used inside USD — [materialx.org](https://materialx.org). The Open Metaverse Interoperability Group publishes OMI_* glTF extensions (physics, audio, seats, spawn points) — [omigroup GitHub](https://github.com/omigroup/gltf-extensions).

### Inferences
- Make glTF (with KHR material extensions plus VRM for avatars) the canonical runtime asset. Accept USD on import for authoring pipelines. Do not invent a scene format.
- For portable behaviors, define the kernel's capsule interface (WIT worlds) as the behavior standard and treat glTF interactivity graphs as an optional import target. Behaviors portable across worlds need a shared capability vocabulary (move, emit, grab), not just shared bytecode.
- Store scenes as ECS component data with glTF asset references. That keeps the kernel independent of both formats.

### Gaps
- I did not verify the 2026 ratification status of `KHR_interactivity`, `KHR_physics_rigid_bodies`, or `KHR_audio_emitter`.
- 2026 status of VRM and MaterialX was not checked.

## 6. XR platform landscape (Nov 2026), device recommendation for a solo developer, streaming

### Takeaway
As of late November 2026: OpenXR is the common runtime API (Android XR, Meta, PICO, Valve/SteamVR, Monado). Android XR has one headset (Galaxy XR, $1,799) and new XREAL Aura wired glasses ($1,279), with display-less AI glasses shipping in December. Meta Quest remains the volume platform, but Meta paused third-party Horizon OS headsets and moved Horizon Worlds to mobile. Apple Vision Pro is a niche product at $3,499 or more, and its Safari supports immersive WebXR without AR. The Android XR emulator does not run OpenXR apps, so a developer using Godot or OpenXR needs hardware. A solo developer targeting OpenXR plus Android XR should buy a Quest 3 (or 3S) now for cheap OpenXR iteration, and add a Galaxy XR only when Android XR-specific testing is needed.

### Cited Findings
**Android XR**
- Samsung Galaxy XR launched 21 Oct 2025 at $1,799 (US, Korea first), reached the UK at £1,699 (shipping 8 Jul 2026), and supports OpenXR, WebXR, and Unity. No unit sales have been published, and analysts expect modest sales. — [MacRumors](https://www.macrumors.com/2025/10/21/samsung-galaxy-xr-headset/); [Samsung UK](https://news.samsung.com/uk/samsung-galaxy-xr-arrives-in-the-uk); [heise](https://heise.de/-11075503)
- XREAL Aura, the first wired XR glasses on Android XR (under 95 g, with a compute puck), launched at Android Dev Summit in November 2026 from $1,279. The 16 GB/512 GB model is $1,499. — [Android Developers Blog, 6 Nov 2026](https://developer.android.com/blog/posts/build-android-xr-devices-tools); [9to5Google](https://9to5google.com/2026/10/27/xreal-aura-is-coming-in-november-with-a-special-200-off-launch-discount/); [Digital Trends](https://digitaltrends.com/wearables/xreal-puts-a-1279-price-tag-on-its-first-android-xr-glasses/)
- Samsung's first Android XR "intelligent eyewear" (no display, camera and audio, Gemini, Snapdragon AR1) opened pre-orders 12 Nov 2026 in Warby Parker and Gentle Monster frames, shipping in December. The price was not confirmed; one aggregator says $395. Display glasses are reportedly late 2027. — [9to5Google](https://9to5google.com/2026/11/05/android-xr-glasses-pre-order/); [The Gadgeteer](https://the-gadgeteer.com/2026/11/06/samsung-android-xr-glasses-preorders-november-12/)
- Jetpack XR core libraries (SceneCore, ARCore for Jetpack XR, XR Runtime) and the Android XR emulators reached stable in November 2026. The ARCore Geospatial APIs are also stable. — [Android Developers Blog](https://developer.android.com/blog/posts/build-android-xr-devices-tools)
- The emulator "is not currently supported for OpenXR or Unity applications of any kind". On macOS it requires Apple Silicon and 16 GB+ RAM. — [Android XR emulator docs](https://developer.android.com/develop/xr/jetpack-xr-sdk/run/emulator/xr-headsets-glasses); [setup docs](https://developer.android.com/develop/xr/jetpack-xr-sdk/setup)
- Early hardware access goes through the Android XR Developer Catalyst Program (g.co/dev/catalyst). — [9to5Google](https://9to5google.com/2026/05/19/xreal-project-aura-android-xr-developers-2026-launch/)

**OpenXR**
- Khronos's conformant list includes Google Android XR, ByteDance/PICO (OpenXR 1.0 and 1.1, April 2026), Collabora Monado, HTC, Acer, and Canon. The spatial-entity extensions (`XR_EXT_spatial_entity` and related anchor and persistence extensions) are adopted by Google, Meta, PICO, and Varjo. OpenXR "Spatial Containers" extensions standardize volumetric multitasking, and Google says Android XR will support them. — [Khronos conformant products](https://www.khronos.org/conformance/adopters/conformant-products/openxr); [Khronos spatial containers blog](https://www.khronos.org/blog/openxr-spatial-containers); [UploadVR](https://www.uploadvr.com/openxr-spatial-container-extensions-standardize-volumetric-app-multitasking/)

**Meta**
- Meta "paused" the third-party Horizon OS headset program (ASUS ROG, Lenovo) in December 2025 to focus on first-party hardware, saying it "will revisit opportunities". I found no reversal in 2026. — [Road to VR](https://roadtovr.com/meta-horizon-os-third-party-headset-cancelled-asus-lenovo/); [UploadVR](https://www.uploadvr.com/meta-pauses-third-party-horizon-os-headsets/)
- Meta's 2026 strategy (19 Feb 2026): Reality Labs is "right-sizing"; Horizon Worlds goes "all-in on mobile" and is pulled from the VR store; 86% of headset time is in third-party apps; nearly $150M went to VR developer programs in 2025; payment volume was roughly flat in 2025 with in-app purchases up 13%. — [Meta developer blog](https://developers.meta.com/vr/blog/2026-vr-state-of-the-union-horizon-mobile-focus/)
- US list prices rose in April 2026, citing memory costs: Quest 3S 128 GB $349.99, 3S 256 GB $449.99, Quest 3 512 GB $599.99. No new Quest model had been announced as of autumn 2026. These figures come from comparison sites, so check meta.com. — [tech-insider.org](https://tech-insider.org/quest-3s-vs-quest-3-2026/); [vr.org](https://vr.org/quest-3-vs-quest-3s); [shattered.io](https://shattered.io/best-vr-headset-2026-quest-3s-349-99-vs-2027-launches/)

**Valve, Apple, WebXR**
- Valve Steam Frame (standalone, Snapdragon): $1,059 (256 GB) and $1,299 (1 TB), launched in 2026 (vr.org says 14 Sep 2026). It runs Android APKs through a compatibility layer called "Lepton", and Quest-targeted APKs can be reused. OpenXR support was not confirmed in these sources; SteamVR itself is an OpenXR runtime (prior knowledge). — [UploadVR](https://www.uploadvr.com/steam-frame-price-revealed-reservations-opened-alyx-included/); [Android Central](https://www.androidcentral.com/gaming/vr-games/steam-frame-costs-usd1-059-but-android-games-could-give-it-an-edge); [vr.org](https://vr.org/steam-frame)
- Apple Vision Pro (M5, October 2025) starts at $3,499. A single June 2026 source claims a rise to $3,699; this is unconfirmed. No cheaper model; reports say Apple shifted to glasses for 2027–28. No sales figures published. — [Apple Newsroom](https://www.apple.com/newsroom/2025/10/apple-vision-pro-upgraded-with-the-m5-chip-and-dual-knit-band/); [Engadget](https://www.engadget.com/ar-vr/apples-new-vision-pro-gets-an-m5-chip-and-dual-knit-band-but-its-still-3499-132123957.html); [vr.org (price rise claim)](https://vr.org/articles/vision-pro-price-increase-3699-enterprise-signal-2026)
- WebXR: full support in Chromium (desktop, Android, Android XR with hand input, depth, anchors, and hit-test, and the Quest browser). visionOS Safari supports immersive-vr but not the AR module. Safari on macOS and iOS and Firefox do not support it. WebXR was proposed for Interop 2026. — [Android XR WebXR docs](https://developer.android.com/develop/xr/develop-with-webxr); [caniuse](https://caniuse.com/webxr); [Apple dev forums](https://developer.apple.com/forums/thread/826845)
- Streaming: Khronos lists NVIDIA CloudXR among notable new OpenXR runtimes. — [Khronos OpenXR slides](https://www.khronos.org/developers/linkto/evolution-of-openxr)

### Inferences
- Device recommendation for a solo developer with no headset:
  - Buy a Quest 3 ($599.99) now, or a Quest 3S ($349.99) to save money (lower-resolution lenses). It has the largest install base, a conformant OpenXR runtime, Godot support via the same Vendors plugin used for Android XR, a Chromium browser for WebXR, and an APK path also used by Steam Frame.
  - Build the XR client in Godot or WebXR so the same code reaches Android XR.
  - Use the stable Android XR emulator only for Jetpack/Compose 2D-panel work, since it cannot run OpenXR builds.
  - Buy a Galaxy XR ($1,799) only once Android XR-specific features matter, or apply to the Developer Catalyst program first.
  - Skip Vision Pro unless Apple is a target (no OpenXR, partial WebXR).
- Thin web client first: WebXR in Chromium reaches Quest, Galaxy XR, and desktop with one codebase. That fits a world kernel with thin clients better than native-first.
- Platform risk: Meta's retreat from Horizon Worlds in VR and from Horizon OS licensing, plus modest Galaxy XR and Vision Pro sales, means headset reach is small. A PC/web client is needed for any audience. AI glasses (audio and camera, no display) are the 2026–27 growth category, which suits an "AI-native, interface-agnostic" world more as voice/agent access than as a 3D view.
- For cloud rendering, CloudXR and Android XR or Quest streaming are options for heavy scenes, but they add per-user GPU cost. Keep the client-side rendering budget small instead.

### Gaps
- No unit sales data for Galaxy XR, Vision Pro, Steam Frame, or Quest in 2026.
- Steam Frame OpenXR status and whether it can run Android XR apps are unconfirmed.
- Exact Samsung AI glasses price is unconfirmed.
- I did not check whether Meta's Quest now runs Android XR apps or any 2026 Meta–Google arrangement.
- No 2026 OpenXR runtime count; no current CloudXR pricing or availability.
