---
rfc: 0123
title: "Two books by audience: the nano-ros Book and nano-ros Internals"
status: Draft
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-532]
supersedes: []
superseded-by: null
---

# RFC-0123 — Two books by audience: the nano-ros Book and nano-ros Internals

**Amends [RFC-0116](0116-adoption-purposes-and-front-doors.md) D2** (the gate's
subject widens from `book/src/getting-started/` to the whole user book, and the
contributor-block exemption is retired). **Follows** RFC-0114 (the board
registry), RFC-0121 (workspace-first projects), RFC-0116 (purposes and front doors), RFC-0117 (CLI tiers), RFC-0118
(target execution), RFC-0119 (extension packages) and RFC-0120
(`nros configure`): the book's parts are cut along the CLI's user-tier groups,
and it states no board fact the registry does not generate (D9). Work:
[phase-532](../roadmap/phase-532-two-books-by-audience.md).

## Summary

`book/` serves three readers through one table of contents: the application
developer, the porter (who adds a board, platform, transport or RMW backend
against the public contract), and the nano-ros contributor. Contributor
material sits inside user chapters behind "**Contributors:**" markers, and
user material sits under "Internals". This RFC splits the material into two
books by reader. **The nano-ros Book** (`book/`) is for application developers
and porters. **nano-ros Internals** (`book-internals/`) is for contributors.
Inside each book a page's directory is its part, so where a page lives says
who reads it. The user-surface gates then cover all of `book/src/`, with no
exemption.

## Motivation / problem

Measured on `main` 8fdb89b (142 pages):

