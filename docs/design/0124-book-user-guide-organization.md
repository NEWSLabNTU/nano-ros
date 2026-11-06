---
rfc: 0124
title: "The nano-ros Book charter: principles, topic levels and a user-guide organization"
status: Draft
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-537]
supersedes: []
superseded-by: null
---

# RFC-0124 — The nano-ros Book charter: principles, topic levels and a user-guide organization

**Amends [RFC-0123](0123-two-books-by-audience.md) D2** (the user book's parts
no longer follow the CLI's user-tier groups; a page's directory is still its
part) and **D4** (one topic, one page per type). **Consolidates** the book
rules of [RFC-0116](0116-adoption-purposes-and-front-doors.md) (D1, D2, D5,
D6, D7, D9), [RFC-0115](0115-emulator-first-board-policy.md) (D4, D8),
[RFC-0114](0114-board-support-registry-primitives.md) (D10, D12),
[RFC-0117](0117-cli-command-tiers.md),
[RFC-0103](0103-one-owner-per-fact.md),
[RFC-0089](0089-ros2-api-adoption-and-the-compile-or-conform-rule.md),
[RFC-0121](0121-workspace-first-project-layout.md),
[RFC-0122](0122-behaviour-bringups-road-witnesses-cadence-backed-ci.md) (D3)
and [RFC-0123](0123-two-books-by-audience.md) into one checklist: the
Principles and Topic levels below. Each principle cites the decision that
stays authoritative for it. Work:
[phase-537](../roadmap/phase-537-book-user-guide-organization.md).

## Summary

The nano-ros Book (`book/`) is published, but it is not approachable. It has
twelve parts, 111 pages (plus 16 generated board pages), about 178,000 words
and 1,188 sub-headings. Seventeen pages are stubs under 500 words, and eleven
run past 3,000. RFC-0123 cut its parts along the CLI's command groups, which is
the product's shape, not the reader's, and most pages mix a tutorial, a how-to,
reference tables and troubleshooting.

This RFC reorganizes the user book as a **user guide**:

- sections ordered by what the reader is doing: Start, Get started on your
  board, Tutorial, User guide, Background, Reference, Extending, plus Releases;
- one concrete starter per supported board, from the emulator or twin to the
  board in your hand (D12);
- one documentation type per page (the Diátaxis split: tutorial, how-to,
  explanation, reference);
- procedure first, with a concrete, working example on every task page;
- nano-ros-specific concepts introduced where the reader first needs them;
- references generated from their source of truth wherever one exists;
- platform pages and tutorial steps that follow the registry's support levels.

Most of the change is moves and merges through the phase-532 page map. The new
writing is the tutorial path, the contract walkthrough and the platform
template.

## Motivation / problem

Measured on `main` c6892401ab (2026-11-05):

| | User book |
| --- | --- |
| Parts | 12 |
| Pages | 111 plus 16 generated board pages |
| Words | ~178,000 |
| `##`/`###` headings | 1,188 |
| Pages under 500 words | 17 (seven under 130: `bench`, `run`, `run-plans`, `flash-and-monitor`, `target-model`, `runner-packages`, `command-packages`) |
| Pages over 3,000 words | 11 (`rmw-vs-upstream` 7.8k, `zephyr` 6.8k, `porting-a-cpp-node` 6.7k, `cli` 6.4k, `configuration` 4.6k, ...) |
| The first node, in three languages | three near-duplicate pages |

What a reader meets:

1. **No path.** Twelve peer parts give no order. The three purposes of RFC-0116
   D1 (education, field solutions, industrial) need different first pages, and
   the book offers none of them a route.
2. **Mixed pages.** A platform chapter is at once a tutorial (scaffold, run), a
   how-to (flash, configure), a reference (Kconfig keys, sizes) and a
   troubleshooting list. Each grows long and is hard to scan.
3. **Concepts arrive as abstractions.** "Concepts and Rationale" explains the
   design apart from any task. The ideas a reader actually needs (a bringup and
   its images, the generated entry, the contract, static pools) are scattered
   across reference pages.
4. **The contract system is buried.** `<stem>.contract.yaml` decides what an
   image creates and how much memory it gets. One `qos` row took a one-node Rust
   listener from 273,802 to 172,298 bytes of static RAM. Yet its only
   explanation sits inside `build/cpp-workspaces.md`, as a C++ detail.
