# Open-source and open-standard virtual worlds: what they built, why they stalled or survived, lessons

Research date: 2026-11-24. GitHub figures were pulled live from the GitHub REST API on 2026-11-24 (`gh api repos/<repo>`, `/releases`, `/stats/participation`). "Commits/52w" means commits to the default branch over the last 52 weeks. Stars and commit counts are noisy proxies for health. Some repos are mirrors (OpenSim) or issue trackers only (Resonite).

## Project case files: architecture, governance and funding, activity, what happened

### Takeaway
Of the projects studied, the healthy ones in late 2026 are a profitable proprietary world with a creator economy (Second Life), a federated volunteer-run server ecosystem (OpenSim hypergrid), a volunteer non-profit fork of a VC-built codebase (Overte), a small commercial closed-source creation platform (Resonite), and library-level building blocks (A-Frame, three.js, glTF, OpenUSD, VRM). Every VC- or corporate-funded "open metaverse" platform was stopped by its funder: High Fidelity, JanusVR, Mozilla Hubs, Third Room, Ambient, Webaverse. Their code survived, but their users and momentum did not.

### Cited Findings

**Second Life (Linden Lab, 2003 to present; proprietary server, LGPL viewer)**
- Late-2026 scale: "just under 700,000 monthly active users", "fairly stable" for several years, and "just over 11% of users actively scripting and creating content". From an interview with Linden SVP of Product Anya Kanevsky dated 29 Oct 2026 — [80.lv](https://80.lv/articles/second-life-wants-its-23-year-old-world-to-be-a-modern-game-platform)
- Other MAU figures conflict. A fan blog says Executive Chairman Brad Oberwager announced about 620k monthly users in Dec 2025, up from about 500k a year earlier (secondhand, unverified) — [SL Newser 2025 review](https://slnewser.blogspot.com/2026/01/2025-year-in-review.html). A third-party estimate gives 1.1M to 1.4M 30-day uniques (March 2026, unofficial methodology) — [grid statistics blog](https://kelaenderkoe.pages.dev/next/python3-http-danielvoyager.wordpress.com)
- Economy: Linden says it has spent about $1.3B building SL and paid about $1.1B to creators since 2003 (Dec 2024 town hall with Oberwager and Rosedale; company-reported) — [GamesBeat](https://www.gamesbeat.com/games/linden-lab-has-spent-1-3b-building-second-life-and-paid-1-1b-to-creators/). Creators keep "approximately 85% of their sales revenue before applicable cash-out fees". More than 13,000 unique creators cashed out real money in 2026. Creators retain their IP — [80.lv](https://80.lv/articles/second-life-wants-its-23-year-old-world-to-be-a-modern-game-platform)
- For comparison: Roblox paid $2.8B to its community from 2018 to mid-2024, while SL paid about $500M over the same period — [GamesBeat](https://gamesbeat.com/linden-lab-has-spent-1-3b-building-second-life-and-paid-1-1b-to-creators/)
- Scripting: SL is adding SLua (Luau, Roblox's typed Lua) alongside LSL. The two "can coexist — even within the same object". Design requirements:
  - scripts run securely next to thousands of others
  - they can pause and resume and keep their state
  - they can migrate between region servers or into inventory
  - they "remain sandboxed and run server-side on Second Life regions"
  - loadstring() is excluded
  - resource limits and fair scheduling stop one script from monopolising execution
  - C# was rejected over "sandboxing, memory management, micro-threading and state serialization"
  - VS Code integration lets creators edit code tied to live in-world objects

  Source: [80.lv](https://80.lv/articles/second-life-wants-its-23-year-old-world-to-be-a-modern-game-platform); [secondlife.com/create](https://secondlife.com/create)
- No instancing: experiences span contiguous regions. Kanevsky: "We don't want to turn Second Life into a conventional game engine." — [80.lv](https://80.lv/articles/second-life-wants-its-23-year-old-world-to-be-a-modern-game-platform)
- The viewer is open source: [secondlife/viewer](https://github.com/secondlife/viewer) has 310 stars, an LGPL-2.1 license, 1,516 commits/52w, and a release on 2026-11-21. This is a very active codebase.

**OpenSimulator and the hypergrid (2007 to present; volunteer core, many independent grids)**
- Core: the [opensim/opensim](https://github.com/opensim/opensim) GitHub mirror has 290 stars, 211 commits/52w, and a last push on 2026-11-20. The latest stable release I found is 0.9.3.0, dated 8 Nov 2024 — [Wikipedia](https://en.wikipedia.org/wiki/OpenSimulator); [AUR](https://aur.archlinux.org/packages/opensimulator?O=10). I found no 2026 release.
- Ecosystem (Hypergrid Business, 15 Jan 2025):
  - 2,669 public grids tracked, of which 300 were active that month and 249 published stats
  - more than 4,100 new registrations that month
  - OSgrid had the most land, at 35,873 standard region equivalents
  - top grids by 30-day actives: Wolf Territories 7,932; OSgrid 4,743; GBG World 2,512; Darkheart's Playground 2,277; Alternate Metaverse 2,253

  Source: [Hypergrid Business](https://hypergridbusiness.com/?p=78877)
- Total land area reached an all-time high of about 133k to 136k standard regions in 2025 — [Hypergrid Business](https://www.hypergridbusiness.com/?p=78433). An undated report says the hypergrid holds 98% of OpenSim land and 99% of active users — [Hypergrid Business](https://www.hypergridbusiness.com/?p=78433)
- Methodology: the counts are self-reported 30-day unique logins from each grid's stats page, and include hypergrid visitors where possible — [Hypergrid Business](https://www.hypergridbusiness.com/?p=78433)

**Croquet → Open Cobalt → Croquet Corp → Multisynq**
- Croquet was led by Alan Kay, David A. Smith, David P. Reed, Andreas Raab, Julian Lombardi and Mark McCahill. Formal work began around 2001, with roots in a 1994 two-user prototype. Messages sent to a shared object are redirected to replicated copies on every participant's machine — [Wikipedia: Croquet Project](https://en.wikipedia.org/wiki/Croquet_Project)
- The Croquet SDK was released under MIT by HP and the Croquet Consortium in 2007. Development moved to the Open Cobalt fork in 2009 (alpha in April 2010) — [Wikipedia: Open Cobalt](https://en.wikipedia.org/wiki/Open_Cobalt); [UBC review](https://blogs.ubc.ca/immersivelearning/open-cobalt)
- Smith founded Croquet Corporation in 2018 and rewrote the system in JavaScript for the web — [Wikipedia: David A. Smith](https://en.wikipedia.org/wiki/David_A._Smith_(computer_scientist))
- Modern model: "stateless Synchronizers" act only as a shared clock and relay time-stamped events. "No application computation actually occurs on the Synchronizer" — [Multisynq Mirror post](https://mirror.xyz/0x55cf7dffbcdD99952452A1a764239F4059b8a08F/KlLMTKhqb0YU8pxmhhLl7QrKZKV2no7P9ZkoqhrcxYk)
- Pivot to a crypto/DePIN network:
  - anyone can run a Synchronizer node for token rewards — [Monad blog](https://blog.monad.xyz/blog/the-real-pied-piper)
  - the Multisynq Foundation handles the ecosystem, with Croquet Labs as technology provider — [Multisynq Foundation announcement](https://mirror.xyz/0x55cf7dffbcdD99952452A1a764239F4059b8a08F/b9pFkcv1XtuN2MHkwaKe7GDuYGhn_5rWol1nn5aOvwo)
  - CB Insights lists $2.2M raised (possibly stale) — [CB Insights](https://www.cbinsights.com/company/multisynq)
- The Croquet network was deprecated on 30 Jul 2025, and users were told to migrate to Croquet 2.x with a Multisynq API key (per a croquet.io notice relayed by search, not fetched directly) — [croquet.io](https://croquet.io)
- GitHub status:
  - [multisynq/multisynq-client](https://github.com/multisynq/multisynq-client): 236 stars, Apache-2.0, archived, last push 2025-08-25
  - [croquet/croquet](https://github.com/croquet/croquet): 39 stars, 9 commits/52w, last push 2026-11-12
  - [croquet/microverse](https://github.com/croquet/microverse) (the Croquet "metaverse" beta): 191 stars, 0 commits/52w, last push 2025-10

**Mozilla Hubs → Hubs Foundation**
- Hubs launched in 2018 and survived the 2020 cut of Mozilla's mixed-reality team — [Road to VR](https://www.roadtovr.com/mozilla-hubs-shutdown-web-xr/)
- Feb 2024 shutdown timeline: subscriptions off from 1 Mar 2024, a data-download tool on 1 Apr, and the end of the demo server, managed subscriptions and community resources on 31 May 2024 — [Road to VR](https://www.roadtovr.com/mozilla-hubs-shutdown-web-xr/); [Mozilla support](https://support.mozilla.org/en-US/kb/end-support-mozilla-hubs)
- Mozilla's reasons:
  - an organisation-wide restructuring toward Firefox and AI, with about 60 layoffs — [TechCrunch memo](https://techcrunch.com/2024/02/13/mozilla-downsizes-as-it-refocuses-on-firefox-and-ai-read-the-memo)
  - "since early 2023 … a shift in the market for 3D virtual worlds", with demand moving away except in gaming, education and niche uses
  - the Hubs team said user and customer bases "were not robust enough to justify continuing to dedicate resources against the headwinds"

  Source for the last two: [Hubs sunset post](https://hubs.mozilla.com/labs/sunset); [XR Today](https://www.xrtoday.com/?p=50752)
- Architecture as handed over: a self-hostable "Community Edition" on Kubernetes (AWS, Azure, GCP) — [Road to VR](https://www.roadtovr.com/mozilla-hubs-shutdown-web-xr/)
- Hubs Foundation in 2026: [Hubs-Foundation/hubs](https://github.com/Hubs-Foundation/hubs) (the mozilla/hubs repo now redirects there) has 2,227 stars, MPL-2.0, only 19 commits/52w, and a release tag prod-2026-10-29. [hubs-cloud](https://github.com/Hubs-Foundation/hubs-cloud) was pushed 2026-10-29, and the reticulum server repo has 460 stars.
- Recent site posts are maintenance only: a Node/PHP security notice (29 Oct 2026), Kubernetes 1.34+ issues, and Community Edition deployment scripts v2.1.1 (Sept 2026) — [hubsfoundation.org](https://hubsfoundation.org/)
- An undated governance roadmap marks "instate revenue sources" (sponsorships, grants, subscriptions, merch, asset marketplace) as not started, and membership, elections and bylaws as launched — [Hubs Foundation roadmap doc](https://docs.google.com/document/d/1SNb3rFNIH6ATmzrDZrUg_s3jsYecnyWty__idK9A-lc/export?format=pdf)

**Third Room (Matrix, 2022 to 2023)**
- Built by Robert Long (ex-Hubs and AltspaceVR). Tech Preview 1 shipped in Sept 2022 as "open, decentralised, immersive worlds built on Matrix" — [Matrix blog TP1](https://matrix.org/blog/2022/09/27/announcing-third-room-tech-preview-1/)
- Tech Preview 2 ("Creator Update", June 2023) added:
  - WebSG, pitched as an open standard for interoperable programming of worlds, intended for the W3C
  - user-generated scripting in Wasm sandboxes, with QuickJS on top
  - a glTF-based, DOM-like scene API
  - MSC3815, a Matrix spec for virtual worlds

  Source: [Matrix blog TP2](https://matrix.org/blog/2023/06/07/introducing-third-room-tp-2-the-creator-update/); [The New Stack](https://thenewstack.io/third-room-teases-user-generated-content-for-the-metaverse/); [GitHub](https://github.com/matrix-org/thirdroom)
- Third Room was paused in July 2023 for lack of funding, and the team disbanded after hoped-for dedicated funders "did not come through" — [The Libre News](https://thelibre.news/matrix-and-freedesktop-are-calling-for-help)
- Matrix confirmed it is "no longer able to be funded by Element (for now)". Element had largely funded Matrix and is now "focused on being able to pay its own costs rather than being dependent on further VC investment" — [Matrix.org, Apr 2024](https://matrix.org/blog/2024/04/open-source-publicly-funded-service/)
- Repo: [matrix-org/thirdroom](https://github.com/matrix-org/thirdroom) has 657 stars, Apache-2.0, and its last push was 2023-08-30. It is dead.

**Webaverse (2020 to about 2023)**
- Webaverse was a browser-based "open metaverse" engine with a crypto flavour. Its Upstreet MMO was built on the engine — [Bankless](https://www.bankless.com/webaverse-explained-)
- In Oct 2023 it introduced Moemate, an AI character studio — [CB Insights](https://www.cbinsights.com/company/webaverse)
- [webaverse/app](https://github.com/webaverse/app) has 378 stars, MIT license, and its last push was 2022-12-20. An [UpstreetAI/upstreet-core](https://github.com/UpstreetAI/upstreet-core) repo ("Build and deploy AI Agents, fast", last push 2025-03) appears to be the successor org. That link is my inference from the shared "Upstreet" name and is not confirmed.

**High Fidelity → Vircadia → Overte**
- Timeline: Philip Rosedale stepped down at Linden and the first High Fidelity commit landed around 2012–13. High Fidelity pivoted to remote teams and announced the VR shutdown in 2020. Project Athena, later named Vircadia, was created, and Overte split from Vircadia in 2022 — [Overte talk, MiniDebConf Lisbon 2023](https://ftp.sunet.se/pub/debian-meetings/2023/MiniDebConfLisboa/mini_dc-lisbon-03-Introduction_to_the_Overte_project.pdf); [Ryan Schultz](https://ryanschultz.com/2020/04/03/the-project-athena-fork-of-high-fidelity-now-has-a-new-name-vircadia/); [Wikipedia](https://en.wikipedia.org/wiki/High_Fidelity,_Inc.)
- What the codebase is: built by "a company with ~70 employees and ~$70M investment". More than 85,000 commits and 400K lines of code, "now being developed by a group of volunteers". Apache 2.0 — [Overte talk](https://ftp.sunet.se/pub/debian-meetings/2023/MiniDebConfLisboa/mini_dc-lisbon-03-Introduction_to_the_Overte_project.pdf)
- Overte's own diagnosis of High Fidelity: "They made the wrong product — HiFi's structure is very hard to monetize", and they "should have been an OSS company, perhaps with community support" — [same](https://ftp.sunet.se/pub/debian-meetings/2023/MiniDebConfLisboa/mini_dc-lisbon-03-Introduction_to_the_Overte_project.pdf)
- Architecture:
  - C++ and Qt5 with a custom OpenGL 4.5/ES 3.2 renderer
  - Bullet physics, "automatically distributed"
  - positional audio with Opus
  - "Everything is scriptable in JavaScript both client-side and server-side" (QtScript, with V8 in progress)
  - a web-like architecture of self-hosted domains with a metaverse server for identity
  - glTF avatars and models
  - explicitly "not a grid"

  Source: [same](https://ftp.sunet.se/pub/debian-meetings/2023/MiniDebConfLisboa/mini_dc-lisbon-03-Introduction_to_the_Overte_project.pdf)
- Why it split (Overte's account, one-sided):
  - Overte "started from dissatisfied Vircadia contributors (including core team members)"
  - Overte rejects cryptocurrency, while Vircadia welcomes it
  - Overte is a non-profit, while Vircadia has commercial ambitions
  - Overte is "a democracy", while Vircadia has a BDFL
  - Vircadia shipped a non-free HiFiAC audio codec and was moving to WebGL

  Overte is governed by Overte e.V., a German registered non-profit with an elected board and a yearly assembly — [same](https://ftp.sunet.se/pub/debian-meetings/2023/MiniDebConfLisboa/mini_dc-lisbon-03-Introduction_to_the_Overte_project.pdf)
- 2026 status:
  - [overte-org/overte](https://github.com/overte-org/overte): 289 stars, 788 commits/52w, release 2026.10.1 on 2026-10-24, 100 or more contributors listed (the API page is capped at 100)
  - [vircadia/vircadia-native-core](https://github.com/vircadia/vircadia-native-core): 542 stars, 0 commits/52w, last release 2024.1.0 in Mar 2024
  - The volunteer non-profit fork outlived the commercial fork.

**NeosVR → Resonite (Yellow Dog Man Studios)**
- Lead developer Tomáš "Frooxius" Mariančík resigned from Neos in April 2023 after a long dispute with Solirax CEO Karel Hulec over cryptocurrency features. He founded Resonite under Yellow Dog Man Studios, which entered Steam early access in October 2023 — [Wikipedia (mirror)](https://backiee.wasmer.app/http_en_wikipedia_org/wiki/Resonite); [Road to VR](https://roadtovr.com/?p=114236)
- A 2023 Lemmy post says Neos Patreon fell from a peak above $18k/mo to just under $5k/mo, while Resonite opened above $14k/mo. This is a weak, secondary source — [Lemmy](https://lemmy.uninsane.org/post/271320/220324)
- Model:
  - the data model is "the single source of truth for the state of the program"
  - sync is eventually consistent, with the host reconciling users' changes
  - all creation, including ProtoFlux visual scripting, happens in-world, live, in multiplayer

  Source: [Resonite wiki: Data model](https://wiki.resonite.com/Data_model); [Data model synchronization](https://wiki.resonite.com/Data_Model_Synchronization); [ProtoFlux](https://wiki.resonite.com/ProtoFlux)
- Openness: the client and server are closed. ResoniteLink is an open WebSocket protocol "to allow anyone to more easily build external tools… to read and write Resonite's data model" — [NuGet ResoniteLink](https://www.nuget.org/packages/YellowDogMan.ResoniteLink); [Beta 2026.2.11](https://wiki.resonite.com/Beta_2026.2.11.1336). [Yellow-Dog-Man/ResoniteLink](https://github.com/Yellow-Dog-Man/ResoniteLink) has 78 stars and 278 commits/52w.
- 2026 activity:
  - frequent betas, e.g. Beta 2026.5.28 — [wiki](https://wiki.resonite.com/Beta_2026.5.28.1269)
  - [issue tracker](https://github.com/Yellow-Dog-Man/Resonite-Issues) pushed 2026-11-20
  - Steam concurrency averages about 170–200, against an all-time peak of about 1,183–1,307 in Oct 2023 (Steam only; Quest and desktop players are excluded) — [Steambase](https://steambase.io/games/resonite/steam-charts); [SteamDB](https://steamdb.info/app/2519830/charts)

**Ambient (Rust/WASM multiplayer runtime)**
- Architecture: Rust; WASM-isolated game code; WebGPU; an ECS "real-time database" with automatic state sync across multiplayer sessions; packages; a configurable asset pipeline; PBR — [README](https://raw.githubusercontent.com/AmbientRun/Ambient/main/README.md)
- Status: "Work on the Ambient runtime is paused indefinitely." The team moved to Braindump, an AI game-creation product. The company's history runs from closed-source Dims to Ambient to Braindump — [README](https://raw.githubusercontent.com/AmbientRun/Ambient/main/README.md); [AI Engineer org profile](https://www.ai.engineer/orgs/ambient)
- The maintainers say it "remains open-source, and we'd be supportive of anyone… forking" — [GitHub issue](https://github.fontlos.workers.dev/AmbientRun/Ambient/issues/1268)
- [AmbientRun/Ambient](https://github.com/AmbientRun/Ambient): 3,908 stars (the most of any world platform studied), Apache-2.0, 0 commits/52w, latest listed release v0.2.1 in May 2023, last push 2025-01. High star counts did not translate into a self-sustaining community.

**JanusVR / JanusWeb**
- The company shut down in 2019 and open-sourced its code — [Wikipedia](https://en.wikipedia.org/wiki/JanusVR)
- In Feb 2020 it became the community-run JanusXR. The focus moved to the WebXR client JanusWeb, and the presence server, CDN and Vesta hosting moved to community servers — [JanusXR news](https://vesta.janusxr.org/news)
- [jbaicoianu/janusweb](https://github.com/jbaicoianu/janusweb): 208 stars, MIT, 120 commits/52w, last push 2026-09. This is effectively a single-maintainer project. The original [janusvr/janus](https://github.com/janusvr/janus) native client was last pushed in Jan 2022.

**A-Frame / WebXR libraries**
- [aframevr/aframe](https://github.com/aframevr/aframe): 17,716 stars, v1.8.0 on 2026-06-24, 120 commits/52w
- [three.js](https://github.com/mrdoob/three.js): 119k stars, 2,393 commits/52w
- [Babylon.js](https://github.com/BabylonJS/Babylon.js): 26k stars, 1,703 commits/52w
- The engine and library layer is far healthier than any web "world" platform built on it.

**Open Metaverse Foundation (Linux Foundation)**
- Announced 18 Jan 2023 as a vendor-neutral home for open-metaverse standards and software — [LF press](https://linuxfoundation.org/press/open-metaverse-foundation)
- Its site was last updated in May 2024. The newest news is from 2023, no projects or working groups are named, and the ten listed members are mostly small or Asia-Pacific organisations. It appears dormant (my inference, not stated by OMF) — [openmv.org](https://openmv.org/)

**Metaverse Standards Forum (Khronos-hosted)**
- The Forum coordinates requirements and does not write standards itself — [LF press](https://linuxfoundation.org/press/open-metaverse-foundation)
- In 2026 it shipped its first code: the Open Metaverse Browser Initiative's "Sneeze" engine, released 15 Jun 2026 under Apache-2.0 on the Forum's GitHub with RP1 as the main contributor. RP1 launched a consumer "Artemis" browser on 6 Jul 2026 — [MSF press](https://metaverse-standards.org/news/press-releases/metaverse-standards-forum-introduces-sneeze-the-worlds-first-open-metaverse-browser-engine/); [BusinessWire](https://www.businesswire.com/news/home/20260615321859/en/RP1-and-the-Metaverse-Standards-Forum-Introduce-Sneeze-the-First-Metaverse-Browser-Engine-for-Spatial-Computing); [vr.org](https://vr.org/articles/sneeze-open-metaverse-browser-engine-ombi-awe-2026)

**OMI (Open Metaverse Interoperability Group)**
- OMI develops glTF extensions through a W3C Community Group process. Extensions can be promoted to Khronos KHR/EXT status — [omigroup GitHub](https://github.com/omigroup); [gltf-extensions](https://madjin.github.io/gltf-extensions)
- [omigroup/gltf-extensions](https://github.com/omigroup/gltf-extensions) has 58 stars and was last pushed 2026-11-16. [omigroup/omi](https://github.com/omigroup/omi) has 329 stars and 0 commits/52w. Activity is low.

### Inferences
- The decisive variable is who pays for servers and staff once the original funder leaves, not code quality or licence. Hubs (Mozilla), Third Room (Element), High Fidelity (VC), JanusVR (VC) and Ambient (VC) all released or kept open code. All lost their user base when the funder left, because only the code was open: the hosted service, identity, discovery and content were still centralised and died with the funder.
- The survivors each have a self-funding loop:
  - SL: land tiers plus a 10–15% economy take
  - OpenSim: grid operators sell land cheaply and volunteers run the core
  - Resonite: Patreon and supporter subscriptions
  - Overte: volunteer non-profit with self-hosted domains and no central bill
- Crypto pivots preceded or caused several splits and deaths: Neos to Resonite, Vircadia to Overte, Croquet to Multisynq DePIN, and Webaverse. Fork leaders explicitly used "no crypto" as a community-trust signal.
- Several metaverse teams pivoted to AI products when funding dried up: Webaverse to Moemate/Upstreet AI agents, and Ambient to Braindump. That is relevant precedent for an AI-native world. AI was where the money went, not a fix for the retention problem.

### Gaps
- Total OpenSim 30-day actives for 2026: only Jan 2025 per-grid figures were retrieved. Hypergrid Business's 2026 monthly reports were not indexed.
- Hypergrid mechanics (avatars teleport between independently run grids, with home-grid identity and inventory served from the home grid) are from background knowledge. I did not verify them against an OpenSim wiki page this session.
- LSL specifics (event-driven state machines, per-script memory caps) were not verified this session.
- Neos 2026 status: no reliable source found.
- Ambient's funding amount and investors: not found.
- Webaverse's funding and the exact successor entity: not confirmed.
- Exact current Multisynq network or token status in 2026: no primary source. GitHub shows the client repo archived in Aug 2025 with no explanation found.
- I did not fetch the Croquet technical papers (TeaTime) or croquet.io docs directly; the croquet.io/origin and /multisynq pages returned 404.
- The Vircadia side of the Overte split: no primary Vircadia statement found.

## Why do OSS world projects fragment or die?

### Takeaway
They die when one corporate funder exits and the project has no independent revenue, no self-hosting community and no content base. They fragment over monetisation philosophy (crypto, commercial vs non-profit) and governance (BDFL vs democracy), not over technology.

### Cited Findings
- Funder exit was the stated cause in each case:
  - Mozilla: restructuring toward Firefox and AI, plus a "shift in the market for 3D virtual worlds" and user and customer bases "not robust enough" — [Hubs sunset](https://hubs.mozilla.com/labs/sunset); [TechCrunch](https://techcrunch.com/2024/02/13/mozilla-downsizes-as-it-refocuses-on-firefox-and-ai-read-the-memo)
  - Element stopped funding Third Room to cover its own costs — [Matrix.org](https://matrix.org/blog/2024/04/open-source-publicly-funded-service/)
  - Ambient's team moved to an AI product — [README](https://raw.githubusercontent.com/AmbientRun/Ambient/main/README.md)
  - The JanusVR company shut down — [Wikipedia](https://en.wikipedia.org/wiki/JanusVR)
- Wrong business model: High Fidelity's structure was "very hard to monetize", it clashed with the business model, and "should have been an OSS company" (Overte chair's view) — [Overte talk](https://ftp.sunet.se/pub/debian-meetings/2023/MiniDebConfLisboa/mini_dc-lisbon-03-Introduction_to_the_Overte_project.pdf)
- A codebase too big for volunteers: 400K LOC and more than 85k commits from about 70 paid staff, handed to volunteers — [same](https://ftp.sunet.se/pub/debian-meetings/2023/MiniDebConfLisboa/mini_dc-lisbon-03-Introduction_to_the_Overte_project.pdf)
- Splits over crypto and governance:
  - Neos to Resonite over crypto features — [Wikipedia mirror](https://backiee.wasmer.app/http_en_wikipedia_org/wiki/Resonite)
  - Vircadia to Overte over crypto, commercial ambitions, BDFL governance and a non-free codec — [Overte talk](https://ftp.sunet.se/pub/debian-meetings/2023/MiniDebConfLisboa/mini_dc-lisbon-03-Introduction_to_the_Overte_project.pdf)
- The orphaned foundations have no revenue: the Hubs Foundation roadmap lists revenue sources as "not started" — [roadmap](https://docs.google.com/document/d/1SNb3rFNIH6ATmzrDZrUg_s3jsYecnyWty__idK9A-lc/export?format=pdf). Its main repo had 19 commits in the past year — [GitHub](https://github.com/Hubs-Foundation/hubs)
- Users peak at launch and then decay. Resonite Steam concurrency peaked at about 1.2–1.3k at launch in Oct 2023 and now averages about 170–200 — [SteamDB](https://steamdb.info/app/2519830/charts); [Steambase](https://steambase.io/games/resonite/steam-charts)
- Standards bodies without shipped code go quiet. The LF Open Metaverse Foundation has had no news since 2023 — [openmv.org](https://openmv.org/)

### Inferences
- Root-cause taxonomy, in rough order of frequency in this sample:
  1. A single corporate funder with no exit plan for hosting and identity: Hubs, Third Room, JanusVR, High Fidelity
  2. A pivot to a hotter market, AI or crypto: Ambient, Webaverse, Croquet
  3. A governance or monetisation schism: Neos, Vircadia
  4. Codebase complexity beyond volunteer capacity: HiFi to Vircadia
  5. "Platform without content". Each new world started from an empty map, unlike SL's 20+ years of user content.
- GitHub stars do not predict survival. Ambient (3.9k) is dead and Overte (289) is thriving.
- An AI-native world could reduce the content-cold-start problem, which no project here solved. It cannot by itself solve the funding-exit problem.
- Forking is not death. Overte shows a fork with democratic non-profit governance can be more active than its commercial parent. Forks split small communities, though, and both halves can remain sub-scale.

### Gaps
- No user or MAU numbers were found for Hubs at shutdown, Overte, Vircadia, JanusXR or Third Room.
- I did not retrieve Hacker News postmortem threads.

## What survived longest and why (Second Life economy, OpenSim hypergrid)?

### Takeaway
Second Life (23 years) survives on a real-money creator economy with creator-owned IP, a persistent non-instanced world and a profitable operator. OpenSim (about 19 years) survives because it is cheap to self-host and federates through the hypergrid, so no single operator's death kills it, and because it reuses SL's mature viewer and content model.

### Cited Findings
- SL's cumulative $1.1B creator payout and Linden's 10% transaction take are company-reported — [GamesBeat](https://www.gamesbeat.com/games/linden-lab-has-spent-1-3b-building-second-life-and-paid-1-1b-to-creators/). Creators keep about 85% of sales, more than 13k creators cashed out in 2026, and IP stays with creators — [80.lv](https://80.lv/articles/second-life-wants-its-23-year-old-world-to-be-a-modern-game-platform)
- SL MAU is stable at about 600–700k, and about 11% of users script or create — [80.lv](https://80.lv/articles/second-life-wants-its-23-year-old-world-to-be-a-modern-game-platform)
- Scripting evolved without breaking content: SLua coexists with LSL in the same object, so the 20-year content base still runs — [80.lv](https://80.lv/articles/second-life-wants-its-23-year-old-world-to-be-a-modern-game-platform)
- OpenSim's scale and spread: in Jan 2025 there were about 300 active public grids, with the largest at about 8k monthly actives. The hypergrid holds about 99% of actives (undated) — [Hypergrid Business](https://hypergridbusiness.com/?p=78877); [Hypergrid Business](https://www.hypergridbusiness.com/?p=78433)
- OpenSim's core is still committed to, with 211 commits/52w on the [GitHub mirror](https://github.com/opensim/opensim)
- Overte has survived since 2022 as a volunteer e.V., with yearly assemblies, weekly in-world dev meetings, and Matrix/Discord channels — [Overte talk](https://ftp.sunet.se/pub/debian-meetings/2023/MiniDebConfLisboa/mini_dc-lisbon-03-Introduction_to_the_Overte_project.pdf). It shipped a release in Oct 2026 — [GitHub](https://github.com/overte-org/overte)

### Inferences
- Survival comes from three things together: (a) creators can earn or own something; (b) the world persists and content accumulates; (c) the running cost is borne by many parties (land renters, grid operators, self-hosters) rather than one sponsor.
- OpenSim's weakness mirrors its strength. It is federated and cheap, but fragmented into about 300 small grids, with no shared economy, discovery or identity standard beyond hypergrid hops. It also has no growth story beyond SL refugees and educators.

### Gaps
- Linden Lab's 2026 revenue and profitability figures were not found.
- SL's historical peak concurrency and user curve (e.g., the 2007–2009 peak) were not retrieved with a source this session.

## Concrete technical ideas worth reusing

### Takeaway
Seven ideas are worth reusing:
- Croquet's replicated deterministic computation, with a dumb timestamping relay
- SL's server-side, pausable, serialisable, migratable sandboxed scripts (now Luau)
- Resonite's single-data-model, live in-world multiplayer editing plus an open external protocol (ResoniteLink)
- Third Room's Wasm-sandboxed UGC scripts over a glTF/DOM-like scene API
- Ambient's ECS-as-database with WASM modules
- Overte's self-hostable domains with central identity only
- OpenSim's hypergrid federation

### Cited Findings
- Croquet/Multisynq: Synchronizers are stateless and only timestamp and relay events. No app computation runs server-side. Every client runs the same deterministic model — [Mirror](https://mirror.xyz/0x55cf7dffbcdD99952452A1a764239F4059b8a08F/KlLMTKhqb0YU8pxmhhLl7QrKZKV2no7P9ZkoqhrcxYk); [Wikipedia](https://en.wikipedia.org/wiki/Croquet_Project)
- SL's script runtime requirements are exactly the ones an AI-generated-behaviour system needs:
  - thousands of untrusted scripts per server
  - pause and resume
  - state serialisation
  - migration across servers and into inventory
  - resource limits and fair scheduling
  - no loadstring

  SL chose Luau over C# for sandboxing, memory and micro-threading — [80.lv](https://80.lv/articles/second-life-wants-its-23-year-old-world-to-be-a-modern-game-platform)
- Resonite: a single data model is the source of truth, eventually consistent and host-reconciled. ProtoFlux visual scripting is edited live in-world. ResoniteLink exposes the data model over WebSocket to external tools — [wiki](https://wiki.resonite.com/Data_model); [NuGet](https://www.nuget.org/packages/YellowDogMan.ResoniteLink)
- Third Room: Wasm sandboxes plus QuickJS for UGC, a glTF-based DOM-like API (WebSG), and world state in Matrix rooms (MSC3815) — [Matrix blog TP2](https://matrix.org/blog/2023/06/07/introducing-third-room-tp-2-the-creator-update/); [The New Stack](https://thenewstack.io/third-room-teases-user-generated-content-for-the-metaverse/)
- Ambient: WASM-isolated modules, an ECS real-time database with automatic multiplayer sync, and WebGPU — [README](https://raw.githubusercontent.com/AmbientRun/Ambient/main/README.md)
- Overte: JavaScript scripting everywhere (client, server, interface), automatically distributed physics, and self-hosted domains with a metaverse server for auth — [Overte talk](https://ftp.sunet.se/pub/debian-meetings/2023/MiniDebConfLisboa/mini_dc-lisbon-03-Introduction_to_the_Overte_project.pdf)
- KHR_interactivity has been submitted for Khronos ratification. It embeds behaviour graphs in glTF assets, with companion selectability, hoverability and visibility extensions. Babylon.js playback, Android XR (Jetpack XR) plans and glTF-Sample-Viewer support are reported — [Khronos](https://www.khronos.org/news/press/gltf-interactivity-extension-submitted-for-ratification). The [glTF-InteractivityGraph-AuthoringTool](https://github.com/KhronosGroup/glTF-InteractivityGraph-AuthoringTool) has 363 commits/52w.

### Inferences
- For "AI turns intent into sandboxed persistent behaviour", the closest proven pattern is SL's design brief plus a portable behaviour format: server-side Luau or Wasm, serialisable and migratable, with quotas. Generated behaviour could compile to a sandboxed script for runtime and to KHR_interactivity graphs for portability between engines.
- Croquet-style replication suits low-server-cost, deterministic, small-session collaboration. It is a poor fit for authoritative persistent worlds with untrusted clients or for large persistent state. Every client must run the whole model, and Croquet itself needed a hosted reflector network that it later tried to tokenise.
- Resonite's in-world editing is the best UX precedent for SAO-style "edit reality from inside". It is closed source, so only its concepts and its open ResoniteLink protocol are reusable.

### Gaps
- No independent benchmarks were found comparing Croquet determinism or snapshot sizes at scale.
- The internal ProtoFlux execution model is not documented in the sources retrieved.

## Which interop standards actually have traction in 2026?

### Takeaway
The standards with traction are glTF (with KHR extensions now adding interactivity), OpenUSD (Core Spec 1.0 in Dec 2025, industrial and DCC backing), VRM (avatars, strongest in Japan, moving into Khronos), and WebXR via three.js, Babylon.js and A-Frame. Umbrella "metaverse" bodies have little traction. The LF Open Metaverse Foundation looks dormant and OMI is low-activity. The MSF only shipped its first code (the Sneeze browser engine) in June 2026.

### Cited Findings
- glTF: [KhronosGroup/glTF](https://github.com/KhronosGroup/glTF) has 7,900 stars and was last pushed 2026-11-20. KHR_interactivity is in ratification with engine adoption — [Khronos](https://www.khronos.org/news/press/gltf-interactivity-extension-submitted-for-ratification)
- OpenUSD:
  - Core Spec 1.0 was announced 17 Dec 2025. It covers grammar, data types, composition, file formats and compliance, and excludes geometry, materials and physics — [LF press](https://www.linuxfoundation.org/press/alliance-for-openusd-announces-core-specification-1.0-the-universal-language-for-building-3d-worlds); [CG Channel](https://www.cgchannel.com/2025/12/aousd-releases-the-first-openusd-core-specification/)
  - Core Spec 1.1 is planned for 2026 (splines, variable and path expressions)
  - A Characters, Motion & Interactivity interest group was formed in Mar 2026 — [AOUSD](https://aousd.org/uncategorized/foundations-of-open-3d-development-introducing-aousd-core-specification-1-0/); [LF Mar 2026](https://www.linuxfoundation.org/press/aousd_prmarch2026)
  - [OpenUSD repo](https://github.com/PixarAnimationStudios/OpenUSD): 7,667 stars, 1,478 commits/52w, v26.11.1 on 2026-10-21
- VRM: 1.0 was released Sept 2022. Khronos and the VRM Consortium are collaborating (Oct 2024) toward glTF-ratified extensions and ISO/IEC recognition. It is the de facto avatar standard in Japan; VRChat uses its own format — [Khronos](https://www.khronos.org/news/permalink/the-khronos-group-and-vrm-consortium-collaborate-to-advance-international-standardization-of-the-vrm-3d-avatar-file-format); [Library of Congress](https://loc.gov/preservation/digital/formats/fdd/fdd000564.shtml); [VIVERSE](https://news.viverse.com/post/vrm-models-explained). [vrm-c/UniVRM](https://github.com/vrm-c/UniVRM) had 205 commits/52w and v0.131.4 on 2026-10-19. A 3GPP Rel-20 document (Sept 2026) contains a "VRM Representation Format" section — [3GPP 26.813](https://www.3gpp.org/ftp/specs/PDF/2026-09/Rel-20/26_series/26813-k00.pdf)
- MSF/OMBI: Sneeze (Apache-2.0) was released 15 Jun 2026 and RP1's Artemis browser on 6 Jul 2026 — [MSF](https://metaverse-standards.org/news/press-releases/metaverse-standards-forum-introduces-sneeze-the-worlds-first-open-metaverse-browser-engine/); [BusinessWire](https://secure.businesswire.com/news/home/20260706640660/en/RP1-Launches-Artemis-the-Worlds-First-Native-Metaverse-Browser)
- LF Open Metaverse Foundation: no news since 2023 — [openmv.org](https://openmv.org/)
- OMI: low commit activity — [omigroup/omi](https://github.com/omigroup/omi) (0 commits/52w); [gltf-extensions](https://github.com/omigroup/gltf-extensions)

### Inferences
- Standards with traction came from asset pipelines that industry already needed (glTF for real-time delivery, USD for DCC and digital twins, VRM for VTubers). Top-down "metaverse interoperability" bodies did not produce them.
- A new world should be glTF/VRM-native at runtime, import OpenUSD, and target KHR_interactivity for portable behaviour. It should not wait on any umbrella metaverse standard.
- Sneeze/OMBI is too new (5 months) to judge. It is tied to one commercial contributor (RP1), which is the same single-funder risk pattern seen above.

### Gaps
- KHR_interactivity final ratification date: not confirmed as of Nov 2026.
- Whether AOUSD Core Spec 1.1 shipped in 2026: not confirmed.
- Sneeze repo stars and contributors: the repo path guess `metaverse-standards-forum/sneeze` returned 404, and the actual GitHub org was not located.
- Hubs, Overte and Resonite actual usage of VRM or KHR_interactivity was not verified.