- **Internal ids in user prose.** Counting `phase-NNN`, `RFC-NNNN`,
  `issue NNNN` and `just check` tokens per directory: `user-guide/` 114,
  `reference/` 126, `concepts/` 38, `start-here/` 6, `getting-started/` 5
  (that last directory was swept by phase-525 W1, and it is the only one
  RFC-0116 D2's gate reads). Examples: `user-guide/configuration.md` cites
  ~40 RFCs, `user-guide/rtic-integration.md` says "when 216.B.4 lands", and a
  heading in `concepts/two-layer-api.md` reads "(122.3.c.6.e)".
- **Checkout-only commands in user chapters.** Users are told to run
  `just zephyr ci-both`, `just setup zephyr`,
  `scripts/qemu/setup-network.sh` and `scripts/read-boot-report.py`. A reader
  who installed the release asset (phase-447 A3) has none of them.
- **Audience and section do not line up.** `reference/build-commands.md` is the
  `just` cheat sheet, filed under Reference. `internals/scheduling-models.md`,
  `realtime-analysis.md`, `safety.md`, `production-readiness.md` and
  `measuring-static-memory.md` are for users and evaluators, filed under
  Internals. The porter references `internals/porting-platform/*` are filed
  under Internals too. `getting-started/*` pages appear in five different
  SUMMARY parts.
- **Gates select pages by directory, and the directories mean nothing.**
  `check-book-no-just` reads `getting-started`, `user-guide`, `start-here` and
  `platform-guides`. `check-user-surface-ids` reads only `getting-started`.
  Everything else is called "contributor-facing" by default, including
  `reference/` and `concepts/`, which users read. Inside the scope, the
  `book_contributor` exemption (any "contributor" within eight lines, or a
  heading containing the word) is a heuristic that has to guess what a
  directory could have stated.
- **One topic, many pages.** There are 8 workspace pages, 2 PX4 pages,
  2 troubleshooting pages, ~6 RMW pages on the user side and ~6 on the porter
  side, and ~7 platform-API pages. `docs/reference/{environment-variables,
  std-alloc-requirements,rmw_zenoh_interop}.md` are full-text copies of book
  pages. `docs/guides/` is half migrated.
- **Retired material in the reader's path.** Two retired ESP32 chapters stay in
  the TOC, ESP32 sections remain in `build-artifacts`, `deployment` and
  `build-profiling`, and `embassy-integration.md` is a 26-line stub.

Contributors have no curated entry either. `docs/` holds ~3,000 records
(RFCs, phases, issues, audits), and its narrative guides sit next to dated
audit logs.

## Design

### D1 — Two books, split by reader

| Book | Root | Reader | Promise |
| --- | --- | --- | --- |
| **nano-ros Book** | `book/` | application developer; porter | Everything here works from an installed release with `nros`, the vendor's build tool and a shell. No checkout. |
| **nano-ros Internals** | `book-internals/` | contributor / maintainer | Assumes a checkout, `just`, and the three numbered doc series. |

**Porting stays in the user book**, as its own part ("Extending nano-ros").
A porter works against the stable public contract (the `Board` trait family,
the platform C ABI, the RMW vtable), from outside the tree, exactly as an
application developer does (`porting/stm32f4-out-of-tree.md` is the model).
Material that only matters to someone changing that contract in the tree
(the full symbol ledgers, how the comparisons are regenerated, CI lanes) goes
to Internals.

### D2 — A page's directory is its part

> **Amended by [RFC-0124](0124-book-user-guide-organization.md) D1 (2026-11):** the user
> book's parts no longer follow the CLI's user-tier groups. They follow what the
> reader is doing: Start, Get started on your board, Tutorial, User guide,
> Background, Reference, Extending, Releases. "A directory is its part" stands; the table below is superseded for
> the user book (Internals keeps its parts).

Each SUMMARY part maps to exactly one directory, and each directory to one
part. The user book:

| Part | Directory | CLI group it teaches (RFC-0117 D2) |
| --- | --- | --- |
| Start | `start/` | — (purposes, RFC-0116 D1) |
| Get Started | `get-started/` | Start: `new`, `setup`; then `run` |
| Build a Project | `build/` | Build: `build`, `configure`, `clean`, `materialize`; Workspace: `list`, `graph`; Interfaces |
| Run on a Target | `targets/` | Run on a target: `run`, `flash`, `monitor`, `bench` (RFC-0118) |
| Communicate | `communicate/` | — (RMW, QoS, interop) |
| Real-Time and Safety | `realtime/` | Inspect: `plan resolve\|check\|explain` |
| Operate and Debug | `operate/` | Inspect: `doctor`, `config show\|explain`, `board info` |
| Coming from ROS 2 | `from-ros2/` | — |
| Concepts and Rationale | `concepts/` | — |
| Extending nano-ros | `extending/` | `board new\|check`; runner and command packages (RFC-0119) |
| Reference | `reference/` (generated board pages under `reference/boards/`) | the whole user tier; Toolchain |
| Migrating from Older Releases | `migrating/` | retired spellings (RFC-0117 D5 aliases) |

Internals:

| Part | Directory |
| --- | --- |
| Orientation | `orientation/` |
| Development Environment | `dev-env/` |
| Build Pipeline and Codegen | `pipeline/` |
| RMW Internals | `rmw/` |
| Platform Internals | `platform/` |
| Real-Time Internals | `realtime/` |
| Testing and Verification | `testing/` |
| Release | `release/` |

Appendix A of this RFC gives the target page list. Phase-531 W0 commits it as
a machine-readable page map.

**Real-Time and Safety is a new user part.** Adopters building safety islands
and zonal controllers (RFC-0029, RFC-0079, RFC-0108) need the scheduling
model, RT analysis, static memory and the E2E protocol in one place. Today
these pages are spread across Concepts, User Guide and Internals.

### D3 — Contributor blocks are retired; the gates cover the whole user book

> **Consolidated in [RFC-0124](0124-book-user-guide-organization.md) Principle 5 (2026-11).**


A user-book page carries no contributor material. A block that is now marked
"**Contributors:**" moves to the matching Internals page, and the user page
keeps at most one line linking to it ("Changing this in the tree: see
Internals → *page*").

Once the sweep is done:

- `check-user-surface-ids` (RFC-0116 D2), `check-book-no-just` and
  `check-book-no-checkout` read **`book/src/**/*.md`**, not a list of
  directories.
- The `book_contributor` exemption is deleted. The `no-just-ok` escape for
  quoted tool output stays: it marks verbatim text, not an audience.
- A `scripts/…` path in a user chapter is a defect of the same kind as a
  `just` recipe. Either the CLI gains the verb (file a front-door issue, as
  RFC-0116 D2 does for refusals), or the content moves to Internals.

**Design background.** A user or porter page may end with one
`## Design background` section that lists RFC links by title. It is the only
place a user page names an RFC, and the gate allows an RFC id there and
nowhere else. Phase, issue and PR ids are never allowed in the user book.
This keeps the rationale one click away for the porter who needs it, without
putting ids in the text.

### D4 — One topic, one page

> **Amended by [RFC-0124](0124-book-user-guide-organization.md) Principle 7 (2026-11):** one topic, one page **per type**. A topic may have a tutorial chapter, a how-to, an explanation and a reference page, each linked to the others, but never two pages of the same type.


Each topic has one owning page, and other pages link to it rather than
restating it. The merges are listed in phase-532 W5. They cover workspace
(8 → 6), PX4 (2 → 1), troubleshooting (2 → 1, with per-chapter
troubleshooting sections linking into it), RMW choice (`rmw-choosing` +
`rmw-backends` → 1, with `rmw-feature-matrix` as the only capability matrix),
and the `docs/reference` duplicates (one copy each).

When a user page and an Internals page cover the same subject from two sides,
for example `communicate/ros2-interop.md` and Internals
`rmw/zenoh-protocol.md`, the user page covers what to do and the Internals
page covers how it works. They link to each other and do not overlap.

### D5 — Retired and migration material lives in one appendix

Retired names, removed commands (`install-local`), the retired ESP32 flows and
the "Retired files" section of `configuration.md` move into
`migrating/`. A retired platform leaves the TOC and the chapters; its row in
`migrating/retired-names.md` says what replaced it. The gates that already
exempt `reference/retired-names.md` (`check-retired-knob-spellings`,
`check-retired-cmake-names`) follow the new path.

### D6 — Relationship to `docs/`

- `docs/design/`, `docs/roadmap/`, `docs/issues/` and the dated audit logs in
  `docs/development/` stay where they are. They are records. Internals links
  to them and does not copy them.
- The narrative guides move into Internals: `docs/development/{sdk-tiers,
  test-harness,ci-conventions,versioning,ros2-on-non-ubuntu,
  zephyr-upstreaming,zephyr-version-support}.md`, the unmigrated
  `docs/guides/*`, and the analysis pages in `docs/reference/` that explain
  rather than record (`canonical-build-path`, `platform-implementation-notes`,
  `codegen-type-mapping`, `board-cmake-schema`, `cyclonedds-fork-delta`,
  `qemu-icount`). The old path keeps a one-line "moved" stub for one release,
  because CLAUDE.md, AGENTS.md and many issues link to these files.
- `docs/guides/` is deleted once empty.

### D7 — Build, publish, link

- `just book` builds both books. `docs.yml` deploys the user book at the
  site root and Internals at `/internals/`.
- A link between the two books is a relative `.html` link between the two
  deploy roots (from `build/x.md`: `../internals/rmw/zenoh-protocol.html`).
  It is not site-absolute, because the site is served under a path prefix
  (GitHub Pages' `/nano-ros/`). `check-book-links` turns the page into its
  deploy URL, joins the link, and resolves the result against the other
  book's source tree, so a cross-book link is checked the same way as a link
  inside one book. `scripts/book-move.py` writes these links.
- Until W4 empties the user book's `internals/` directory, its pages share
  the `/internals/` URL prefix with the Internals book. Filenames cannot
  collide, because every Internals page sits in a part subdirectory. D8's gate
  asserts that no URL is served by both books.
- Each moved page gets an `[output.html.redirect]` entry from its old URL,
  kept for two releases. External links into the book (READMEs, issues,
  papers) do not break.
- Scripts that name book paths (63 files today: generators, gates, probes) do
  not hardcode them. They read the page map (D2), so the next move edits one
  file.

### D8 — Gate: `check-book-audience`

> **Extended by [RFC-0124](0124-book-user-guide-organization.md) D2 and phase-537 W0/W13 (2026-11):** the gate also reads each page's type and level declaration and checks the gated principles.


On the fast line:

- every page in a SUMMARY lives in that book's directory for its part (D2),
  and every `.md` file under a book is in its SUMMARY (no orphans);
- no page is in both books;
- the user book contains no "**Contributors:**" marker (D3);
- every page-map row resolves.

### D9 — The book follows the CLI design and the board registry

> **Consolidated in [RFC-0124](0124-book-user-guide-organization.md) Principles 4 and 5 (2026-11);** board admission is RFC-0115 D4 (RFC-0124 D8).


The book teaches the tool as it is designed now. It does not describe how the
tool used to work, and it is not cut along nano-ros's internal layering.

- **Only user-tier verbs (RFC-0117 D2) in the user book.** `nros dev …`, the
  hidden aliases and the old spellings (`sync`, `ws …`, `store`, `init`)
  appear only in Internals and in `migrating/`. `check-cli-old-spellings`
  already covers `book/src`. `reference/cli.md` documents the user tier, and
  Internals `dev-env/cli-dev-tier.md` documents `nros dev`.
- **Vocabulary is RFC-0120 D9's.** Unqualified, "configure" means
  `nros configure`, and "host configure" means CMake's, west's or make's.
  `nros build` is the recommended front door. A page for a user who stays in
  west, CMake, NuttX make or an IDE (the vendored-tree and CMake-subdirectory
  pages, and the per-RTOS chapters) shows `nros configure` in the host's
  configure hook, never `nros build`.
- **Running is one model (RFC-0118).** The Run on a Target part opens with the
  target model: launcher or flasher, console, link. It continues with one page
  per verb (`run`, `flash` + `monitor`, `bench`) and one on the RunPlan and its
  customisation ladder (`--plan`, the five fact levels, `[board.run]`,
  `[board.flash]`, `[board.console]`, the bench file). Per-RTOS chapters say
  how a host tool or vendor build reaches nano-ros. They do not restate run
  or flash steps; they show the `nros run` / `nros flash --monitor` line and
  link to the verb page.
- **Every project is a workspace, from the first page (RFC-0121).** The
  first chapter runs `nros new <ws> --board <board> --template talker`, then
  `nros run <node>`, serial first (RFC-0116 D5, D6 as amended by RFC-0121).
  It shows the workspace tree and tells the reader they need not open the
  bringup yet. Bringup, image, launch and entry are explained in Build a
  Project, which teaches one path: growing the workspace. No page shows a
  `system.toml` beside code. Start's entry page routes by **purpose** (industrial, field
  solutions, education; RFC-0116 D1), not by persona.
- **No board fact is typed in prose (RFC-0114 D12).** A board's status
  (the status word: "Verified in emulator, daily", "Builds", …), breadth,
  emulator and run line come from the registry and the board descriptor
  through the generated pages (`reference/supported-boards.md`,
  `reference/board-support-levels.md`, `reference/boards/*`). A chapter
  names a board and links its generated page. It never says "tier N", never
  says "works on X" in its own words, and never copies a run command the
  descriptor owns. The words are the status words, not tier numbers.
- **Extending covers both kinds of extension.** Boards and platforms (the
  `Board` trait, `nros board new|check`, descriptors with `[board.run]`,
  `[board.flash]` and `[board.console]`) are one kind. Runner and command
  packages (RFC-0119: descriptors, the protocol-1 runner contract, the
  command contract, `nros dev runner|command check`) are the other.
- **Pages wait for the verb to ship.** A verb page goes in when the verb lands
  (phase-528 W4 for `run`, phase-529 W3 for `flash`/`monitor`, phase-530 W1
  for `configure`). Until then the part holds a stub naming what to use today,
  so the TOC already has its final shape.

## Alternatives considered

- **One book with a Contributor part at the end.** This is the closest to the
  current layout. Rejected because search, navigation and the gates still
  mix the readers. The `book_contributor` heuristic exists only because one
  tree serves two readers.
- **Three books (users, porters, contributors).** Rejected. Porters are also
  application developers (they write the first node on their new board), the
  porter surface is a public contract, and a third book would duplicate
  Concepts and Reference.
- **No Internals book; contributors read `docs/` directly.** Rejected:
  `docs/` is a record store (~3,000 files) with no reading order. CLAUDE.md
  already acts as a router to it, which shows that a curated entry is needed.
- **Keep the directories, fix only SUMMARY.** Rejected. The gates select by
  directory (`TRACKS`, `BOOK_GLOB`), so an audience that only SUMMARY knows
  about cannot be enforced.

## Open questions

1. **Internals root:** `book-internals/` (a sibling of `book/`, symmetric
   tooling) or `docs/book/` (next to the records it narrates)? Proposed:
   `book-internals/`.
2. **The `## Design background` allowance (D3):** keep it, or forbid RFC ids
   in the user book entirely and link to Internals pages instead?
3. **API reference "Internals (porting only)" sections** in `rust-api.md` and
   `c-api.md`: move them to `extending/` (porter) or to Internals? Proposed:
   `extending/`, because they are part of the public contract.
4. **`design/rmw-vs-upstream.md` (1,074 lines) and
   `reference/rmw-api-comparison.md` (1,361 lines):** proposed split is a
   porter summary in `extending/` and the full symbol ledger plus its
   "Reproduce" section in Internals `rmw/`.
5. **How long redirects are kept:** two releases, or permanently?

## Appendix A — Target page list

User book (`book/src/`). The old path is in parentheses. *merge* folds a page
into the one above it. *split* means the contributor half moves to Internals.

- **start/**: introduction (`introduction`; "Key Features" rewritten as user
  benefits), choose-by-purpose (`start-here/choose-your-entry`, re-cut by
  RFC-0116 D1 purposes)
- **get-started/**: installation (common path first; per-host special cases in
  a second page), first-project (workspace-first; owned by phase-525 W4 together with
  phase-531 W8),
  anatomy, first-node-{rust,c,cpp}, your-own-msg-package, first-10-minutes
  (`troubleshooting-first-10-min`)
- **build/**: build-and-configure (new: `nros build` vs `nros configure`, the
  three layers of RFC-0120 D1, which road calls which), growing-your-workspace (`workspace-from-app-node`; `workspace-node-pkgs`,
  `workspace-bringup` and `workspace-entry-pkg` merge into it, phase-531 W8;
  the entry-pkg migration section moves to `migrating/`), cpp-workspaces, mixed-language (+ merge
  `workspace-languages`), roles (`component-and-entry-pkg`, split),
  message-generation (split), configuration (split), build-profiles,
  build-artifacts, vendored-tree, cmake-subdirectory
- **targets/**: target-model (new, RFC-0118 D1), run (new, phase-528 W4),
  flash-and-monitor (new, phase-529 W3), bench (new, phase-529 W4),
  run-plans (new: `--plan`, the fact ladder, `[board.run|flash|console]`),
  how-integration-works, linux-host (`platform-guides/native-host`),
  freertos, zephyr (split), nuttx, threadx, bare-metal, px4 (+ merge
  `integration-px4`), arm-fvp
- **communicate/**: choosing-an-rmw (`rmw-choosing` + merge `rmw-backends`,
  split), switching-rmw, qos-status-events (`concepts/status-events`),
  ros2-interop, bridges, serial-transport, tx-tuning
- **realtime/**: rtos-cooperation, scheduling-models, execution-model
  (`internals/rt-execution-model`, split), dispatch-strategy (split),
  rt-analysis, static-memory (`internals/measuring-static-memory`),
  safety-protocol (`internals/safety`), production-readiness, rtic (split)
- **operate/**: logging, simulated-time, deployment (split), console-less-boards,
  build-profiling, troubleshooting (split; "Test Failures" and the FFI section
  move to Internals)
- **from-ros2/**: setup-compared, porting-a-cpp-node, differences
  (`ros2-comparison`), vs-micro-ros, migration-guide
- **concepts/**: architecture, two-layer-api (split), platform-model (split),
  no-std (split), design-overview, client-library
- **extending/**: overview, board-trait, custom-board (`nros board new|check`,
  descriptor `[board.run|flash|console]`), freertos-board,
  vendor-overlay, stm32f4-out-of-tree, board-crate-import, custom-platform,
  add-a-platform, custom-transport, custom-rmw, porting-pitfalls (split),
  platform-c-abi (the checklist half), rmw-design (`design/rmw`),
  rmw-vs-upstream (summary, OQ 4), platform-api-design, board-integration
  (`concepts/board-integration`), zenoh-pico-symbols, xrce-dds-symbols,
  runner-packages (new, RFC-0119 D6), command-packages (new, RFC-0119 D7),
  adding-a-board-to-the-registry (the "How to add a new board" half of
  `supported-boards`)
- **reference/**: cli (user tier only, grouped as RFC-0117 D2), rust-api, c-api, cpp-api, rmw-api, platform-api,
  platform-differences, support-status, rmw-feature-matrix, sched-matrix,
  supported-boards, board-support-levels + `boards/*`, environment-variables
  (build-time and run-time tables), static-pool-inventory,
  configuration-surface, bridge-toml
- **migrating/**: retired-names, install-local-removal, esp32 (both retired
  chapters, folded into one row-set), retired-config-files

Removed from the TOC: `user-guide/embassy-integration.md` (its "use instead"
content becomes one row in `rtic` and `migrating/`), `user-guide/workflow.md`
and `workflow-by-platform.md` (merged into `targets/how-integration-works`
and `build/growing-your-workspace`; the contributor recipe column moves to
Internals).

Internals (`book-internals/src/`):

- **orientation/**: start-here (what the three doc series are, how to find
  things; replaces the router half of CLAUDE.md for humans), architecture
  (links ARCHITECTURE.md), repo-map, contributing
- **dev-env/**: build-commands (`reference/build-commands`), cli-dev-tier
  (`nros dev …`, RFC-0117 D3), sdk-tiers,
  ros2-on-non-ubuntu, versioning, ci-conventions, cli-in-monorepo
- **pipeline/**: build-system, canonical-build-path, codegen-packs,
  codegen-type-mapping, sizing-descriptor
- **rmw/**: backend-host-language-policy, zpico-build, zenoh-protocol,
  rmw-vs-upstream (full ledger), rmw-api-comparison (+ Reproduce),
  cyclonedds-fork-delta
- **platform/**: opaque-storage-sizing, platform-sync-abi,
  implementation-notes, board-cmake-schema, board-registry (RFC-0114: rows,
  generator, gate), platform-lanes, zephyr-in-tree,
  nuttx-in-tree, zephyr-upstreaming
- **realtime/**: dispatch ABI history, running the RT tests, wcet-baselines
- **testing/**: test-harness, creating-examples, verification (+ merge
  `docs/guides/verus-verification`), qemu-patched-binary, qemu-icount,
  freertos-lan9118-debugging (one copy), book probes
- **release/**: release pipeline (from `reference/cli`), changelog fragments

## Changelog

- 2026-11 — created (Draft). Classification and counts taken on `main` 8fdb89b.
- 2026-11 — D7: cross-book links are relative between deploy roots (found
  by the phase-532 W0 trial); scripts reach pages through the map (`book_pages.page_file`).
- 2026-11 — renumbered from 0121 (taken upstream the same day by
  workspace-first-project-layout). D9 and Appendix A follow RFC-0121: every
  project is a workspace, and the four workspace pages become one.
- 2026-11 — D9 added: the book follows the CLI tiers, the target model, `nros configure` and the board registry.
- 2026-11 — D2 amended by RFC-0124: the user book's parts follow the reader's task, not the CLI groups.
- 2026-11 — D4 amended and D3, D8, D9 consolidated by RFC-0124 (the book charter).