5. **RMW appears late and mixed with implementing one.** Choosing and
   configuring zenoh, Cyclone DDS or XRCE-DDS is a day-one task. Today it sits
   beside writing a custom RMW backend.
6. **Claims outrun the support levels.** A chapter can teach a board or feature
   with no registry row. RTIC had none and was removed (#3145); PX4 has none yet.

## Principles

Every page of the user book, and every change to its organization, follows
these principles. Each one cites the decision that holds it; this list is the
one place they are read together. A principle marked **gated** is checked by
`check-book-audience` or a sibling gate (phase-537 W0, W13).

**Who and why**

1. **Three purposes, stated budgets.** Every user-facing surface is accepted
   against education, field solutions and industrial, with their budgets, and
   adoption probes measure it. (RFC-0116 D1, D9)
2. **Two books by reader.** Application developers and porters read the Book;
   contributors read Internals. (RFC-0123 D1)

**What may be in the book**

3. **A board enters the book only with an emulator twin.** The twin boots the
   same ELF the bench board runs, or the board has a track that replays its
   chapter. QEMU mps2-an385 is the concepts board, deliberately with no
   hardware twin. (RFC-0115 D4) **Gated.**
4. **Claims follow the registry.** Board facts and status words are generated,
   never typed. A board, feature or integration with no registry row gets no
   user page. (RFC-0114 D10, D12; RFC-0123 D9; RFC-0115 D8) **Gated.**
5. **Only the user tier.** No `nros dev`, no checkout steps, no internal ids in
   prose. RFC numbers appear only in a closing "Design background". (RFC-0116
   D2; RFC-0117; RFC-0123 D3) **Gated.**

**How it is organized**

6. **Sections follow what the reader is doing:** Start, Get started on your
   board, Tutorial, User guide, Background, Reference, Extending, Releases.
   (D1)
7. **One topic, one page per type.** Tutorial, how-to, explanation and
   reference are separate pages, linked to each other. A topic has at most one
   page of each type. (RFC-0123 D4 as amended; D2) **Gated.**
8. **Starters before depth.** Every admitted board has a starter that depends
   only on Install, with twin-and-hardware boards first. (D12)
9. **Reference is generated from its owner:** the contract format, the CLI,
   the APIs, `system.toml`, boards. (RFC-0103; D6, D7)

**How it is written**

10. **Procedure first.** A task page gives steps, each a command or edit with
    its expected output, and a complete working example. (D5)
11. **Concepts at the point of need.** Explain only what is uncommon outside
    nano-ros and central inside it, starting from an example. Link to the ROS 2
    documentation for what a ROS 2 user knows. (D4)
12. **The simplest working path first.** Serial before ethernet, with WiFi
    stated as deferred. The first chapter names no launch file, image or entry.
    One run verb, with `nros flash --monitor` on hardware. (RFC-0116 D5, D6,
    D7; RFC-0118 D2; RFC-0121)
13. **Say plainly where nano-ros differs from ROS 2,** and never imply a
    symmetry that is not there. (RFC-0089)
14. **Every runnable step is probed** on the row's evidence. A step that cannot
    run yet fails loudly with an `awaits` line; nothing is skipped. (RFC-0116
    D9; phase-532; D3) **Gated.**
15. **Budgets:** a length per type, nothing under 300 words, and languages in
    tabs, not parallel pages. (D9) **Gated.**

## Topic levels

Every user page declares one level (D2). A page may assume only topics at its
own level or below, and links to them as prerequisites. A level 1 page never
requires a level 3 idea. **Gated:** a page's "needs first" links may point
only at pages of the same or a lower level.

| Level | Topics | Where | Needs first |
| --- | --- | --- | --- |
| **1. First contact** | install; a board starter; run a talker; what you built (node package, bringup, image); add a node; pick and configure the RMW | Start, starters, Tutorial 1–4 | nothing |
| **2. Building real applications** | your own message; services and actions; parameters and launch files; the contract and sizing; the same project on another target; talking to ROS 2 and QoS; a serial link; logging; workspace layout; mixing languages | Tutorial 5–10, User guide | level 1 |
| **3. Production and tuning** | configuring an image; build profiles; offline and vendored builds; deploying; static memory; scheduling and timing; throughput tuning; bridges; porting a ROS 2 node | User guide, Background | level 2 |
| **4. Extending nano-ros** | adding a board, platform or transport; a custom RMW backend; runner and command packages; API design | Extending, then Internals | level 3 and the Background pages |

Reference pages carry the level of the most advanced topic they describe, and
anyone may look them up.

## Reconciled RFCs

| RFC | Book decisions | Status after this RFC |
| --- | --- | --- |
| 0116 | D1, D2, D5, D6, D7, D9 | authoritative; read through Principles 1, 5, 12, 14 |
| 0115 | D4 (board admission), D8 (evidence page) | authoritative; Principles 3, 4. Admission is D4's; D8 below only places what D4 admits |
| 0114 | D10 (status word), D12 (generated facts) | authoritative; Principle 4 |
| 0117 | the user tier | authoritative; Principle 5 |
| 0103 | one owner per fact | authoritative; Principle 9 |
| 0089 | upstream include paths; asymmetries stated | authoritative; Principle 13 |
| 0121, 0122 D3 | workspace from the first page; first chapters point at the copy-out talker | authoritative; Principle 12, D3 |
| 0123 | D1, D3, D5, D6, D7, D8, D9 | authoritative; D2 amended (D1 here); D4 amended to "one topic, one page per type" (Principle 7) |
| 0064 | its book rows (a porting-guide todo, the tier page) | superseded for the book by RFC-0114, RFC-0115 and this RFC |

## Design

### D1 — Sections follow what the reader is doing

The user book has these sections, in this order. Each is one directory, which
keeps RFC-0123 D2's "a directory is its part":

| Section | Directory | Holds | Reader's question |
| --- | --- | --- | --- |
| Start here | `start/` | welcome, choose your path, install | "Where do I begin?" |
| Get started on your board | `starters/` | one starter per supported board or platform (D12) | "Make it run on *my* board" |
| Tutorial | `tutorial/` | one numbered path (D3) | "Teach me" |
| User guide | `guide/` | how-to pages, grouped by task area | "How do I …?" |
| Background | `background/` | nano-ros-specific explanation (D4) | "Why does it work this way?" |
| Reference | `reference/` | lookup material, generated where possible (D7) | "What exactly is …?" |
| Extending nano-ros | `extending/` | boards, platforms, transports, custom RMW backends | "How do I add …?" |
| Releases | `migrating/` | migration and retired names | "What changed?" |

The User guide groups are:
- RMW and communication;
- Contracts and sizing;
- Build and configure;
- Platforms;
- Real-time;
- Deploy and debug;
- Port from ROS 2.

**This amends RFC-0123 D2:** the parts no longer follow the CLI's user-tier
groups. The CLI's groups stay visible where they belong, in the generated CLI
reference.

Appendix A gives the full table of contents and the source of each page.

### D2 — One type per page, declared and checked

Every user page declares its type and level in front matter:

```markdown
<!-- type: how-to · level: intermediate -->
```

- **Types:** `tutorial`, `how-to`, `explanation`, `reference`.
- **Levels:** `beginner`, `intermediate`, `advanced`, shown as a badge on the
  page.

A page's section must match its type. A page that needs a second type is split,
and the halves link to each other.

`check-book-audience` reads the declaration. It starts as a report and becomes a
refusal once the moves land (phase-537 W13).

### D3 — One tutorial path, one project, on Verified rows only

The Tutorial is one numbered path through one growing project, the way the Rust
Book does it. Each chapter fits an RFC-0116 education lab: under two hours, and
the same result on every seat.

| # | Chapter | nano-ros concept introduced (D4) |
| --- | --- | --- |
| 1 | Run a talker on an emulated board (mps2-an385, QEMU, serial) | — |
| 2 | What you just built | node package, bringup, image, generated entry |
| 3 | Add a second node | component rows, the generated launch file |
| 4 | Pick and configure the RMW | the `rmw =` line, per-image RMW, router and agent |
| 5 | Your own message | messages generated from the ROS install |
| 6 | Services and actions | — |
| 7 | Parameters and launch files | — |
| 8 | Tell the build what your system creates | the contract |
| 9 | Same project, another target | image rows |
| 10 | Talk to ROS 2 | interoperable QoS |

Rules for the path:

- **Verified rows only.** Every chapter runs on a registry row whose generated
  status word is Verified (RFC-0114 D10): today `nros-board-mps2-an385`
  (Verified in emulator), `nros-board-linux` (Verified on host), and for chapter
  9, FreeRTOS on mps2-an385 or Zephyr native_sim. A board that is only "Boots,
  manual" never carries a tutorial step. No row is Verified on hardware yet, so
  "run on a real board" is a User guide page (D8), not a chapter.
- **Every chapter is a probe track.** Its `probe=` blocks replay on the
  installed track.
- **One chapter for all three languages,** in tabs (D9).

### D4 — Introduce a concept where the reader first needs it

A concept is explained in the book only if it is uncommon outside nano-ros but
central to using it. It is explained at the point the reader first needs it,
starting from a concrete example.

- **Explained in place:** the bringup and its images, the generated entry, the
  contract, static pools and sizing, board keys and support levels. Each gets a
  short "What this is" passage in the tutorial chapter or how-to that first
  uses it, built around the file or command the reader has just met.
- **Linked, not re-explained:** ideas a ROS 2 user already has (nodes, topics,
  services, actions, QoS, launch files, parameters). The page links to the ROS 2
  documentation and states only what differs in nano-ros.
- **Background holds only the deeper "why".** The execution model, static
  memory and pools, how a build becomes an image, safety, and how nano-ros
  differs from ROS 2 and micro-ROS. Each Background page opens with an example
  before its explanation.

### D5 — Procedure first; RMW use before RMW implementation

Every how-to follows one shape:

1. what you will have at the end;
2. prerequisites, with links;
3. numbered steps, each a command or file edit with its expected output;
4. a complete working example;
5. "If it doesn't work".

Probe fences cover the steps where the target is Verified.

**Using an RMW is a day-one task.** Choosing and configuring zenoh (router,
locators, serial link), Cyclone DDS (no router) and XRCE-DDS (agent), and
switching the RMW per image, appear in Tutorial chapter 4 and in the User
guide's RMW and communication group. Writing a custom RMW backend moves to
Extending, the last section, with the RMW API design material beside it.

### D6 — The contract system: introduction, walkthrough, generated reference

The contract (`<stem>.contract.yaml` beside a launch file, or
`<bringup>/system.contract.yaml` for a bringup with no `launch/`) gets a full
path:

- **Introduction and walkthrough: Tutorial chapter 8.** Write the contract for
  the talker and listener, rebuild, and see static RAM fall. Then rename an
  endpoint and read the refusal, which shows that names are resolved names and
  that the derivation is all-or-nothing per image.
- **How-tos (Contracts and sizing):** declare QoS in a contract; size an image
  from its contract; contracts for a bringup with no launch file; fix a contract
  refusal; measure static memory.
- **Reference:** the contract format, **generated** from ros-launch-manifest's
  normative field table (`types/src/field_table.rs`, rendered by that project
  as `docs/format-reference.md`). It is generated at the tag nano-ros pins
  (`ros-launch-manifest-types` in `packages/cli/nros-cli-core/Cargo.toml`) and
  never copied by hand. A key the reference does not list is a parse error, so
  the page and the parser cannot disagree.

### D7 — Generate a reference from its source of truth

| Reference | Source | Today |
| --- | --- | --- |
| Rust API | rustdoc (`book/rustdoc-driver/`) | built by `docs.yml`; `reference/rust-api.md` is a 160-word stub |
| C and C++ APIs | Doxygen (`packages/api/nros-{c,cpp}/Doxyfile`) | built by `docs.yml`; hand-written pages beside it |
| Contract format | ros-launch-manifest field table (D6) | not in the book |
| CLI | the CLI's own help, which `user-surface-ids` already renders | hand-written `reference/cli.md`, 6.4k words |
| `system.toml` keys | `gen-config-surface.py` | generated |
| Supported boards, support levels | the board registry | generated |

The book's API pages become short landing pages that link to the generated
rustdoc and Doxygen output.

A generated reference is either built in the docs job, or committed and kept
current by a gate. The board pages already work the second way.

### D8 — Admission by RFC-0115 D4, placement by the status word

Two questions, two owners:

- **Admission: may a board have a user page?** RFC-0115 D4 decides. The board
  needs an emulator twin that boots its ELF (the QEMU concepts board,
  Renode, Wokwi), or a track that replays its chapter. Host boards (Linux,
  native_sim, the ThreadX and FreeRTOS simulators) run as host executables
  and qualify the same way. A board that fails this test keeps only its
  generated reference page. Arm FVP is maintainer-run with no replaying track
  (RFC-0115 D4), so `targets/arm-fvp.md` leaves the user book (phase-537 W1)
  until a track replays it.
- **Placement: where does an admitted board go, and what may it claim?** The
  registry's generated status word (RFC-0114 D10) decides:

| Status word, for an admitted board | In the book |
| --- | --- |
| Verified on host / in emulator / on hardware | a starter (D12) and a User guide platform page; eligible for Tutorial steps (D3) |
| Boots, manual (date), with an emulator twin **and** `hardware` | a starter (D12), first in the list, showing the date, and a User guide platform page; never a Tutorial step |
| Builds, or Not supported | the generated board reference page only |
| no registry row | no user page; contributor material goes to Internals |

- **Platform pages are grouped by status, not by OS.** Today one `zephyr.md`
  covers a host-verified board, an emulator-verified board and a board with
  no replaying track. It splits so that each page's generated header is honest. When a status
  changes, for example when a manual check goes stale after
  `manual_stale_days` or a hardware runner lands, the page moves group on
  regeneration.
- **Every platform page uses one template:**
  1. prerequisites;
  2. run in the emulator or simulator;
  3. run on hardware;
  4. board notes;
  5. if it doesn't work.

  It aims for about 1,500 words; detail moves to Reference.
- **"Flash a real board" is a User guide page.** It covers `nros flash` and
  `nros monitor`, with a generated table of what each board's row says about
  hardware.

Applied so far:
- RTIC (no row) moved to Internals in #3145.
- PX4 SITL — record of the state on 2026-11-06, not yet applied. The
  registry row `px4-sitl@Px4` and the `px4` probe track are **not on `main`**:
  they are on PR #3169 (`registry-px4-row`, open, failing
  `check-flavour-lanes`), which replaced the closed #3150. This line said the
  row and track had landed with #3150; they had not.
  [Phase-542](../roadmap/phase-542-px4-supported-platform.md) W1 plans the gate
  rule the row needs. The track, once landed, waits on
  [issue 2289](../issues/2289-px4-release-ships-no-posix-archives.md) (the
  release ships no PX4 POSIX archives; phase-541 ships them, phase-542 W3–W4
  finish the user-tier build). The page stays in User
  guide › Platforms meanwhile, by maintainer ruling (2026-11-05): an exception
  to Principle 3 that ends when the track replays.
