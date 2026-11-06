# Phase 537 — The Book charter: principles, topic levels and a user-guide organization

**Status (2026-11-05). OPEN — W0 (the charter's tooling) first.**
Implements [RFC-0124](../design/0124-book-user-guide-organization.md): its
fifteen Principles, four Topic levels, D1 to D12 and the table of contents in
its Appendix A. Follows [phase-532](phase-532-two-books-by-audience.md), whose
page map and `book-move` tooling it reuses.

Depends on:

- [phase-531](phase-531-workspace-first-project-layout.md) W4 (`nros new <ws>`,
  `nros run <node>`, PR #3143) and W5 (one build road), for the starters (W4
  here) and every Tutorial chapter (W5);
- [phase-535](phase-535-example-reorg-and-cadence-ci.md), for the example
  pointers of the platform pages (W9);
- [issue 2289](../issues/2289-px4-release-ships-no-posix-archives.md) (the
  release ships no PX4 POSIX archives), for PX4's probe track to replay. Its
  page stays in Platforms meanwhile (maintainer ruling, 2026-11-05).

---

## Why

The published book has twelve parts, 111 pages, about 178,000 words, seventeen
stubs and eleven pages over 3,000 words, with no path through them (RFC-0124,
Motivation). This phase rebuilds it as the eight sections of RFC-0124 Appendix
A, one work item per section, each written to the charter.

The order of the work matters for two reasons:

- **Cheap structure before expensive prose.** The tooling, moves, merges and
  front door (W0–W3) make the book approachable within days and rewrite no
  text.
- **Prose after the CLI settles** (D11). The starters and the Tutorial (W4,
  W5) and the platform pages (W9) depend on CLI work still in flight. Written
  now, they would be written twice.

## Rules every work item follows

Each PR of this phase is checked against the charter before review. The PR
description lists, for every page it adds or rewrites, the page's type and
level and the principles it was checked against.

**Organization (Principles 2–9).**

- The page sits in the section of Appendix A that names it, and its sources
  are the pages Appendix A lists. A page not in Appendix A needs an RFC-0124
  amendment first.
- One type per page (Principle 7): tutorial, how-to, explanation or reference.
  A topic has at most one page of each type. The types link to each other.
- A board, feature or integration appears only if it has a registry row and,
  for a board, an emulator twin or a replaying track (Principles 3, 4). Board
  facts and status words come from the generator.
- No `nros dev`, no checkout steps, no internal ids in prose. RFC numbers only
  in a closing "Design background" (Principle 5).
- Reference content is generated from its owner where one exists (Principle 9).

**Topic levels.** Every page declares one level, and its "needs first" links
point only at pages of the same or a lower level:

| Level | Sections it is written for |
| --- | --- |
| 1. First contact | Start, starters, Tutorial 1–4 |
| 2. Building real applications | Tutorial 5–10, most of the User guide |
| 3. Production and tuning | the rest of the User guide, Background |
| 4. Extending nano-ros | Extending, then Internals |

**Writing (Principles 10–15).**

- **Procedure first.** A task page is steps; each step is a command or edit
  with its expected output. The page ends with a complete working example.
- **Concepts at the point of need.** Explain a concept only if it is uncommon
  outside nano-ros and central inside it (the bringup, the image, the contract,
  static pools, the execution tiers), and start from the example in front of
  the reader. For what a ROS 2 user knows, link to the ROS 2 documentation.
- **The simplest working path first:** serial before ethernet, WiFi stated as
  deferred; one run verb, with `nros flash --monitor` on hardware.
- **Say plainly where nano-ros differs from ROS 2.** Never imply a symmetry
  that is not there.
- **Every runnable step is a probe fence.** A step that cannot run yet carries
  an `awaits` line and fails loudly; nothing is skipped.
- **Budgets:** how-to 300–1,500 words, tutorial chapter 1,500–3,000,
  explanation 800–2,500, reference unlimited but tables and lookups only.
  Nothing under 300 words. Rust, C and C++ in tabs, not parallel pages.
- **Voice:** the Google developer documentation style guide (second person,
  present tense, active voice, sentence-case headings).

## Work items

Effort: S ≈ 1 day, M ≈ 2–4 days, L ≈ a week or more. Each item lands as one or
more section-sized PRs, rebased onto main rather than merging main in (D11).
Moves go through `book/page-map.toml` and `scripts/book-move.py`, so every
moved page keeps a redirect.

### W0 — Charter tooling: style guide, templates, declarations, report (S)

- A "Writing the Book" page in nano-ros Internals. It opens with RFC-0124's
  Principles and Topic levels, then the rules above, then one template per
  type: a tutorial chapter, a how-to (D5), an explanation (D4), a reference
  page, the board starter (D12) and the platform page (D8).
- A `<!-- type: … · level: … -->` declaration on every user page (D2), set from
  Appendix A.
- `check-book-audience` reports, without refusing, every gated principle and
  the level rule:
  - pages with no declaration;
  - a type that doesn't match the page's section, or two pages of one type for
    one topic (Principle 7);
  - pages outside their type's length budget (Principle 15);
  - a "needs first" link to a page of a higher level (Topic levels);
  - a board page for a board RFC-0115 D4 does not admit (Principle 3);
  - a board fact or status word typed by hand (Principle 4);
  - an internal id outside "Design background", `nros dev`, or a checkout step
    (Principle 5; these already refuse today);
  - a runnable step with neither a probe fence nor an `awaits` (Principle 14).
- A PR description template with the type, level and principles checklist.

**Acceptance:** every user page declares a type and level; the report runs on
the fast lane; the Internals page builds.

### W1 — Page map v2: the eight sections, moves and merges (M)

- New section directories: `start/`, `starters/`, `tutorial/`, `guide/`,
  `background/`, `reference/`, `extending/`, `migrating/`.
- `book/page-map.toml` rows follow Appendix A. Moves and stub merges only, no
  prose rewrite.
- The seven stubs under 130 words fold into their parent pages.
- `rmw-vs-upstream`, `zenoh-pico-symbols` and `xrce-dds-symbols` move to
  Internals (Principle 2).
- `targets/arm-fvp.md` leaves the user book (Principle 3: no replaying track),
  for Internals or the board's generated reference page.
- **Fix `book-move` first,** so it remaps the board registry's book-relative
  fields (`chapter`, `book_run`, `book_flash`). phase-532 did that by hand,
  twice.

**Acceptance:**
- `book-move.py --plan` reports nothing pending;
- the sidebar has the eight sections;
- every old URL redirects;
- all book gates pass, including in a clean clone.

### W2 — Language tabs (M)

- An in-repo mdBook preprocessor for Rust | C | C++ tabs (D9), modelled on
  `scripts/mdbook-board-status.py` and registered in `book/book.toml`. mdBook
  stays pinned at 0.5.4.
- Probe extraction (`extract-book-steps.py`) treats a tabbed block as one step
  per language variant.

**Acceptance:** the docs workflow builds with the preprocessor, and a tabbed
page's probe track extracts each language.

### W3 — Section 1, Start here (S)

| Page | Type · level | From |
| --- | --- | --- |
| Welcome | explanation · 1 | `start/introduction` |
| Choose your path | how-to · 1 | `start/choose-by-purpose`, rewritten as the five cards of D10 |
| Install | how-to · 1 | `get-started/installation` |

- Each card names its reader, route and budget (RFC-0116 D1), and the route a
  card states depends only on pages before it.
- The level badge is rendered from the W0 declaration.
- Install is procedure only: the 15-minute path, each step with its expected
  output. Pinning, offline and vendored installs move to the User guide (W8).

**Acceptance:** each card's route resolves to existing pages; Install replays
in its probe track within the education budget.

### W4 — Section 2, Get started on your board (M–L)

One starter per admitted board under `starters/` (D12), on the starter
template: what you need, provision, scaffold, run in the emulator or twin,
flash the board, see it in ROS 2, if it doesn't work, next. About 1,500 words,
how-to, level 1, depending only on Install.

| Order | Starters | From |
| --- | --- | --- |
| 1. Twin and hardware | Nucleo-F767ZI (Renode twin) · Raspberry Pi Pico W (Wokwi twin, **new**) | `targets/nucleo-f767zi` |
| 2. Verified in emulator | mps2-an385 in QEMU: bare-metal, FreeRTOS, Zephyr · NuttX Arm and RISC-V · ThreadX RISC-V 64 | the starter parts of `targets/{bare-metal,freertos,zephyr,nuttx,threadx}` |
| 3. Verified on host | Linux host · Zephyr native_sim · ThreadX Linux simulator · FreeRTOS POSIX | `targets/linux-host` and the host parts |

- **The list, order and headers are generated from the registry:** a twin and
  `hardware` on the row, or the status word. A row that changes level changes
  slot on regeneration.
- **Each step is a probe fence.** Twin starters replay through the twin
  tracks: `renode-nucleo` in CI, and `wokwi-pico` where the user has a
  `WOKWI_CLI_TOKEN`, failing loudly where it isn't set.
- Serial first; ethernet as the "next" step where the row has it; WiFi stated
  as deferred (Principle 12).
- PX4 SITL gets a starter only when its probe track replays (issue 2289).

**Depends on:** phase-531 W4 (the scaffold); W2 (tabs, where a starter shows
more than one language).

**Acceptance:**
- every board whose row is Verified, or has a twin and `hardware`, has a
  starter, and no other board has one;
- each starter's probe track replays, or fails naming its unmet precondition.

### W5 — Section 3, the Tutorial (L)

One project carried through ten chapters (D3). Each chapter runs on a Verified
row, fits a two-hour lab, is a probe track on the installed track, and
introduces its nano-ros concept from the example in front of the reader (D4).

| Chapter | Level | Concept introduced | From |
| --- | --- | --- | --- |
| 1 Run a talker on an emulated board | 1 | none named: no launch file, image or entry | `get-started/first-project` |
| 2 What you just built | 1 | node package, bringup, image | `get-started/anatomy` |
| 3 Add a second node | 1 | the workspace | `get-started/first-node-{rust,c,cpp}` (one page, tabs), `build/growing-your-workspace` steps 1–2 |
| 4 Pick and configure the RMW | 1 | RMW backends: zenoh, Cyclone DDS, XRCE-DDS, in use | `communicate/choosing-an-rmw`, `communicate/switching-rmw` (task parts) |
| 5 Your own message | 2 | message packages | `get-started/your-own-msg-package`, `build/message-generation` (tutorial part) |
| 6 Services and actions | 2 | none new | **new**, on the `server`, `client` and `action` templates |
| 7 Parameters and launch files | 2 | the launch file as the system's description | `build/growing-your-workspace` (launch and parameters) |
| 8 Tell the build what your system creates | 2 | the contract and static sizing | `build/cpp-workspaces` (contract section) **+ new** |
| 9 Same project, another target | 2 | image rows | **new** |
| 10 Talk to ROS 2 | 2 | where nano-ros differs from ROS 2 on the wire | `communicate/ros2-interop`, `communicate/qos-status-events` (basics) |

- Chapter 4 teaches RMW use only; a custom RMW stays in Extending (D5).
- Chapter 8 is the contract walkthrough of D6: write the contract for the
  talker and listener, watch static RAM fall, read one refusal. Its measured
  RAM drop replays in its probe.

**Depends on:** phase-531 W4 (#3143) and W5; W2 (tabs).

**Acceptance:**
- every chapter's probe track replays green with no `awaits`;
- every chapter states its board's generated status;
- an education adoption probe (RFC-0116 D9) completes chapters 1–7 within
  budget.

### W6 — Section 4, User guide: RMW and communication (M)

How-tos, level 2–3:
- Configure zenoh · Use Cyclone DDS · Use XRCE-DDS · Switch the RMW per image
  (from the task parts of `communicate/*`);
- Use a serial link (`communicate/serial-transport`);
- Bridge between backends (`communicate/bridges`);
- Tune throughput and latency (`communicate/tx-tuning`).

Each page: the task, its steps with expected output, one working example. The
"why" parts go to Background (W10), the option tables to Reference (W11).

**Acceptance:** each page is one how-to within budget, and each runnable step
is a probe fence or carries an `awaits`.

### W7 — Section 4, User guide: Contracts and sizing (M)

- Declare QoS in a contract · Size an image from its contract · Contracts for
  a bringup with no launch file · Fix a contract refusal: moved out of
  `build/cpp-workspaces.md` and written for every language, in tabs.
- Measure static memory (the task part of `realtime/static-memory`).
- Each links back to Tutorial 8 for the walkthrough and forward to the
  contract format reference (W11).

**Acceptance:** no contract how-to remains in a C++-only page; each page
replays in its probe.

### W8 — Section 4, User guide: Build and configure, Real-time, Deploy and debug, Port from ROS 2 (M–L)

Split the mixed pages by type (Principle 7) while moving their task parts here:

| Group | Pages (level) | From |
| --- | --- | --- |
| Build and configure | Organize a larger workspace (2) · Mix languages (2) · Configure an image (3) · Build profiles (3) · Build inside an existing CMake or vendor tree (3) · Install options, pinning, working offline (3) | `build/{growing-your-workspace,cpp-workspaces,roles,mixed-language,configuration,build-profiles,cmake-subdirectory,vendored-tree}`, `operate/build-profiling`, `get-started/installation-details` |
| Real-time | Pick a scheduling model (3) · Analyze timing (3) | `realtime/{scheduling-models,rt-analysis}` |
| Deploy and debug | Deploy to production (3) · Log and observe a board (2) · Use simulated time (3) · Troubleshoot (2) | `operate/{deployment,logging,console-less-boards,simulated-time,troubleshooting}`, `realtime/production-readiness` |
| Port from ROS 2 | Port a C++ node (3) · Move a ROS 2 project over (3) | `from-ros2/{porting-a-cpp-node,migration-guide,setup-compared}` |

The largest pages split this way: `reference/cli.md`, `build/configuration.md`
and `from-ros2/porting-a-cpp-node.md` send their task parts here and their
tables to Reference (W11); `build/growing-your-workspace.md` sends its steps to
the Tutorial (W5).

**Acceptance:** no page outside Reference is over 3,000 words, and every page
in these groups is one how-to.

### W9 — Section 4, User guide: Platforms by support level (M–L)

- One platform page per platform on the D8 template, taking over where its
  starter ends: configuration, board notes, deeper tasks. Level 2–3, needing
  only Tutorial 1–2 and the starter.
- Grouped by generated status: Verified on host · Verified in emulator · Twin
  and hardware. "Checked by hand" rows show their manual date.
- `targets/zephyr.md` splits by status. `targets/{run,flash-and-monitor,bench,
  run-plans,target-model}` become sections of the pages that use them.
- A Zephyr app you build with west (`targets/zephyr-west-app`).
- Flash a real board (**new**), with a generated per-board hardware table.
- A gate checks that each platform page's group matches its row's status word.

**Depends on:** phase-535 (example pointers); W4 (the starters these pages
follow).

**Acceptance:**
- every platform page sits in the group its status word names;
- no user page describes a board or feature with no registry row.

### W10 — Section 5, Background (M)

Explanations, level 3, 800–2,500 words, each opening with a concrete example
before the model (D4):

| Page | From |
| --- | --- |
| How a build becomes an image | `concepts/{architecture,design-overview}`, `targets/how-integration-works` |
| The execution model | `realtime/{execution-model,dispatch-strategy,rtos-cooperation}`, `concepts/two-layer-api` |
| Static memory and pools | `concepts/no-std`, `realtime/static-memory` (background part) |
| The client library | `concepts/client-library` |
| Safety | `realtime/safety-protocol` |
| How nano-ros differs from ROS 2 and micro-ROS | `from-ros2/{differences,vs-micro-ros}` |

Steps found in these sources move to the User guide; tables move to Reference.
The differences page states each asymmetry plainly (Principle 13).

**Acceptance:** each page is one explanation within budget, with no runnable
step.

### W11 — Section 6, Reference: generated where an owner exists (M)

- **Contract format,** generated from ros-launch-manifest's field table at the
  tag pinned in `packages/cli/nros-cli-core/Cargo.toml` (D6). Either the docs
  job fetches it, or a committed copy is kept current by a gate that compares
  it with the pinned tag (RFC-0124 open question).
- **CLI,** generated from the CLI's help, in the docs job or committed and
  gated like the board pages (D7).
- **Rust, C and C++ APIs:** landing pages that link to the rustdoc and Doxygen
  output `docs.yml` already publishes.
- **`system.toml` keys** stay generated from `gen-config-surface.py`;
  **boards and support levels** and the **static pool inventory** stay
  generated.
- Hand-kept lookups: environment variables, platform API and C ABI, RMW API,
  message generation, build outputs, feature matrices, a troubleshooting index
  (from `get-started/first-10-minutes`) and a **new** glossary of the nano-ros
  terms the Tutorial introduces.

**Acceptance:**
- the contract reference names the pinned tag, and a gate refuses a copy that
  differs from that tag's table;
- `reference/cli` cannot drift from `nros --help`;
- no hand-written reference duplicates a generated one.

### W12 — Sections 7 and 8, Extending and Releases (S–M)

- Extending, level 4: Add a board · Add a platform · Add a transport · Write a
  custom RMW backend · Runner and command packages, each merging the
  `extending/*` pages Appendix A lists into one how-to, with its design parts
  linked from Internals.
- Releases: Migrating from older releases (`migrating/*`) · Retired names.

**Acceptance:** Extending has five pages, each needing only level 3 pages and
Background; Releases has two.

### W13 — Tighten the gates (S)

W0's report becomes a refusal for every gated principle and the level rule:
- a missing or mismatched type or level;
- a "needs first" link to a higher level;
- a page under 300 words, or a page outside Reference over its budget;
- a platform page in the wrong status group;
- a board page with no admitting twin or track.

**Acceptance:** `check-book-audience` refuses each of these, with a self-test
case for each.

## Absorbed from phase-523 W1 (maintainer, 2026-11-06)

[Phase-523](phase-523-education-labs-adoption-probes.md) W1 planned the same
deliverable as W5, a lab series, so it moved here. W5 carries these as well:

- **Labs W5's chapter list lacks.** QoS (523 lab 2), lifecycle (lab 4), and
  two tiers with the real-time promise measured (lab 5). Each is a W5 chapter
  or a level-2 lab beside it, on the same probe-track rule. Issue
  [2269](../issues/2269-no-emulator-chapters-for-lab-topics.md) is the round-one
  education stop that measured the gap.
- **A grading command per chapter.** It is the chapter's own
  `nros run --expect '<line>'`, the line an instructor runs per seat. A chapter
  whose output is not deterministic across seats states a range or a predicate,
  or is kept as a demonstration and left out of the graded series.
- **The twin and the bench.** Chapters run on QEMU and on the Renode
  Nucleo-F767ZI twin, so a seat needs no hardware. One chapter runs the same
  image on the Nucleo bench, in manual cadence with a dated run. It waits for
  phase-524 W5, whose hardware work is deferred.
- **Acceptance.** `just probe <chapter>` passes for every graded chapter.

## Order

```
W0 ──► W1 ──► W3 (Start)
        ├───► W2 (tabs) ──► W4 (starters; also phase-531 W4)
        │                └► W5 (Tutorial; also phase-531 W4/W5)
        ├───► W6, W7, W8 (User guide)  ──► W10 (Background)
        ├───► W11 (Reference)
        └───► W12 (Extending, Releases)
W9 (Platforms) after W4 and phase-535
W13 last
```

W6–W8 and W10–W12 need no CLI work in flight, so they go first while W4, W5
and W9 wait.

## Acceptance for the phase

- The user book has the eight sections of Appendix A, about 80 pages, no page
  under 300 words, and no page outside Reference over 3,000 words.
- Every user page declares one type and level, and the gate refuses a missing
  declaration, a mismatched type or a link to a higher level.
- Every admitted board has a starter, twin-and-hardware boards first, each
  replaying through its probe track.
- The Tutorial's ten chapters replay green on Verified rows.
- The contract, CLI and API references are generated from their sources.
- Every platform page's group matches its board's generated status word.

## Not in scope

- New platforms or boards: the registry decides what has a page.
- Rewriting the Internals book's structure; only pages moved into it are
  touched.
- Translations.
