# Phase 532 — Two books by audience: the nano-ros Book and nano-ros Internals

**Status (2026-11-05). DONE except what waits on other phases.** W0 to W10
are done: every page-map row has run (`book-move.py --plan`: nothing
pending), the book merged in #3093 and is published, with follow-ups #3131
(RFC-0122's example layout) and #3132 (issue 2275: ROS sourced before the
first build). Left, owned elsewhere: the `awaits` lines on 17 probe tracks
lift when phase-531 W4 lands; the example pointers settle when phase-535
moves the pairs; the pinned tracks wait for a release with `nros configure`;
issue 2278 decides the CMake road's bundled fallback. The user book's reorganization as a user guide continues in [phase-537](phase-537-book-user-guide-organization.md) (RFC-0124). Implements
[RFC-0123](../design/0123-two-books-by-audience.md) D1 to D9. Amends the
scope of [RFC-0116](../design/0116-adoption-purposes-and-front-doors.md) D2's
gate (W3).

This phase owns the books' **structure**: which book a page is in, its
directory and part, the Internals book, the moves, the merges and the gates.
It does **not** own the content of chapters that other open phases are
writing. Those phases keep their chapters, at the new paths:

| Chapter content | Owner |
| --- | --- |
| The serial-first first chapter | [phase-525](archived/phase-525-adoption-front-doors.md) W4, with [phase-531](phase-531-workspace-first-project-layout.md) W8 |
| The workspace-first rewrite: getting-started chapters, the four workspace pages merged into one, the platform chapters, `porting/*` | [phase-531](phase-531-workspace-first-project-layout.md) W8 |
| `nros run` and the run-plan pages | [phase-528](phase-528-run-on-host-and-emulator.md) W4 |
| `nros flash`, `monitor` and `bench` pages | [phase-529](archived/phase-529-flash-monitor-bench.md) W3, W4 |
| Renode and Wokwi twin chapters | [phase-524](phase-524-emulator-twins.md) W4b |
| Old CLI spellings in the tree's docs | [phase-526](archived/phase-526-cli-two-tier-restructure.md) W4 |
| `nros configure` vocabulary in docs | [phase-530](phase-530-nros-configure-single-pass.md) W8 |
| Runner and command package docs | [phase-527](phase-527-cli-extension-packages.md) W7 |

To avoid conflicts, every move is driven by the page map (W0). An in-flight
branch rebases over a move by running `scripts/book-move.py --rewrite`, which
applies the map to its own diff (W0).

---

## Why

RFC-0123's problem section has the measurements. In short: user prose holds
~280 internal ids outside `getting-started/`; user chapters tell readers to
run `just` recipes and `scripts/`; directory, SUMMARY part and audience
disagree on ~40 pages; and the gates that keep the user surface clean read
only a list of directories plus a "contributor" heuristic. The CLI
(RFC-0117/0118/0120) and the board registry (RFC-0114) have been redesigned
since most chapters were written, so the book's parts no longer follow the
tool's own groups.

## Work items

### W0 — The page map and its tools — DONE (2026-11-03)

Landed: `book/page-map.toml` (161 rows: 143 existing pages, including the two
gitignored generated ones, and 18 new pages), `scripts/lib/book_pages.py`,
`scripts/book-move.py` (`--plan`, `--apply`, `--rewrite`, `--summary`) and
`check-book-page-map` on the fast line. Negative cases: a stray page and a
row outside a part directory both fail it.

Trial (`--apply w2` on a throwaway branch, then discarded):

- 17 pages moved, 73 links repaired, and path literals rewritten in 30 files;
- `mdbook build` succeeds for both books;
- all 31 `just/check/docs.just` gates are green.

Three things it found, fixed here:

- `check-book-links` did not read `book-internals/` or resolve cross-book
  links. It now does both (RFC-0123 D7, amended: links are relative between
  deploy roots).
- `check-markdown-links` would have double-checked the Internals book. It now
  leaves `book-internals/` to `check-book-links`, as it already did for `book/`.
- Six generators built their output path as `os.path.join(ROOT, "book",
  "src", …)`, which no literal rewrite can follow. They now ask
  `book_pages.page_file(<seeded path>)`, so moving a generated page is a map
  edit. `gen-rmw-api-comparison` is in the W2 batch.

`book-move.py` repairs any broken relative link from the page's old location,
not only `.md` links. The trial's last dead link was a directory link.