- Arm FVP leaves the user book (no replaying track, RFC-0115 D4).

### D9 — Length budgets, no stubs, languages in tabs

| Type | Budget |
| --- | --- |
| how-to | 300–1,500 words |
| tutorial chapter | 1,500–3,000 words |
| explanation | 800–2,500 words |
| reference | no limit, but tables and lookups only |

A page under 300 words is merged into its parent. The seven stubs listed above
become sections of the pages they describe.

Rust, C and C++ variants of the same step are tabs on one page, not parallel
pages. The tabs come from an in-repo mdBook preprocessor, like the existing
`board-status` one. mdBook is pinned at 0.5.4 in `nros-sdk-index.toml`, and
third-party tab preprocessors are not assured for its preprocessor API.

### D10 — A "choose your path" front door for each purpose

`start/choose-your-path` replaces the twelve-part sidebar as the real front
door. It has one card per reader, each with a route and a budget:

| Reader | Route |
| --- | --- |
| Education (RFC-0116 D1: a 15-minute install, labs under 2 hours) | Start → Tutorial 1–7, then a board starter for the lab's board |
| Field solutions (30 minutes from box to `ros2 topic echo`) | Start → Get started on your board (twin, then the board) → Tutorial 10 |
| Industrial (reproducible, auditable, offline) | Start → Background (execution, memory, safety) → User guide (offline, deploy) → Reference (support levels, configuration) |
| ROS 2 developer porting nodes | Background › differences → User guide › Port a C++ node |
| Board porter | Background › how a build becomes an image → Extending → Internals |

