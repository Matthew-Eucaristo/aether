# Why the successful UGC / sandbox / social worlds succeeded (case files, as of Nov 2026)

Scope note: research run 2026-11-24 with ~27 search/fetch calls. Primary sources used where reachable (Roblox SEC 8-K Q3 2026 shareholder letter, Roblox newsroom, luau.org, Epic tech blog/Verse docs, Mojang NFT post, Microsoft Learn). Many player-count figures for Minecraft, Gorilla Tag, and Garry's Mod come from press or aggregators; flagged where so. Items I could not source are in Gaps, not in Findings.

## Q1. What are the common success factors? ("a game with a reason to come back, not a metaverse"; low-end reach; creator economy; scripting power; community servers)

### Takeaway
Every winner started as a concrete, fun activity (build/survive, tag, hang out with friends, battle royale) and grew a creation layer on top of an existing audience. None launched as "a metaverse." The winners also ran on whatever hardware their audience already owned, and they paid or empowered creators before they had polish. The 2026 shakeout shows how tight the margin is: Rec Room shut down and Horizon Worlds left VR, while VRChat (social-first, avatar-culture-first, about half flatscreen) set records.

### Cited Findings

**Roblox: scale, economy, and the 2026 reversal**
- FY2025: revenue $4.9B (+36%), bookings $6.8B (+55%), operating cash flow $1.8B. Q4 2025 average DAU was 144M (+69% YoY), with 35B hours (+88%). Original 2026 guidance was bookings of $8.28B–$8.55B (+22–26%), and management said it would move to quarterly guidance because viral hits are hard to predict — [Q4 2025 Shareholder Letter](https://s27.q4cdn.com/984876518/files/doc_financials/2025/q4/Q4-2025-Shareholder-Letter.pdf); [GamesBeat Q4 2025](https://gamesbeat.com/?p=316195); [Motley Fool transcript](https://www.fool.com/earnings/call-transcripts/2026/02/05/roblox-rblx-q4-2025-earnings-call-transcript/)
- Q3 2026 (reported 29 Oct 2026): DAU 138M (-9% YoY, +12% QoQ), hours ~34B (-15% YoY), revenue $1,572M (+16%), bookings $1,697M (-12% YoY), DevEx fees $403M (-6% YoY; 26% of revenue, 24% of bookings), net loss $230M. Over two years, DAU was +55% and hours +63%. Roblox attributes the decline to "difficult comparisons to the growth we experienced in 2025" — [Roblox Q3 2026 shareholder letter, SEC 8-K ex99.1](https://www.sec.gov/Archives/edgar/data/0001315098/000162828026068235/ex991-q32026earningsshar.htm)
- Q4 2026 guidance: bookings $1.89B–$2.00B (-10% to -15% YoY), adjusted EBITDA negative ($-70M to $-10M), FCF negative ($-190M to $-105M) — [same 8-K](https://www.sec.gov/Archives/edgar/data/0001315098/000162828026068235/ex991-q32026earningsshar.htm)
- Monthly unique payers were 30M (-16% YoY) and bookings per payer rose 5%. Growth markets: India DAU +37%, Japan DAU +33% YoY — [GamesBeat Q3 2026](https://gamesbeat.com/roblox-grows-revenues-in-q3-by-16-but-stock-falls-in-after-hours-trading/)
- Baszucki says Roblox has about 20% of US under-18 players and about 20% of US under-18 gaming revenue, and is aiming for 10% of the global gaming market and for adults (18+) — [GamesBeat Q3 2026](https://gamesbeat.com/roblox-grows-revenues-in-q3-by-16-but-stock-falls-in-after-hours-trading/)
- Creator economy: about $1.5B in DevEx payouts in 2025, up from $922M in 2024. Q4 2025 DevEx was $477M (+70% YoY). Roblox reports more than $5B lifetime since DevEx launched in 2013. One later report says 42,000+ creators participated with a median payout of about $1,500, and the top 1,000 averaged about $1.1M–$1.3M. These come via secondary coverage, and the periods are inconsistent — [Roblox Economic Impact Report (Sep 2025)](https://about.roblox.com/newsroom/2025/09/roblox-annual-economic-impact-report); [Roblox newsroom Dec 2025](https://corp.roblox.com/newsroom/2025/12/how-roblox-impacts-the-global-economy); [IBTimes on top creators](https://www.ibtimes.co.uk/roblox-top-creators-devex-payouts-2026-1819403)
- Low-end reach is designed in. Roblox docs say Android is about 65% of a typical game's player base, and the Indonesian docs say about 60% of those players have 2–4 GB RAM. Roblox pushes instance streaming, splitting places, and built-in materials to save memory. It added an analytics insight that fires when 5% or more of a game's concurrent users are on Android devices under 2 GB and crash rates there are worse than average — [Roblox: Test on hardware](https://create.roblox.com/docs/performance-optimization/test-on-hardware); [Design for performance](https://create.roblox.com/docs/performance-optimization/design); [DevForum analytics post](https://devforum.roblox.com/t/analytics-optimize-low-end-android-crash-rates-server-memory-and-cpu-performance/3145364). Counterpoint: forum developers report crashes on 3 GB phones — [same thread](https://devforum.roblox.com/t/analytics-optimize-low-end-android-crash-rates-server-memory-and-cpu-performance/3145364/18)
- AI-native creation is now core:
  - Studio Assistant launched Dec 2023. More than 80% of the top 100 games by hours used AI tools in the past month, and AI-built game volume grew more than 50% QoQ. In a survey, 97% of regular AI users said it sped up development.
  - The prompt-to-game tool "Build" has had 500K+ users and 70K+ published games since its alpha (New Zealand, Singapore, Serbia, Thailand) — [Q3 2026 letter](https://www.sec.gov/Archives/edgar/data/0001315098/000162828026068235/ex991-q32026earningsshar.htm)
  - Daily MCP Server users grew about 5x QoQ, and two near-fully-AI-made games reached the top 20 by concurrents — [GamesBeat Q3 2026](https://gamesbeat.com/roblox-grows-revenues-in-q3-by-16-but-stock-falls-in-after-hours-trading/)
  - The open-source Cube 3D model (GitHub and Hugging Face, Mar 2025) had generated 1.8M+ objects. "4D generation" beta (Feb 2026): players type a prompt and get a *functional* object, such as a drivable car, built from "schemas" (rule sets that split an object into parts and attach behaviors; e.g. Car-5 = four independently rotating wheels). One game saw 160K+ objects generated in early access, and players who used it had 64% longer sessions — [Roblox newsroom Feb 2026](https://about.roblox.com/newsroom/2026/02/accelerating-creation-powered-roblox-cube-foundation-model)
- "Roblox Everywhere" (RDC 2026): in H1 2027 Roblox will publish select top titles as standalone app-store apps. Later, any creator will be able to export a standalone binary — [GamesBeat Q3 2026](https://gamesbeat.com/roblox-grows-revenues-in-q3-by-16-but-stock-falls-in-after-hours-trading/)

**Minecraft**
- 350M+ copies sold, confirmed by Mojang/Guinness as of ~April 2025 (300M was Oct 2023) — [VGChartz](https://www.vgchartz.com/article/466157/minecraft-sales-top-350-million-units/); [Gamereactor](https://www.gamereactor.eu/minecraft-sells-350-million-copies-1627043/). A later VGChartz headline cites 425M — [VGChartz](https://www.vgchartz.com/article/469182/minecraft-sales-top-425-million-units/). I did not open or verify it, so treat it as unconfirmed.
- MAU of about 200M+ (2025 average ~212M, peak ~222.5M June 2025) comes only from aggregators (DemandSage-type sites), not Mojang. Low confidence — [DemandSage](https://demandsage.com/minecraft-statistics/); [HostHavoc](https://hosthavoc.com/en-GB/blog/minecraft-statistics)
- Community servers were the multiplayer layer, and Mojang tolerated them, then governed them. Bukkit was a community server API. In June 2014 Mojang began enforcing the Java EULA against pay-to-win server sales, using "brand enforcers," emails, and a blacklist. This contributed to Bukkit's lead announcing its end (Aug 24, 2014), and a contributor (Wesley Wolfe) filed a DMCA takedown (Sep 3, 2014) over GPL and Mojang code licensing. Mojang said it was not responsible for the takedown. Spigot survived by distributing BuildTools (patch-at-build) instead of binaries — [PC Gamer](https://pcgamer.com/minecraft-bukkit-team-lead-tries-to-end-development-but-mojang-steps-in); [Slashdot](https://games.slashdot.org/story/14/09/06/1323225/dmca-claim-over-gpl-non-compliance-shuts-off-minecraft-plug-ins); [Kotaku](https://kotaku.com/the-uncertain-future-of-minecrafts-independent-servers-1790360981). Most detail is from community sources, so medium confidence.
- NFT ban (July 20, 2022): Mojang said NFTs "create models of scarcity and exclusion that conflict with our Guidelines and the spirit of Minecraft." It banned blockchain/NFT integration in the client and server applications. Server hosts may charge for access, but "all players should have access to the same functionality." The NFT Worlds token and NFTs fell more than 70% on the news — [Mojang: Minecraft and NFTs](https://minecraft.net/en-us/article/minecraft-and-nfts); [Decrypt](https://decrypt.co/105575/microsoft-minecraft-ban-nfts-game-servers-derivative-projects?amp=1); [GamesRadar](https://www.gamesradar.com/leading-minecraft-nft-project-tanks-after-mojang-bans-the-blockchain)
- August 2023 EULA/Usage Guidelines update added further rules on creator promotion and paywalling — [Tubefilter](https://www.tubefilter.com/2023/08/09/minecraft-eula-update-new-creator-rules-nfts-promotion/)

**Fortnite Creative / UEFN**
- UEFN creator payouts passed $1B cumulative (Unreal Fest, June 2026). Earlier checkpoints were $320M within one year of launch and $722M across ~260K islands by Nov 2025. Creators receive 40% of island revenue (net revenue pool). About 3,000 islands earn more from in-island transactions (enabled Dec 2025) than from engagement payouts. Epic cites 75M+ MAU — [Tubefilter](https://www.tubefilter.com/2026/06/17/epic-games-unreal-editor-for-fortnite-creator-payouts/); [Outlook Respawn](https://respawn.outlookindia.com/gaming/gaming-news/epic-games-expands-uefn-amid-722m-creator-boom); [Fortnite news, Sep 2025](https://www.fortnite.com/news/fortnite-developers-will-soon-be-able-to-sell-in-game-items?lang=en)
- Naavik estimates about $370M (range $350–390M) in 2025 engagement payouts. Statista puts 2024 at $352M — [tech-insider citing Naavik](https://tech-insider.org/fortnite-creator-economy-1-billion-2026/); [Statista](https://statista.com/statistics/1602861/fortnite-ugc-user-engagement). Third-party estimates.
- Coverage links Epic layoffs to "spending more money than we're making" — [FRVR blog](https://frvr.com/blog/epic-games-reveals-uefn-creators-have-been-paid-over-1-billion-as-fortnite-layoffs-were-blamed-on-spending-more-money-than-were-making/) (secondary)
- Comparison: Roblox paid more to creators in 2025 alone (~$1B+) than UEFN did in three years — [Tubefilter](https://www.tubefilter.com/2026/06/17/epic-games-unreal-editor-for-fortnite-creator-payouts/)

**VRChat vs Rec Room vs Horizon Worlds (the 2026 social-VR shakeout)**
- March 2026: Meta pivoted Horizon Worlds fully to flatscreen (mobile/web), with VR in "maintenance mode." Days later Rec Room announced a full shutdown on June 1, 2026. It said it "never reached sustained profitability" and cited the VR market shift and broader gaming headwinds — [UploadVR](https://www.uploadvr.com/vrchat-statement-after-rec-room-and-horizon-worlds-fold/)
- VRChat records: 148,886 concurrent users on NYE 2025–26 (typical weekend peak ~120–125K). In Feb 2026 a Japanese concert (Kaguya) drew 156,716, later corrected to 158,192. VRChat says its creator economy, avatar marketplace, and first-party stores are growing, and that usage is roughly as much flatscreen as VR — [UploadVR NYE](https://www.uploadvr.com/vrchat-nye-user-record/); [Road to VR](https://roadtovr.com/vrchat-all-time-high-japanese-concert/); [UploadVR statement](https://www.uploadvr.com/vrchat-statement-after-rec-room-and-horizon-worlds-fold/)
- Steam alone peaked at 78,613 on 2026-02-15, which excludes Quest and other platforms — [Steambase](https://steambase.io/games/vrchat)
- Growth is heavily Japanese — [Road to VR key stats](https://roadtovr.com/vrchat-key-stats-japan-growth-concurrents/)
- A VRChat world won an Emmy in Sep 2026 (*Uncanny Alley: A New Day*) — [UploadVR related headlines](https://www.uploadvr.com/vrchat-statement-after-rec-room-and-horizon-worlds-fold/)

**Gorilla Tag (VR-native hit)**
- $100M+ revenue (AWE, mid-2024), mostly cosmetics. 1M+ DAU, 3M+ MAU, 10M+ lifetime players (12M+ by Nov 2024). No newer official figures. A mobile spin-off, "Monke Mayhem," is announced for 2027 — [Road to VR](https://roadtovr.com/gorilla-tag-revenue-vr-success-another-axiom/); [GamesBeat](https://gamesbeat.com/gorilla-tag-crosses-10m-vr-players-and-100m-in-revenue/); [Meta dev blog](https://developers.meta.com/horizon/blog/another-axiom-orion-drift-gorilla-tag-VR/); [Road to VR mobile](https://roadtovr.com/gorilla-tag-mobile-game-announce-2027/)

**Garry's Mod, Dreams**
- Garry's Mod sold 5,729 copies on day one, 1M by July 2011, 20M+ by Sep 2021, and a reported ~25.5M later. Garry Newman said he originally didn't want to sell it because "no one would pay for it." Facepunch is building s&box as its successor — [NME](https://www.nme.com/news/physics-sandbox-garrys-mod-hits-20million-sales-landmark-3040652); [GamesRadar](https://gamesradar.com/platforms/pc-gaming/garrys-mod-dev-says-he-didnt-want-to-sell-it-originally-because-no-one-would-pay-for-it-25-million-copies-later-he-admits-he-was-wrong)
- Dreams (PS4, Media Molecule) ended live support on Sep 1, 2023. Engadget wrote it "wasn't quite as successful" as hoped. It was a single-console creation suite with no PC/mobile reach and no creator payouts — [Gematsu](https://www.gematsu.com/2023/04/dreams-to-end-live-support-on-september-1-as-media-molecule-shifts-focus-to-exciting-new-project); [Engadget](https://www.engadget.com/sony-will-stop-updating-dreams-later-this-year-143055503.html?src=rss)

**Club Penguin / Habbo (social-space lifespan)**
- Club Penguin had 200M+ registered accounts by July 2013. Disney announced closure Jan 30, 2017, and servers shut Mar 29–30, 2017. Commentary cites declining revenue, 200+ moderators and developers on payroll, and the audience moving from desktop to mobile. The successor Club Penguin Island closed the next year. A fan server (Club Penguin Rewritten) launched Feb 12, 2017, after the announcement — [Wikipedia](https://en.wikipedia.org/wiki/Club_Penguin); [TechCrunch](https://techcrunch.com/2017/01/31/club-penguin-is-shutting-down/?ncid=rss); [FourWeekMBA (commentary)](https://fourweekmba.com/why-did-club-penguin-shut-down)
- Habbo (Sulake, ~15M monthly users in 100+ countries): the 12 June 2012 Channel 4 investigation found sexual chat with apparent minors was common and rarely moderated. Sulake muted the entire site ("The Great Mute," 13 June 2012) and restored chat in four hotels on 28 June. Tesco, WHSmith, and GAME pulled Habbo cards, and investor Balderton returned its 13% stake at zero value — [Channel 4](https://www.channel4.com/news/what-is-happening-in-habbo-hotel); [TNW](https://thenextweb.com/news/amidst-child-sex-scandal-habbo-mutes-its-entire-site-while-it-investigates-its-users-behavior); [HuffPost UK](https://huffingtonpost.co.uk/amp/entry/habbo-hotel-backlash-tesco-whsmith-game_n_1593630); [Game Developer](https://gamedeveloper.com/business/-i-habbo-i-investors-balk-amid-child-safety-concerns---report)

### Inferences
- **"Game first, platform second" holds in every case.** Each started as something with a core loop:
  - Minecraft: survival/building
  - Roblox: kids playing games with friends, with a hit-driven catalog
  - Fortnite: a battle royale with an existing 75M+ MAU audience, before UEFN
  - Gorilla Tag: tag
  - Garry's Mod: a Half-Life 2 physics toy
  - VRChat: hanging out as an avatar

  Rec Room, Horizon Worlds, and Dreams were creation platforms first, without a breakout core loop. Rec Room and Horizon VR died and Dreams was sunset. For the founder, the first shipped thing should be a specific, repeatable fun activity. The AI-intent engine should be how users extend it, not the pitch.
- **Reach beats fidelity.** Roblox optimizes for 2–4 GB Android phones. Minecraft's blocky look is cheap to render and easy for children to author in. Gorilla Tag uses low-poly, Quest-native visuals. The two biggest platforms (Roblox ~65% Android; Minecraft on every device) treat low-end support as core strategy. Dreams (one console) and Rec Room/Horizon (VR-centric) show the cost of a narrow device base. Even VRChat, the VR survivor, is about half flatscreen. An interface-agnostic design is right, but the binding constraint is the cheapest phone, not the headset.
- **Low fidelity is a creation advantage, not just a performance one.** Voxels and simple primitives let non-artists produce acceptable content. That is why AI generation (Cube 4D "schemas") is being layered onto constrained, rule-based object models instead of free-form meshes.
- **Creator money compounds.** Roblox (~$1.5B in 2025) and UEFN ($1B over 3 years) both pay creators, and Roblox's hit-driven DAU swings (+69% then -9%) show that the platform's fortunes ride on creator hits. Minecraft deliberately never built a big central creator payout for servers. It let the server economy self-fund through access fees and cosmetics within guidelines, and banned scarcity mechanics (pay-to-win, NFTs).
- **The newest signal is that AI creation is now a measurable growth lever on the leading UGC platform.** Roblox's 80% of top games using AI, Build's 70K games, 4D generation, and the MCP server match the founder's thesis. The founder's differentiation therefore cannot be "AI makes things"; Roblox already ships that.
- **Social spaces without a game loop age out with their cohort.** Club Penguin's 200M registered accounts did not prevent closure once costs (moderation headcount) exceeded revenue and its audience moved platforms. Fan-run private servers outlived it, which argues for open-source and self-hostability as a lifespan hedge.

### Gaps
- No official current Minecraft MAU from Mojang/Microsoft. The 425M-copies figure was not opened or verified.
- No fetched primary data on Bedrock's device list or cross-play share, Minecraft Marketplace revenue, Hypixel concurrents, or Paper/Fabric/Forge adoption. Searches returned nothing usable.
- VRChat funding, revenue, profitability, layoffs, and iOS/Android phone launch timing could not be verified this session. I have an unverified recollection of a 2024 layoff of about 30% of staff and a 2021 Series D; do not use without a source.
- Animal Crossing: New Horizons sales and its role as a pandemic social space were not researched (budget).
- Neopets history was not found.
- Rec Room's peak user and funding numbers were not retrieved.
- Gorilla Tag figures are from mid/late 2024, with no 2025–26 data.
- Fortnite's share of playtime spent in creator islands vs Epic-made modes was not found.
- The size and current state of Roblox's 2025 DevEx rate change was not verified (a PocketGamer.biz headline suggests an increase).

## Q2. How did they balance creative freedom against safety, griefing, and moderation (permissions, sandboxing, server ownership)?

### Takeaway
There are two working models.

- **Minecraft: decentralize and govern by contract.** Anyone hosts a server with operator permission levels, Mojang sets only monetization/brand rules (EULA, no pay-to-win, no NFTs), and server owners own moderation.
- **Roblox (and later Fortnite): centralize the platform.** One company hosts everything and must moderate everything, which works until regulators and plaintiffs arrive.

In 2026 Roblox paid heavily for that centralization: about $54M in state settlements, 7+ AG suits, a federal MDL, mandatory age checks, and banded chat, alongside its first YoY DAU decline.

### Cited Findings
- **Roblox litigation timeline:**
  - Louisiana AG suit, Aug 2025: no effective age verification or parental consent at signup.
  - Kentucky AG suit, Oct 2025: "playground for predators" — [Engadget](https://www.engadget.com/gaming/louisiana-attorney-general-has-filed-a-child-protection-lawsuit-against-roblox-182354250.html); [PC Gamer](https://www.pcgamer.com/games/kentucky-sues-roblox-calling-it-a-playground-for-predators-who-seek-to-harm-our-children)
  - Federal MDL-3166 (*In re: Roblox Corporation Child Sexual Exploitation and Assault Litigation*), created Dec 2025 in N.D. Cal.; about 162 cases by June 2026 — [tech-insider tracker](https://tech-insider.org/roblox-lawsuit-2026/) (secondary)
  - By April 2026: 7 state AG suits (TX, FL, LA, IA, NE, KY, TN), an LA County suit (Feb 2026), about 80 family suits in California, and Australian eSafety transparency notices (fines up to A$825K per day) — [Malwarebytes](https://www.malwarebytes.com/blog/news/2026/04/roblox-clamps-down-on-chats-and-age-checks-as-legal-pressure-builds)
- **Settlements:**
  - Nevada ~$12M (Apr 2026)
  - Alabama $12.2M and West Virginia $11M (Apr 21, 2026)
  - South Dakota (Jul 13, 2026)
  - Total about $54M per one tracker. Ohio sought to lead a class action (Aug 2026) — [Claims Journal](https://www.claimsjournal.com/news/national/2026/04/22/337051.htm); [Courthouse News](https://www.courthousenews.com/roblox-agrees-to-12-2-million-child-safety-settlement-in-alabama/); [tech-insider](https://tech-insider.org/roblox-lawsuit-2026/)
- **Roblox safety architecture (2026):**
  - No chat without age verification (Jan 2026).
  - Age checks for everyone by facial estimation or government ID (from May 1, 2026, under the Alabama deal), plus behavior monitoring for age liars.
  - Adults and under-16s can chat only as "trusted friends" (added by QR code or contact import).
  - Minor communications cannot be end-to-end encrypted.
  - Account tiers from June 2026: Roblox Kids (5–8, no chat, "minimal/mild" games only), Roblox Select (9–15), and standard (16+). Unverified users are treated like Kids.
  - A Feb 2026 report found the Persona age-check vendor frontend suggested watchlist facial recognition — [Malwarebytes](https://www.malwarebytes.com/blog/news/2026/04/roblox-clamps-down-on-chats-and-age-checks-as-legal-pressure-builds)
- **Roblox Q3 2026 safety metrics:**
  - 64% of global DAU had completed an age check, including more than 80% of DAU aged 13–17.
  - Communications are banded by age cohort, with image-sharing restrictions.
  - RM3 is a real-time multimodal moderation system, and BAM scans assembled avatars.
  - An LLM chat filter *rewrites* messages into age-appropriate versions.
  - "Certain Infrastructure and Trust & Safety" expense was $258M (+24% YoY, 16% of revenue), driven by AI training and inference — [Q3 2026 letter](https://www.sec.gov/Archives/edgar/data/0001315098/000162828026068235/ex991-q32026earningsshar.htm)
- The company attributes the YoY DAU decline to tough comparisons, not safety changes — [Q3 2026 letter](https://www.sec.gov/Archives/edgar/data/0001315098/000162828026068235/ex991-q32026earningsshar.htm). Independent attribution was not found.
- **Minecraft permission model:**
  - Bedrock "Operator Commands" gives permission level 3 in singleplayer/LAN but level 1 on a dedicated server.
  - Command blocks run at level 2, so they cannot run level-3 admin commands such as /setmaxplayers.
  - Microsoft's docs separate "GameDirectors" commands (runnable by command blocks and scripts) from "Admin" commands (not runnable from automation).
  - Most commands require cheats or operator status.
  - A Bedrock function cannot run more than 10,000 commands per call — [Microsoft Learn: Commands](https://learn.microsoft.com/en-us/minecraft/creator/commands/commands); [Microsoft Learn: Functions](https://learn.microsoft.com/en-us/minecraft/creator/documents/functionsintroduction); [Minecraft Wiki: Permission level (mirror)](https://wiki.ronlab.site/content/minecraftwiki_en_all_maxi_2025-11/OP_level)
- Java bug reports show function permission-level settings being ignored for macros and for levels 3–4, so the permission model has real edge cases — [MC-266136](https://bugs-legacy.mojang.com/browse/MC-266136); [MC-197804](https://bugs.mojang.com/browse/MC-197804)
- Minecraft's governance of servers focuses on economics, not content. The EULA (enforced since June 2014) bans pay-to-win. The 2022 policy says "all players should have access to the same functionality" and bans NFTs — [Mojang NFT post](https://minecraft.net/en-us/article/minecraft-and-nfts); [PC Gamer 2014](https://pcgamer.com/minecraft-bukkit-team-lead-tries-to-end-development-but-mojang-steps-in)
- VRChat Quest/Android avatar limits (e.g. lights disabled) are framed as serving performance, safety, and discouraging malicious behavior — [VRChat Quest content limitations](https://creators.vrchat.com/platforms/android/quest-content-limitations/)
- Habbo 2012 is the canonical failure: under-moderated child chat led to a whole-site mute, retailers pulling cards, and an investor writing its stake to zero — [TNW](https://thenextweb.com/news/amidst-child-sex-scandal-habbo-mutes-its-entire-site-while-it-investigates-its-users-behavior); [Game Developer](https://gamedeveloper.com/business/-i-habbo-i-investors-balk-amid-child-safety-concerns---report)

### Inferences
- **Moderation is mostly about communication between strangers, especially adults and minors, not about scripts.** Every major child-safety crisis (Habbo 2012, Roblox 2025–26) centered on chat and contact. The fixes converge on age assurance, cohort-banded communication, trusted-friend graphs, and AI rewriting or filtering of messages.
- **For an open-source, interface-agnostic world:**
  - Design age bands and contact rules into the protocol from day one.
  - Default to the Minecraft model, where self-hosted server owners own moderation of their instance.
  - Avoid becoming a single global host of child chat without Roblox-scale trust-and-safety spend (~$258M/quarter, including infrastructure).
- **Minecraft shows a mechanism for limiting what automation can do.** Its graded permission levels (player < command block 2 < op 3 < console 4) are a ready template for an AI agent that turns intent into behavior: the agent should run at a bounded permission level like a command block, never at admin level. The "GameDirectors vs Admin" split in Microsoft's docs is the clearest existing statement of that boundary.
- **Fork-ability protects the commons but creates legal risk.** Bukkit's 2014 meltdown came from unclear licensing (GPL community code mixed with proprietary Mojang code). An open-source project should settle licensing, CLA, and trademark policy before the plugin ecosystem forms.
- **Ban scarcity economics up front.** Mojang's NFT and pay-to-win bans protected the "spirit" of a creative sandbox and blocked speculative capture. A founder can state this as policy early.

### Gaps
- No independent evidence (outside Roblox's own letter) on whether age checks or chat restrictions caused part of the 2026 DAU and bookings drop.
- Outcomes of the Texas and Florida suits and of MDL-3166 as of Nov 2026 were not found.
- Fortnite/UEFN moderation and its island review and rating process were not researched.
- VRChat's trust system and moderation approach were not researched beyond avatar limits.
- Minecraft Realms vs third-party server moderation tooling (e.g. chat reporting added in 1.19.1) was not sourced this session.

## Q3. How do their scripting systems (command blocks/datapacks, Luau, Udon, Verse) trade off power and safety?

### Takeaway
All four restrict what *code may touch* rather than trusting authors:

- **Command blocks/datapacks:** a declarative, permission-levelled command language with no host access, capped per call.
- **Luau:** a stripped, fuzzed Lua VM with read-only globals, per-script environments, and watchdog interrupts.
- **Udon:** a custom VM with no file, OS, or threading access.
- **Verse:** goes furthest, with effect-typed functions and transactional rollback so that failed logic leaves no trace.

The trend is toward languages where safety is a property of the type and effect system, not just the sandbox. That trend fits an AI agent that compiles intent into behavior.

### Cited Findings
- **Luau sandbox (luau.org):**
  - The threat model assumes untrusted, possibly malicious code.
  - Removed: `io`, `package`, `dofile`, `loadfile`, `string.dump`, `load`. `os` is reduced to clock/date/time, and `debug` to traceback/info.
  - `loadstring` rejects bytecode.
  - Builtins, the string metatable, and globals are read-only.
  - Each script gets its own global table.
  - The host installs an interrupt handler called at function calls and loop iterations; Roblox's watchdog limits Studio scripts to 10s.
  - No default memory limit.
  - Caveats: `getfenv`/`setfenv` can inject globals, same-VM isolation is not guaranteed (separate VMs are advised for trusted vs untrusted code), the C++ stack is not formally proven, and a long-running C function can evade interrupts — [Luau Sandbox](https://luau.org/sandbox)
- Luau has been open-source (MIT) since Nov 2021, including the compiler, type checker, and linter. It uses gradual typing because large codebases suffered without types, and it is used outside Roblox (Alan Wake 2, Warframe) — [Luau goes open-source](https://luau.org/2021/11/03/luau-goes-open-source.html); [Wikipedia](https://en.wikipedia.org/wiki/Luau_(programming_language)); [Luau performance](https://luau.org/performance)
- Verse runs all code inside transactions. Failure contexts (e.g. if-conditions) are nested transactions that commit on success and roll back on failure, and functions called in them must carry the `<transacts>` effect, which the compiler enforces — [Verse: Failure](https://dev.epicgames.com/documentation/en-us/uefn/failure-in-verse); [Speculative execution](https://dev.epicgames.com/documentation/fortnite/speculative-execution)
- To make engine C++ calls rollback-safe, Epic built AutoRTFM, a Clang-based compiler that automatically inserts undo logging for every store inside a transaction scope. It shipped on some Fortnite servers from v28.10 and is single-threaded (not full STM). As of March 2024, Verse runtime errors did not roll back correctly, and a `<no_rollback>` effect blocked many APIs from failure contexts as a stopgap — [Epic tech blog: Bringing Verse Transactional Memory Semantics to C++](https://www.unrealengine.com/tech-blog/bringing-verse-transactional-memory-semantics-to-c)
- A community bug report says transactional ops on arrays of functions were not rolled back — [Unreal forums](https://forums.unrealengine.com/t/critical-verse-transactional-operations-on-arrays-of-functions-are-not-rolled-back-when-the-enclosing-failure-context-is-rolled-back/1640382) (current status unknown)
- **Udon and UdonSharp:** Udon's VM acts as a sandbox preventing dangerous instructions, so VRChat can safely run world code. UdonSharp compiles C# to Udon assembly, with no file-system access, no pointers, no multithreading, and (historically) no generic `List<T>` — [VRChat wiki: Udon](https://wiki.vrchat.com/wiki/Udon); [VRChat creators: UdonSharp](https://creators.vrchat.com/worlds/udon/udonsharp/); [uhiyama-lab guide](https://uhiyama-lab.com/en/notes/vrchat/udonsharp-csharp-features/)
- Feature requests show that avatars cannot run custom scripts, and that worlds have limited control over avatar safety — [VRChat feedback](https://feedback.vrchat.com/feature-requests/p/world-controlled-avatar-safety-policy-and-udon-support)
- **Minecraft command blocks/functions:** command blocks run at permission level 2, and Bedrock functions are capped at 10,000 commands per call. A function containing an unparseable command will not load — [Microsoft Learn: Functions](https://learn.microsoft.com/en-us/minecraft/creator/documents/functionsintroduction); [Microsoft Learn: Commands](https://learn.microsoft.com/en-us/minecraft/creator/commands/commands)
- Java server plugins (Bukkit/Spigot) by contrast run arbitrary JVM code with full host access; the Bukkit history shows these ran outside Mojang's control — [Slashdot](https://games.slashdot.org/story/14/09/06/1323225/dmca-claim-over-gpl-non-compliance-shuts-off-minecraft-plug-ins)
- Roblox's newest layer above code is "schemas" in 4D generation: AI fills a rule set (parts plus behaviors) rather than writing arbitrary code — [Roblox newsroom](https://about.roblox.com/newsroom/2026/02/accelerating-creation-powered-roblox-cube-foundation-model)

### Inferences
- **A spectrum of power vs safety:**
  - Command blocks/datapacks: safest, least expressive. Declarative, capped, permission-levelled, with no host access.
  - Udon, then Luau: general-purpose but sandboxed VMs.
  - Verse: general-purpose with compile-time effect tracking and runtime rollback.
  - Native plugins/mods (Bukkit/Paper, Forge/Fabric, Garry's Mod Lua with server authority): most powerful, with server owners trusting code like any software install.

  Minecraft succeeded by offering both ends: vanilla commands for everyone, native mods for server owners who accept the risk.
- **For an AI agent that turns intent into persistent behavior, three primitives map directly:**
  - Verse-style transactions (an agent-generated behavior either fully applies or leaves no trace).
  - Luau-style per-script environments, read-only builtins, and interrupt budgets.
  - Minecraft-style permission levels (agent code cannot run admin commands).

  The Roblox "schema" approach (AI fills a constrained template) is a further layer: generate into a typed, rule-bounded object model rather than free code.
- **Do not rely on one shared VM.** Even Luau's maintainers say same-VM isolation is not guaranteed and that the C++ stack is unproven. Per-instance VMs, process isolation, or WASM-style capability boundaries are prudent, especially when an LLM is the author.
- **Persistence plus generated code multiplies blast radius.** Verse's rollback-on-failure and Minecraft's per-call caps are both ways to bound damage from buggy logic, which LLM-written logic will often be.

### Gaps
- No fetched source on Minecraft Java datapack function-permission-level defaults (server.properties `function-permission-level`), or on the Bedrock Script API (`@minecraft/server`) sandbox.
- No primary data on exploit or griefing incidents tied to each scripting system (Roblox exploits/executors, VRChat crashers, Garry's Mod backdoored addons). These are widely discussed but were not sourced here.
- Udon runtime performance limits and instruction budgets were not found.
- Current status of Verse's `<no_rollback>` deprecation and the runtime-error rollback fix after 2024 is unknown.