- Write `book/page-map.toml`, one row per page of either book:
  `old` (today's path, or none for a new page), `book` (`user` | `internals`),
  `path` (the new path), `action` (`keep` | `move` | `merge-into <path>` |
  `split <internals-path>` | `retire` | `new <owner phase>`), and an optional
  `note`. Seed it from RFC-0123 Appendix A.
- Write `scripts/lib/book_pages.py`. It reads the map and answers "which book
  and directory is this page in", "what is the new path for this old path"
  and "list the user-book pages". From W2 on, gates and generators read it
  instead of hardcoding globs. Today 63 files name `book/src/…` paths.
- Write `scripts/book-move.py`, with three modes:
  - `--plan`: print what the next batch moves;
  - `--apply <batch>`: `git mv` the pages, rewrite relative links inside both
    books, rewrite every `book/src/…` literal the tree names (scripts,
    `just/*.just`, workflows, `CLAUDE.md`, `AGENTS.md`, `docs/`), and add
    the `[output.html.redirect]` rows;
  - `--rewrite`: apply the map to the working tree of another branch.
- Write `scripts/check-book-page-map.py` (W8 folds it into
  `check-book-audience`). It checks that every `.md` under `book/src` is
  exactly one row's `old` or `path`, and that every row's target directory is
  a D2 directory for its book.
- **Acceptance:**
  - every one of today's 142 pages has a row, and the check passes;
  - `book-move.py --plan` prints the W2 batch;
  - `--apply` on a scratch branch builds both books with `check-book-links`
    green.

### W1 — Scaffold nano-ros Internals — DONE (2026-11-03)

Landed with W2 in one commit, because the Orientation pages link to W2's
pages:

- `book-internals/` with Orientation (start-here, architecture, repo-map).
  The architecture and repo-map pages point to ARCHITECTURE.md and RFC-0001's
  directory map rather than copying them.
- `just book` and `docs.yml` build Internals after the user book and stage it
  at `internals/` (copied, because mdBook clears its build dir).
- `just book-internals-serve` previews it on port 3001.
- `check-book-links` reads both books (W0). The pointers in `CLAUDE.md` and
  `AGENTS.md` now name both books.
- There is no mermaid preprocessor yet, because no Internals page draws one.

Verified: both books build, cross-book links render and resolve, and the
staged `internals/index.html` is "Start Here".


- Create `book-internals/` (`book.toml` with the same mermaid preprocessor and
  a pinned mdBook, plus a `SUMMARY.md` with the RFC-0123 D2 parts as stubs).
- Make `just book` build both books. In `docs.yml`, deploy the user book at
  the site root and Internals at `/internals/`.
- Make `check-book-links` resolve site-relative links to `/internals/…`
  against `book-internals/src`, and the reverse.
- Write the Orientation part (start-here, architecture, repo-map,
  contributing) from CLAUDE.md's router table, `docs/design/README.md` and
  `internals/contributing.md`, for human readers.
- **Acceptance:**
  - both books build locally and in the docs workflow's dry run;
  - one cross-book link in each direction is checked.

### W2 — Move contributor-only pages into Internals — DONE (2026-11-03)

`book-move.py --apply w2` moved 17 pages, repaired 74 links and rewrote path
literals in 30 files, including `gen-rmw-api-comparison` through the map, the
bootstrap probe's path to `contributing.md`, and the `doc-recipe-refs`
baseline row. Old URLs redirect. All docs gates are green.

**SUMMARY.md is generated from now on.** `--apply` rebuilt both TOCs from the
map, so the user book already reads in RFC-0123 D2's part order. Its pages
stay at their old paths until W4, and a page waiting for a W5 merge is nested
under its target. To add or move a page, edit the map and run
`book-move.py --summary`. `check-book-page-map` fails on drift (tested both
ways). Titles stay authored in SUMMARY, and the board generator's region
passes through unchanged.


These are the pages RFC-0123 Appendix A lists for Internals:

- `internals/` rows marked D: codegen-packs, rmw-backends, the LAN9118
  debugging page, qemu-patched-binary, opaque-storage-sizing, build-system,
  cli-in-monorepo, zpico-build, verification, creating-examples,
  platform-lanes, zephyr-in-tree, nuttx-in-tree, contributing;
- `reference/build-commands.md`.

The user pages that link into them change to cross-book links. Run the
scripts that hardcode these paths through `book_pages.py`.

- **Acceptance:**
  - no page in the user book's `internals/` is contributor-only;
  - `check-book-audience` (W8; until then, `check-book-page-map`) passes;
  - redirects are in place.

### W3 — Split the mixed pages; widen the gates; delete the exemption — DONE (2026-11-03)

**Gates.** `check-user-surface-ids`, `check-book-no-just` and
`check-book-no-checkout` read every user-book page through
`book_pages.tracked_user_pages()`, which is 124 pages. `book_contributor`
keeps only the verbatim escape (`no-just-ok`), and the word "contributor"
exempts nothing. `## Design background` licenses RFC ids and nothing else,
with self-tests in both directions. RFC-0116 D2 is amended. Measured before
the sweep: 388 ids on 73 pages, 42 `just` on 20 pages, and 11 checkout steps
on 6 pages. After the sweep all three are zero.

**Generated pages were fixed at their source:**

- seven board-registry `notes` that render on board pages were rewritten as
  user facts, with the evidence moved to TOML comments above each row;
- the templates of `gen-support-status`, `gen-sched-matrix`,
  `gen-rmw-feature-matrix` and `gen-config-surface` were swept. Facts were
  stated from the records, e.g. phase-303 became "a default Jazzy peer is
  XCDR1/FINAL".

**Content.** 67 user pages were swept, and about 35 now end in a
`## Design background` section. Six Internals pages were created:
`rmw/rmw-vs-upstream-ledger`, `dev-env/cli-dev-tier`, `testing/test-failures`,
`realtime/dispatch-abi-history`, `realtime/running-rt-tests` and
`realtime/mem-report` (one new map row). Five existing Internals pages
received the sections moved out of user pages. Four facts changed while ids
were removed, and each was checked against its record: issue 1589 resolved,
phase-445 complete, issue 1499 wontfix, and `nros-board-common::board_init`
gone. `deployment.md`'s "no `nros run`/`flash`, none planned" was corrected to
RFC-0118.

**Fixed along the way.** Until W4, the user book's `internals/` pages share the
`internals/` URL prefix with the Internals book. `check-book-links` and
`book-move` now try both books for a URL there; before, they misread four
links from Internals back to those pages.

**Not closed here. Carried to the item that owns each:**

- **W9, no gate yet:**
  - user pages still teach `nros dev …` (`sdk root`/`sdk path`,
    `image model-path`, `metadata`, `rmw session-properties`);
  - `nros sync` stays until `nros configure` ships (phase-530 W1; RFC-0123
    D9's "pages wait for the verb");
  - `reference/cli.md` still lists `store`, `generate-px4-msgs` and
    `toolchain manifest`;
  - `check-book-no-checkout`'s own hint recommends `nros dev sdk root`.
- **W9, checkout flavour the gates do not catch:**
  - `board-crate-import.md` runs `scripts/zephyr/resolve-fvp-bin.sh` in a fence
    (not a pattern the gate knows);
  - `freertos-board.md` names `integrations/s32ds/`;
  - `configuration.md` mentions the `.env` that `just` loads;
  - `console-less-boards.md` points at `scripts/read-boot-report.py`;
  - `platform-porting-pitfalls.md` has a "Test Infrastructure" section.
- **W9, front-door gaps** (a user step with no `nros` spelling): the mem-report
  tool and the boot-report reader. Each gets a front-door issue, or moves to
  Internals.
- **W6:** `migration-install-local-removal.md` still calls the deleted
  `integrations/nano-ros/` the live ESP-IDF component.
- **Generated, gitignored pages are outside the gates' subject.**
  `static-pool-inventory.md` carries 32 ids, because `gen-pool-inventory` copies
  `build.rs` comments verbatim. Its fix is a design choice (a user-facing
  description per knob, or a scrub). The gates also need to generate these
  pages first, as `book-links` already does. Both go with W8.


- For each page RFC-0123 marks *split*, move the contributor half to its
  Internals page and leave at most one link line. The split pages are:
  - integration-zephyr, troubleshooting, workflow-by-platform;
  - rmw-backends, message-generation, configuration, component-and-entry-pkg;
  - deployment, rtic-integration;
  - two-layer-api, platform-model, no-std;
  - the cli reference, rt-execution-model, dispatch-strategy;
  - platform-porting-pitfalls, rust-api, c-api.
- Replace phase, issue and PR ids in user prose with the fact they stand for.
  An RFC id stays only inside a `## Design background` section (RFC-0123 D3).
- Change the scope of `check-user-surface-ids`, `check-book-no-just` and
  `check-book-no-checkout` to the user book from `book_pages.py`. Teach
  `check-user-surface-ids` the `## Design background` allowance.
- Delete the contributor exemption from `scripts/lib/book_contributor.py`
  and keep only the `no-just-ok` verbatim escape. Amend RFC-0116 D2's
  "Gate:" paragraph in the same commit.
- Each user-chapter `scripts/…` call either becomes an `nros` verb (file a
  front-door issue if the verb is missing) or moves to Internals.
- **Acceptance:** all three gates read the whole user book with a baseline of
  zero, `book_contributor`'s heuristic is gone, and RFC-0116 D2 is amended.

### W4 — Re-lay the user book into its part directories

**Progress (2026-11-03).**

- **Batches done:** `w4-start`, `w4-communicate`, `w4-realtime`, `w4-operate`,
  `w4-from-ros2`, `w4-concepts` and `w4-extending`. `w4-reference` is all
  `keep`.
- **Result:**
  - 185 links repaired and path literals rewritten in 62 files;
  - every generator re-run;
  - all docs gates, the book gates and both `mdbook` builds are green;
  - the user book no longer has an `internals/` directory, so the shared
    `internals/` URL prefix of RFC-0123 D7 is gone.
- **Held batches run (2026-11-04).** The maintainer chose not to wait for
  phase-525 W4, phase-528 W4 and phase-531 W8: the book is written to their
  specs now and reviewed against them when they land. `w4-get-started`,
  `w4-build` and `w4-targets` moved 28 pages and repaired 260 links. The
  extracted probe scripts of five tracks are unchanged command for command.
  - `book-move` does not see the board registry's book-relative paths
    (`chapter`, `book_run`, `book_flash`); they were remapped through the
    map by hand. A future move needs the same, or `book-move` should learn
    those fields.
  - The first chapter is rewritten to RFC-0121 D4/D5 and RFC-0116 D5/D6:
    the mps2-an385 talker in QEMU over its serial link, `nros run <node>`,
    `ros2 topic echo`, plus a Linux section. Its blocks are `variant=qemu`
    (new weekly `first-project` probe track) and `variant=linux` (the
    `linux` track). The qemu track fails until phase-531 W4 ships the
    workspace scaffold and the node rung. `anatomy.md` and
    `first-10-minutes.md` (the `system.toml`-beside-code refusal) follow.

The batches found four defects in `book-move`, fixed here:

- **Cross-book links.** A `.html` link was read only from the page's old
  location, so it was kept while it still resolved there. Links are now read
  from where the page is first, then from where it was.
- **Generated pages.** Generators hardcode book links at their seeded paths,
  so regenerating undid the repair. Each generator now passes its output
  through `book_pages.relink(text, <seeded path>)`, which re-spells every
  mapped link from where the page lives now.
- **Inputs of generators.** The literal rewrite edits generator inputs too:
  the board registry names chapter paths, so its data hash and all 23 views
  moved. `--apply` now re-runs every generator in the map, not only those of
  moved pages.
- **Links from `docs/`.** Links into moved pages, archived records included,
  are repointed through the map (`repair_doc_links`). Archived dead links
  went from 412 to 411.
- **User-book stubs** say "coming soon" and name no phase. The extending
  stubs (runner and command packages, adding a board to the registry) state
  what the page will cover and point at what exists today.


- Run `book-move.py --apply` in batches, one per part, and rewrite
  `book/src/SUMMARY.md` into the RFC-0123 D2 parts and order.
- Rewrite the probe tags' paths in `just/probe.just` and
  `scripts/probe/*`, and the exempt paths in `check-retired-*`, through the
  map.
- **Sequencing:**
  - Run each batch when no open phase-524/525/528/529 PR touches that part's
    pages, or rebase those PRs with `--rewrite`.
  - The `get-started/` batch goes after phase-525 W4 merges.
  - The `targets/` batch goes after phase-528 W4.
- **Acceptance:**
  - every user page is in its part's directory;
  - `just book`, `check-book-links`, `book-probe-tracks` and
    `just probe bootstrap` are green;
  - the old URLs redirect.

### W5 — Merges: one topic, one page (RFC-0123 D4) — DONE (2026-11-04)

**Progress (2026-11-03).** Four merges landed:

- **`rmw-backends` into `communicate/choosing-an-rmw.md`.** It gains "How
  each backend works" and the Cyclone per-platform network section. Its
  unique selection material went to `switching-rmw`. Both pages' capability
  tables are replaced by a summary that links to the generated
  `rmw-feature-matrix`, which is now the only matrix.
- **`embassy-integration` into `realtime/rtic.md#embassy`.**
- **Troubleshooting.** `operate/troubleshooting` owns it. The per-topic
  sections in ros2-interop, serial-transport and bridges were already topic
  specific, and each now points to it.
- **ros2-interop and Internals `rmw/zenoh-protocol`** are split by D4's rule.
  The user page keeps the steps; protocol detail and implementation status go
  to Internals; each links the other.

Stale claims were corrected against the matrix and the records:

- Cyclone "has no status events" / "pub/sub-only": the matrix shows status
  events wired;
- XRCE "zero heap" is now "no per-message allocation";
- Iron/Jazzy type hashes are computed at codegen.

`book-move` now adds a redirect for every mapped page whose old file is gone,
merges included, and keeps hand-written rows that name an anchor.
`gen-rmw-feature-matrix`'s Related list lost its duplicate entry.

**The seven merges held for phase-531 W8 landed (2026-11-04).** They were
written to the RFC-0121 spec (every project a workspace, `system.toml` only in
a bringup, a generated launch file, the `nros run` target rungs) ahead of its
implementation, and are reviewed again when phase-531 lands.

- **`workspace-node-pkgs`, `workspace-bringup`, `workspace-entry-pkg` and
  `workflow` into `build/growing-your-workspace.md`.** The page is now W8's one
  path from the first chapter's `sensor_ws`: add a node package (`nros new
  <pkg> --to <bringup>`), add its component row, add `launch/` when a launch
  argument is needed, add an image. It keeps the node API essentials
  (`ENTITY_BOUNDS`, timers, client calls in `tick`), the launch tag set and
  parameter files, the image keys, the target rungs, Zephyr Kconfig in the
  bringup, `before_boot` and `nros materialize`, and the ROS 2 command map.
  Dropped as retired: the single-package leaf shape and its plain `cargo build`
  flow, `nros new --workspace` and `--platform` scaffolds, the
  `<pkg>_nros_selection` crate, `nros new entry` and hand-written entry
  packages, the entry-package migration table, the `nros::main!` form list
  (in `roles.md`), the `nros plan` sidecar caveat, and the install steps
  (in `installation.md`).
- **`workspace-languages` into `build/mixed-language.md`.** Per-language
  notes (Rust, C, the RMW spelled once) and `nros new … --to` scaffolding
  replace the `--component` / `nros new system` spellings. The C node's
  `DEPLOY` argument is gone, as in the example.
- **`workflow-by-platform` into `targets/how-integration-works.md`.** It gains
  "When `nros build` drives the build" (which host tool each image uses,
  `--dry-run`, the west follow-up) and the per-platform table (`#per-platform`).
  The CI builder-per-grid-row table went to Internals
  `platform/platform-lanes.md`. Dropped as retired: the leaf `nros sync`
  failure modes, the plain `cargo build --config` and `cmake --preset` leaf
  escape hatches, and "where a leaf comes from".
- **`integration-px4` into `targets/px4.md`.** It adds the rename-in-three-places
  rule, the no-registry note and the register-check reference; the rest was
  already there.

Links into the merged pages were repaired through the map and each old URL
redirects. Pages still teaching the leaf shape (the platform chapters,
`roles.md`, `cpp-workspaces.md`, `first-node-*`) are phase-531 W8's.


- workspace-languages → mixed-language. Also check what is left of
  component-and-entry-pkg against the node, bringup and image pages.
- integration-px4 → px4.
- troubleshooting-first-10-min stays as the first-run page. The per-chapter
  troubleshooting sections link into `operate/troubleshooting` instead of
  restating it.
- rmw-backends → choosing-an-rmw. `rmw-feature-matrix` becomes the only
  capability matrix.
- workflow and workflow-by-platform → how-integration-works plus
  growing-your-workspace.
- workspace-node-pkgs, workspace-bringup and workspace-entry-pkg →
  growing-your-workspace. That content merge is phase-531 W8's rewrite; this
  phase's map only names the target.
- ros2-interop (what to do) and Internals rmw/zenoh-protocol (how it works)
  are split by D4's rule.
- **Acceptance:** each merged-away page is a redirect, and no capability
  matrix exists outside `rmw-feature-matrix` and the generated board pages.

### W6 — `migrating/` and retired material (RFC-0123 D5) — DONE (2026-11-03)

Landed:

- `migrating/` holds retired-names, install-local-removal,
  retired-config-files and esp32.
- The two ESP32 chapters are folded into `migrating/esp32.md`, which covers
  both retirements: the esp-hal board and the ESP-IDF component with its
  `idf.py` driver.
- "Retired files" moved out of `configuration.md`.
- `install-local-removal.md` no longer tells users to add the deleted
  ESP-IDF component.

ESP32 sections and claims are gone from build-artifacts (which had a
deleted `idf.py` driver row), deployment, build-profiling, the introduction
(whose board table said the retired ESP32-C3 QEMU board was "Verified"),
choose-by-purpose, porting-pitfalls (its errno lesson is kept, under
"Bare-metal RISC-V"), platform-differences, environment-variables, logging,
platform-model, board-integration, platform-c-abi, the from-ros2 pages, and
supported-boards (now a single "Retired" row).

**Acceptance as met.** The plan said that `grep -ri esp32` would hit only
`migrating/` and generated pages. That turned out too strict: what must go is
ESP32 *instructions and support claims*. Mentions that remain are one of
three kinds:

- retired pointers;
- ESP-IDF named in lists of vendor build systems;
- "no ESP-IDF binary channel".

**Held.** Pages that phase-531 W8 rewrites keep their mentions until then:

- `getting-started/how-integration-works.md`, `workspace-*` and
  `first-node-rust.md`;
- `user-guide/component-and-entry-pkg.md` and `workflow.md`;
- the "If you already have entry packages" migration section of
  `workspace-entry-pkg.md`.

`embassy-integration` is folded by W5. The introduction's whole board table
is hand-typed status, so replacing it with the generated support page is W9
(RFC-0123 D9).


- Move retired-names, migration-install-local-removal, the two ESP32 chapters
  (folded into one page), the "Retired files" section of configuration and
  the "If you already have entry packages" section of workspace-entry-pkg
  into `migrating/`.
- Delete the ESP32 sections from build-artifacts, deployment and
  build-profiling. Fold embassy-integration into one row.
- **Acceptance:** `grep -ri esp32` over the user book hits only
  `migrating/` and generated pages.

### W7 — `docs/` consolidation (RFC-0123 D6) — DONE (2026-11-03)

**W7a: contributor documents.** `book-move` gained `import <repo path>`. The
document moves into its Internals page and its old path keeps a one-line
"moved" stub. Path literals follow it, and links to the stub are repointed
at the page.

- **Imported (19):**
  - `dev-env/`: four `docs/development` guides;
  - `pipeline/`: canonical-build-path and codegen-type-mapping;
  - `rmw/`: cyclonedds-fork-delta;
  - `platform/`: four pages (implementation notes, board CMake schema, Zephyr
    upstreaming and version support);
  - `testing/`: test-harness, qemu-icount, Verus, the four QEMU dev-setup
    guides, and the LAN9118 reference (a different document from the existing
    debugging page).
- **Collapsed to stubs:**
  - `docs/reference/environment-variables.md` had drifted: it still listed the
    retired `XRCE_*` spellings;
  - `docs/reference/std-alloc-requirements.md`;
  - `docs/reference/rmw_zenoh_interop.md`, after its TRANSIENT_LOCAL `@adv`
    section moved into Internals `rmw/zenoh-protocol`. Its "Test Scripts" named
    three scripts that no longer exist.
- **Fixed.** `docs/issues/README.md` is skipped by the literal rewrite, because
  its "Recently resolved" block is content-frozen (`check-issue-index`
  failed on it).

**W7b: user guides.** Each folds into its one user page, and the `docs/guides`
file becomes a stub:

- **`cpp-api` into `reference/cpp-api`.** The page now holds the full C++
  API. Its error-code table was wrong (NotInitialized -4 / Full -5), and the
  page now quotes `rclcpp/result.hpp`. The typed FFI and its pipeline went to
  Internals `platform/implementation-notes`.
- **`embedded-tuning` into `communicate/tx-tuning`** ("Transport buffers and
  limits") **and `realtime/static-memory`** ("Sizing the pools"). Facts were
  checked against the tree: the XRCE env names are `NROS_XRCE_*`,
  `STREAM_HISTORY` defaults to 16, and `Z_TRANSPORT_LEASE` is 60000 ms. Two
  knobs the guide called "yours" are now derived. The per-slot memory table
  was dropped: it described static arrays that `zpico.c` no longer has.
- **`realtime-lint-guide` into `realtime/rt-analysis`.** Only what was unique
  went in; the CI workflow example is rewritten for a user's own repository.
- **Scripts follow:**
  - `check-knob-ends` checks the two tuning pages instead of the stubs;
  - `check-cpp-no-nros-namespace` reads `book-internals/`;
  - `gen-config-surface`'s evidence text names the new page.

**Left in `docs/guides/`, deliberately:**

- the moved-stubs, kept for one release by D6, so the directory is deleted
  after that;
- `workspace-feature-slides.md` and `model1-execution-model-slides.html`,
  which are presentation records that predate workspace-first.

**Unverified:** the agent's note that `reference/environment-variables`
should list the POSIX frag/batch default as 65535, not 65536.


- Move the narrative pages of `docs/development/`, the unmigrated
  `docs/guides/*` and the explanatory pages of `docs/reference/` into
  Internals, and leave one-line "moved" stubs. Update CLAUDE.md's and
  AGENTS.md's pointers.
- Collapse `docs/reference/{environment-variables,std-alloc-requirements,
  rmw_zenoh_interop}.md` into their book page.
- Delete `docs/guides/` once it is empty.
- **Acceptance:** no full-text duplicate between `docs/` and either book, and
  `check-doc-refs` is green.

### W8 — `check-book-audience` on the fast line (RFC-0123 D8) — DONE (2026-11-03)

`scripts/check-book-audience.py` replaces the `book-page-map` recipe. It folds
`check-book-page-map`, and its self-test covers each of these three rules:

- **A. One URL, one book.** No user page sits under `book/src/internals/`.
- **B. No contributor material in the user book.** A "**Contributors**" lead
  or a heading naming contributors fails.
- **C. Generated, gitignored user pages.** `static-pool-inventory` and
  `configuration-surface` are rendered in memory (`--stdout`, new on both
  generators, so parallel gates never race on the file). They are held to the
  id, `just` and checkout rules.

Its first run found 14 contributor pointers that W3's gates could not see,
because they held no id and no recipe. Each page keeps at most one, as D3's
"Changing this in the tree: see Internals → …" line. The PX4 pointer named a
user page "contributor" and was removed, and so was the `just`-recipes
paragraph in workflow-by-platform.

**Pool inventory: 32 ids to 0.** `book_pages.scrub_ids()` drops citation-only
parentheticals (`(issue 1037)`, `(phase-474 I2)`, `since issue 1424`, a
leading `issue 1808:`) from text a generator copies out of the tree. The
source keeps the citation. Prose whose subject is an id was rewritten in the
template or in `scripts/pool-inventory-knobs.txt`, and `just mem-report` is
now "the memory report", linking to Static memory.

**Not covered by this gate:** "no page is in both books". Paths make that
true by construction, and rule A covers the only way it could fail (a shared
URL).


- Fold `check-book-page-map` into `check-book-audience`. It checks:
  - SUMMARY ↔ directory ↔ map agree, with no orphans;
  - no page is in both books;
  - there are no "**Contributors:**" markers in the user book.
- Add it to `just check fast`.
- **Acceptance:** baseline zero, and a page moved by hand without a map row
  fails the gate.

### W9 — Follow the CLI and the registry (RFC-0123 D9) — DONE (2026-11-03)

**Gates.** `check-book-audience` gains two rules:

- **D, the user tier.** No `nros dev …` on a user page, and no old spelling
  the CLI's alias tables still accept. The spellings are read from
  `user::ALIASES` and `dev::ALIASES`, so the gate keeps no list of its own.
- **E, board facts.** No tier number, no registry status word, and no legacy
  "Verified"/"Ready"/"Untested" table cell outside the generated pages and
  regions.

`migrating/` is exempt from both. The two "Tier-1" hits were the core subset
of status events, not a board tier, and are renamed "core".

**Probe fences.** A `probe=NN` fence runs in the clean-container probes, so
its command cannot change without re-running its track. Nine dev-tier steps
sit inside probe fences. They are counted against `PROBE_FENCE_BASELINE = 9`,
which can only fall, rather than exempted:

- `$(nros dev sdk path qemu)` six times (bare-metal, freertos ×2, nuttx ×2,
  threadx);
- `nros dev sdk manifest` in installation;
- `nros dev sdk path zephyr-sdk` and `nros dev sdk root` in integration-zephyr.

The QEMU six can become `nros locate tool qemu-system-<arch>` once their
probe tracks are re-run (`just probe <platform>`).

**Content** (one agent; every verb checked against the built `nros`):

- **CLI reference.** `reference/cli.md` is the user tier in `GROUPS` order.
  `flash`, `monitor` and `debug` are reserved and refuse; `run` and
  `bench scan|list` work. The dev tier, the old spellings and the removed dev
  verbs (with their replacements) are in Internals `dev-env/cli-dev-tier`.
- **Dev-tier steps replaced.** Raw CMake with `$(nros dev sdk root)` became
  `nros setup presets` with `cmake --preset`. Codegen became
  `nros interface generate` or `nros sync`.
- **Typed board statuses removed:**
  - the introduction's board table;
  - `supported-boards`' own table, now "Other boards and how you would
    approach them", with no status column;
  - arm-fvp, freertos and freertos-board.
- **"How to add a new board"** is now
  `extending/adding-a-board-to-the-registry`.
- **`start/choose-by-purpose`** is re-cut by the RFC-0116 D1 purposes.
- **W3 leftovers closed.** The checkout-only steps in board-crate-import,
  freertos-board, configuration, console-less-boards and porting-pitfalls are
  gone or moved to Internals.
- **Stubs** say what each page will cover and what to use today:
  `targets/{target-model,run,flash-and-monitor,bench,run-plans}` and
  `build/build-and-configure`.

**Front-door gaps.** Each needs a user verb, or a decision:

- ~~the SDK root~~ — closed: `nros locate nano-ros` (below);
- store tool directories other than QEMU (Zephyr SDK, Arm FVP);
- the SDK manifest;
- the boot-report reader;
- mem-report;
- QEMU `-nic` for a self-started QEMU;
- the census for a hand-run west/CMake build;
- NuttX apps staging;
- `nros new`'s scaffolds, which still write `nros dev sdk root` into
  generated files.

**SDK root — closed (2026-11-04).** `nros locate nano-ros` is the user verb
(RFC-0103 D6). It prints the root of the nano-ros the *current project* builds
with, and that is per project: a project's `nros-toolchain.toml` pins a
version, the launcher hands the call to that version's `nros`, and that
binary answers with its own root. Two projects pinned to 0.6.0 and 0.7.0 get
two different roots from the same shell. `--project <dir>` naming a project
pinned to a different version than the running binary is refused with a
message to run the command inside that project, so it never answers with the
wrong version's tree. The 13 hand-typed store paths are now
`"$(nros locate nano-ros)"`.

**First chapter — fixed (2026-11-04).** `getting-started/first-project.md`
now runs with `nros run` (rebuilds if stale, streams the console) instead of
the raw binary, adds `nros setup` to the Rust path, says where the SDK comes
from (`nros locate nano-ros --why`) and that the first build pins the
version. Its smoke test is a probe fence, `nros run --expect 'Received: 1'`.
`nros new --workspace`'s "Next steps" print the same flow. Walked by hand
from the in-tree CLI, C++ and Rust both pass. The workspace-first rewrite
(`nros new <ws> --board`, `nros run <node>`) stays with phase-525 W4 and
phase-531 W8.


- Sweep the user book for `nros dev …` and old spellings. This shares
  `check-cli-old-spellings` with phase-526 W4.
- Rewrite the user `reference/cli.md` as the user tier grouped by RFC-0117
  D2, and move the dev tier to Internals `dev-env/cli-dev-tier`.
- Remove board facts typed in prose ("tier N", status claims, copied run
  lines), so chapters link to the generated board pages.
- Move "How to add a new board" from supported-boards to
  `extending/adding-a-board-to-the-registry`.
- Re-cut `start/choose-by-purpose` along RFC-0116 D1.
- Add stub pages so the TOC already has its final shape:
  `build/build-and-configure`, `targets/target-model`, `targets/run`,
  `targets/flash-and-monitor`, `targets/bench`, `targets/run-plans`,
  `extending/runner-packages` and `extending/command-packages`. Each stub
  names its owner phase and what to use today. The owner phase fills it.
- Add a `check-book-board-facts` gate (or a rule in `check-book-board-names`):
  no `tier [0-9]` and no status word outside generated pages.
- **Acceptance:** both gates are green; every D9 stub exists and names its
  owner.

### W10 — Front matter — DONE (2026-11-04)

- Rewrite the introduction's "Key Features" as user benefits, and move the
  evidence prose (the `example_portability` test) to Internals.
- Split installation into a common path and a host-specific page, aiming at
  RFC-0116 D1's budgets (field: 30 min to `ros2 topic echo`; education:
  15-min install).
- **Acceptance:**
  - the installation page's common path is under 1,000 words;
  - `just probe bootstrap` and `just probe installed` are green.

**Done.**

- `start/introduction.md`: "Key Features" is now "What you get", eight user
  benefits with no evidence prose. The evidence (the `example_portability`
  test, the backends' allocation behaviour and `rmw-alloc-sites.py`, the
  Kani/Verus coverage) moved to Internals `orientation/architecture.md`,
  "What backs the Book's claims". "How This Book Is Organized" now lists the
  user book's parts and points contributors at Internals.
- `getting-started/installation.md` is the common path: host packages, the
  CLI install, `nros setup`, and whether ROS 2 is needed. It is 573 words,
  within the 1,000-word acceptance.
- The rest moved to a new page, `getting-started/installation-details.md`:
  what each host package is for, install options, provisioning in depth
  (the board table, whole-workspace provisioning, the RMW daemons, distro
  cross compilers), ROS 2 and the Zenoh router, pinning, using nano-ros
  outside a workspace, and migrating. Its map row is a `move` in the held
  `w4-get-started` batch, so it starts at its old path and moves with
  `installation.md`.
- The registry's `workspace`, `pinned` and `pinned-newer` probe tracks now
  name `installation-details.md`. The harness reads `installation.md` first
  on every installed track, as before. Extracted probe scripts for the
  `linux`, `workspace` and `pinned` tracks are command-for-command identical
  to those from before the split.
- Anchors that moved (`#provisioning-with-nros-setup`, `#pinning-a-version`,
  `#rust-only-consumers`) are repointed in both books and the README.

**Not run here:** `just probe bootstrap` and `just probe installed` need a
release asset and a container. The extraction check (`check-book-probe-tracks`)
is green, and since the extracted commands did not change, the weekly probe
run is the confirmation.


## Probes that await the CLI

The user book is written to RFC-0121 and RFC-0120 ahead of the CLI: a
workspace from `nros new <ws>`, `nros run <node>`, and no `nros sync` (the
maintainer's call, 2026-11-04). Until phase-530 W1 and phase-531 W4 land,
those chapters' `probe=` blocks cannot pass. Rather than let 18 tracks go red
on their first unknown flag, or skip them, each affected `[[probe]]` row
carries an `awaits` list. `run-bootstrap-probe.sh` fails such a track up front
with `[UNMET PRECONDITION] … it awaits: …`, before any asset build. The
checkout and zenoh tracks resolve the `linux` row, so they fail the same way.
`PROBE_EXTRACT_ONLY` still extracts every track, so the per-push drift check
stays live.

| Tracks | Await |
|---|---|
| first-project, linux, baremetal, offline, freertos, freertos-c, threadx-linux, threadx-riscv64, threadx-linux-c, nuttx-arm, nuttx-arm-rust, nuttx-riscv, nuttx-riscv-rust, zephyr, zephyr-rust, workspace | phase-531 W4 (workspace scaffold, node rung); phase-530 W1 landed in #3096 (2026-11-04) |
| pinned, pinned-newer | phase-530 W1 in a release the pinned run can pin: the pinned 0.6.0 release still needs `nros sync`, which the book no longer shows |
| renode-nucleo, wokwi-pico | nothing: their `zephyr-twin` blocks are unchanged |
| zephyr-west-app, zephyr-west-app-cpp, zephyr-west-app-flash | nothing: a west app needs neither phase |
| zephyr-west-app-rust | nothing since #3096: its `nros configure` step shipped with phase-530 W1 |

When phase-531 W4 lands (its owner will say so), delete the `awaits` lines,
regenerate (`scripts/gen-board-support.py`), and replay each track
(`just probe <track>`); fix the chapter where the shipped CLI differs from
the spec.

## Order

W0 → W1 → W2, then W3 and W8. W4 and W5 run batch by batch around the owner
phases. W6, W7, W9 and W10 can run in parallel after W4's batch for their
part.

## Out of scope

- The content of chapters owned by phases 524 to 530 (table above).
- Translations, and a versioned book per release.