Because the field route goes straight to a board starter, a starter depends
only on Install, and platform pages in the User guide only on Tutorial 1–2.

### D11 — Delivery

- **Moves and merges** go through the phase-532 page map (`book/page-map.toml`)
  and `scripts/book-move.py`, so every moved page keeps a redirect.
- **The work lands in small, section-sized PRs,** each rebased onto main rather
  than merging main into it. During phase-532, one 400-file branch with six
  merges of main could not be squashed by the merge queue.
- **Writing waits on the CLI.** Pages whose commands depend on work still in
  flight (phase-531 W4 and W5, phase-535) are rewritten after that work lands,
  so nothing is written twice.

### D12 — A concrete starter for every supported board

"Get started on your board" has one starter per supported board or platform.
Each is a short, complete path from Install to the reader's own board talking
to ROS 2, with nothing assumed from the Tutorial. It is the Tutorial's chapter 1
done on that board, and the platform's User guide page takes over after it.

- **One template:**
  1. what you need (the board, a cable, the host packages);
  2. provision (`nros setup <board>`);
  3. scaffold (`nros new <ws> --board <board>`);
  4. run in the emulator or twin;
  5. flash the board in your hand (when the row names `hardware`);
  6. see it in ROS 2;
  7. if it doesn't work;
  8. next.

  About 1,500 words. Each step is a probe fence that replays where the row's
  evidence runs.
- **Order, by what a newcomer can reach:**
  1. **Twin and hardware** (the row's `runs = "emulator"` names a twin of a real
     board, and `hardware` names that board). A reader runs the very image in
     the twin first, then flashes it to the board, and the twin result predicts
     the board result. This is the level the field and education purposes need
     (RFC-0116 D1), so these starters come first. Today: Nucleo-F767ZI (Renode
     twin) and Raspberry Pi Pico W (Wokwi twin).
  2. **Verified in emulator:** mps2-an385 (bare-metal, FreeRTOS, Zephyr, in
     QEMU), NuttX Arm and RISC-V, ThreadX RISC-V 64.
  3. **Verified on host:** Linux, Zephyr native_sim, ThreadX Linux simulator,
     FreeRTOS POSIX.
- **The header is generated.** Each starter shows its row's status word, so a
  twin board's "Boots, manual (date)" and its staleness after
  `manual_stale_days` are stated, never typed. A starter whose step needs
  something the twin can't run says so, for example a Wokwi twin that needs a
  `WOKWI_CLI_TOKEN`.
- **The list is generated from the registry.** A row that gains a twin and
  `hardware`, or reaches Verified, gets a starter slot. A row that drops to
  Builds loses its starter, and its board reference page remains.

The Tutorial keeps D3's rule: its chapters run on Verified rows only, because a
two-hour lab must give the same result on every seat. A twin board is where a
reader takes the Tutorial's project next.

## Alternatives considered

- **Keep RFC-0123's CLI-group parts and only trim pages.** This fixes length but
  not the missing path or the mixed pages. Readers think in tasks, not verb
  groups.
- **A book per purpose.** RFC-0116 already rejected this: three books triple the
  probe cost and drift apart. One book with per-purpose routes (D10) serves all
  three.
- **Three parallel language books, or language sub-parts.** This triples
  maintenance, and the steps are the same apart from the code. Tabs (D9) keep
  one page.
- **Hand-copy the contract format into the book.** It would drift from the
  parser. The format has one owner, ros-launch-manifest's field table, and the
  book generates from it (D6).
- **A third-party tabs preprocessor.** Its support for mdBook 0.5's API is
  unproven, and it would add another pinned tool. An in-repo preprocessor
  follows the `board-status` precedent.

## Open questions

- **The tutorial's own project.** Does the tutorial use the
  `examples/<plat>/<lang>/talker/` copy-out witnesses (RFC-0122 D3)? Or does it
  scaffold fresh with `nros new` and point at the witnesses as "the same
  project"? The second is the current first chapter's choice.
- **How the docs job gets the contract reference.** The job builds no CLI today,
  so it either fetches ros-launch-manifest at the pinned tag or reads a
  committed, gated copy.
- **PX4's place.** Settled 2026-11-05: an `external` registry row, and the page
  in User guide › Platforms while issue 2289 is open (D8).
- **Background versus Internals.** The boundary needs one rule. The proposal: a
  page whose reader writes an application stays in Background; a page whose
  reader changes nano-ros goes to Internals.

## Appendix A — Target table of contents

← names the current pages each new page is built from. **new** marks pages with
no current source.

```
1. Start here
   Welcome                         ← start/introduction
   Choose your path                ← start/choose-by-purpose (rewritten as cards, D10)
   Install                         ← get-started/installation
2. Get started on your board  (D12; ordered by twin and hardware, then emulator, then host)
   Nucleo-F767ZI: Renode twin, then the board    ← targets/nucleo-f767zi
   Raspberry Pi Pico W: Wokwi twin, then the board **new**
   mps2-an385 in QEMU: bare-metal · FreeRTOS · Zephyr  ← targets/{bare-metal,freertos,zephyr} (starter parts)
   NuttX in QEMU (Arm, RISC-V) · ThreadX in QEMU (RISC-V 64)  ← targets/{nuttx,threadx} (starter parts)
   Linux host · Zephyr native_sim · ThreadX Linux simulator · FreeRTOS POSIX  ← targets/linux-host and the host parts
3. Tutorial
   1 Run a talker on an emulated board   ← get-started/first-project
   2 What you just built                 ← get-started/anatomy
   3 Add a second node                   ← get-started/first-node-{rust,c,cpp} (tabs),
                                            build/growing-your-workspace (steps 1–2)
   4 Pick and configure the RMW          ← communicate/choosing-an-rmw, communicate/switching-rmw (task parts)
   5 Your own message                    ← get-started/your-own-msg-package, build/message-generation (tutorial part)
   6 Services and actions                **new**
   7 Parameters and launch files         ← build/growing-your-workspace (launch and parameters)
   8 Tell the build what your system creates ← build/cpp-workspaces (contract section) **+ new**
   9 Same project, another target        **new** (image rows)
   10 Talk to ROS 2                      ← communicate/ros2-interop, communicate/qos-status-events (basics)
4. User guide
   RMW and communication: Configure zenoh · Use Cyclone DDS · Use XRCE-DDS · Switch the RMW per image ·
     Use a serial link (← communicate/serial-transport) · Bridge between backends (← communicate/bridges) ·
     Tune throughput and latency (← communicate/tx-tuning)
   Contracts and sizing: Declare QoS in a contract · Size an image from its contract ·
     Contracts for a bringup with no launch file · Fix a contract refusal (all ← build/cpp-workspaces + new) ·
     Measure static memory (← realtime/static-memory)
   Build and configure: Organize a larger workspace (← growing-your-workspace, cpp-workspaces, build/roles) ·
     Mix languages (← build/mixed-language) · Configure an image (← build/configuration, task part) ·
     Build profiles (← build/build-profiles, operate/build-profiling) ·
     Build inside an existing CMake or vendor tree (← build/cmake-subdirectory, build/vendored-tree) ·
     Install options, pinning, working offline (← get-started/installation-details)
   Platforms (after the starter: configuration, board notes, deeper tasks), grouped by generated status (D8):
     Verified on host: Linux host · Zephyr native_sim · ThreadX Linux simulator · FreeRTOS POSIX
     Verified in emulator: Bare-metal Cortex-M3 · FreeRTOS on mps2-an385 · Zephyr on mps2-an385 ·
       NuttX Arm and RISC-V · ThreadX RISC-V 64
     Twin and hardware: Nucleo-F767ZI · Raspberry Pi Pico W
     Framework integration: PX4 SITL (← targets/px4; waits on issue 2289 for its probe track)
     (Arm FVP: out of the user book until a track replays it, RFC-0115 D4)
     A Zephyr app you build with west (← targets/zephyr-west-app) · Flash a real board (**new**)
     (← targets/{linux-host,bare-metal,freertos,zephyr,nuttx,threadx,arm-fvp,nucleo-f767zi},
        targets/{run,flash-and-monitor,bench,run-plans,target-model} as sections)
   Real-time: Pick a scheduling model (← realtime/scheduling-models) · Analyze timing (← realtime/rt-analysis)
   Deploy and debug: Deploy to production (← operate/deployment, realtime/production-readiness) ·
     Log and observe a board (← operate/logging, operate/console-less-boards) ·
     Use simulated time (← operate/simulated-time) · Troubleshoot (← operate/troubleshooting)
   Port from ROS 2: Port a C++ node (← from-ros2/porting-a-cpp-node, task part) ·
     Move a ROS 2 project over (← from-ros2/migration-guide, from-ros2/setup-compared)
5. Background
   How a build becomes an image    ← concepts/architecture, concepts/design-overview, targets/how-integration-works
   The execution model             ← realtime/execution-model, concepts/two-layer-api,
                                      realtime/dispatch-strategy, realtime/rtos-cooperation
   Static memory and pools         ← concepts/no-std, realtime/static-memory (background part)
   The client library              ← concepts/client-library
   Safety                          ← realtime/safety-protocol
   How nano-ros differs from ROS 2 and micro-ROS ← from-ros2/differences, from-ros2/vs-micro-ros
6. Reference
   Contract format (generated, D6) · CLI (generated, D7; ← reference/cli) ·
   system.toml keys (← reference/configuration-surface, build/configuration reference part, reference/nros-bridge-toml) ·
   Environment variables · Rust / C / C++ APIs (landing pages to rustdoc and Doxygen) ·
   Platform API and C ABI (← reference/platform-api, extending/platform-c-abi) · RMW API (← reference/rmw-api) ·
   Message generation (← build/message-generation, reference part) · Build outputs (← build/build-artifacts) ·
   Supported boards and support levels (generated; + the board pages) ·
   Feature matrices (← reference/rmw-feature-matrix, sched-matrix, platform-differences) ·
   Static pool inventory (generated) · Troubleshooting index (← get-started/first-10-minutes) · Glossary (**new**)
7. Extending nano-ros
   Add a board (← extending/{custom-board,vendor-overlay,board-crate-import,freertos-board,
     stm32f4-out-of-tree,adding-a-board-to-the-registry,board-integration,board-trait}) ·
   Add a platform (← extending/{custom-platform,add-a-platform,porting-pitfalls,platform-api-design}, concepts/platform-model) ·
   Add a transport (← extending/custom-transport) ·
   Write a custom RMW backend (← extending/{custom-rmw,rmw-design}) ·
   Runner and command packages (← extending/{runner-packages,command-packages}, extending/overview)
8. Releases
   Migrating from older releases (← migrating/{install-local-removal,esp32,retired-config-files}) · Retired names
To Internals: extending/rmw-vs-upstream, extending/zenoh-pico-symbols, extending/xrce-dds-symbols
```

The result is about 80 user pages (from 111) in eight sections (from twelve),
about ten of them board starters,
with no stubs, no page over 3,000 words outside Reference, and one page per
task for every language.

## Changelog

- 2026-11-06 — D8's PX4 line corrected: the row and probe track are on open
  PR #3169, not on `main` (#3150 was closed). Phase-542 owns the rest.
- 2026-11-05 — PX4 SITL placed in User guide › Platforms, waiting on issue
  2289 (maintainer ruling).
- 2026-11 — Retitled as the book charter. Principles, Topic levels and
  Reconciled RFCs added, consolidating the book rules of RFC-0116, 0115, 0114,
  0117, 0103, 0089, 0121, 0122 and 0123. D8 now admits by RFC-0115 D4 and
  places by status word, so Arm FVP leaves the user book. RFC-0123 D4 is
  amended to "one topic, one page per type".
- 2026-11 — D12 added (maintainer review): a concrete starter for every
  supported board, twin-and-hardware boards first. The D8 table gives those
  rows a starter.
- 2026-11 — created (Draft) from the maintainer review of the published book
  (2026-11-05): procedure first, RMW use before implementation, concepts at the
  point of need, the contract system with a generated reference, and the
  support levels respected. Counts taken on `main` c6892401ab.
