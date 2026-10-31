# Phase 522 — The board support registry becomes the one primitive

**Status (2026-11-01). IN PROGRESS: W1, W2, W3 and W4 landed.** Implements
[RFC-0114](../design/0114-board-support-registry-primitives.md) (D10 support
levels, D11 derived and inherited breadth, D12 one registry, everything else
generated). Amends the tier section of
[RFC-0064](../design/0064-board-support-organization.md). Written from the
adoption discussion of 2026-10-31; the three adoption purposes (industrial,
field solutions, education) are the readers of the evidence page this phase
produces.

---

## Why

Board support is typed in five places today: `packages/boards/board-support.toml`
(the tier), `packages/testing/nros-tests/src/matrix.rs` (the cells),
`examples/fixtures.toml` (the builds), the probe workflow plus its case table
(the weekly replay) and the `ROWS` table of the issue 2169 test (the printed
run command). Each has its own drift gate, each gate was written after a
drift, and none of them agrees with the others about reach. The tier numbers
on top of that encode cadence (every merge versus nightly) while reading as
quality, and cannot say whether a board runs on a host, in an emulator or on
silicon.

RFC-0114 makes the registry the only place a board fact is typed and derives
the rest. This phase does the migration, in an order that proves each step
before the next relies on it: round-trip first, then gates, then the book,
then the CLI.

## Scope

In: the registry schema, the generator, the gate consolidation, the book
pages, `nros board list` and `nros doctor --report`. Every row that exists
today is mapped; one new row (the S32K344 safety island) is added because the
evidence for it already exists in phase-474.

Out: new boards (phase-524 adds the Nucleo, Arduino and Pico rows after W1
lands), RFC-0112's framework tiers (another session's phase-521), the
contents of the book chapters (phase-523), and the build descriptors
`nros-board.toml` (RFC-0049 and RFC-0064 own them; the generator reads them
and never writes them).

## Dependencies

- Blocks phase-524 W3 (board rows for the emulator twins): a row cannot be
  added in the new shape before W1 defines it.
- Independent of phase-523 (adoption front doors) and of the education and
  adoption-probe phase; those touch CLI verbs and chapters, not the registry.
- Issue 2177's branch (PR 2920) moves `nros-board-mps2-an385-zephyr` and
  `nros-board-qemu-cortex-a53` from scaffold to infra. W1 is written against
  main after that lands; if it has not, W1 carries the same move.

## Work items

Each item ends in a decision step: the evidence needed to call it done, and
what happens if the evidence says no.

### W1 — Registry schema and migration of every row

- Add the fields of RFC-0114 D10 (`runs`, `emulator`, `hardware`, `cadence`,
  `last_run`, `infra`) and D11 (`breadth`) to every `[[board]]` row, keeping
  `tier` and `execution_class` in place for this item so the old gate still
  passes. Add the top-level `manual_stale_days = 120`.
- Map every current row per the RFC's migration table. Add the S32K344 island
  row (`runs = "hardware"`, `hardware = "mr-canhubk344"`, `cadence =
  "manual"`, `last_run` from phase-474's last measured boot).
- Make the ZephyrQemuCortexM row's package `nros-board-mps2-an385-zephyr`,
  the descriptor the witness builds with, and drop the scaffold row of the
  same name.
- Extend `scripts/check-board-tiers.py` to validate the new fields alongside
  the old ones (the D10 predicates), so both readings are checked during the
  migration.

Acceptance: the `check-board-tiers` gate green with both field sets; every row has
both; a loader round-trips the file.

Decision step: review the migration table against the rows with the
maintainers. Any row whose new status word surprises a maintainer is fixed
in the registry before W2 starts. If more than two rows need a status word
D10 does not have, amend the RFC first.

**Result (2026-11-01).** Every row carries both field sets;
`scripts/lib/board_registry.py` holds the D10 shape and evidence predicates
and the one status-word derivation, `check-board-tiers` runs them beside the
tier predicates and refuses a row whose two readings disagree, and
`python3 scripts/check-board-tiers.py --print-status` prints the derived word
per row. Applied without a live review (the maintainer was not available), so
these are the decision step's assumptions, each a registry or predicate fix
rather than a new status word:

- `nros-board-mps3-an536-freertos` reads **Builds**, not the table's "Not
  supported": two fixture rows build it (`NROS_BOARD = "mps3-an536-freertos"`),
  and a fixture row naming the board counts as build evidence, which is also
  what gives `nros-board-s32z270-freertos` the table's **Builds** (it has no
  `matrix_platform`, so no cell can).
- The S32K344 island row inherits **ZephyrQemuCortexM**, not FreertosMps2:
  phase-474 records a Zephyr image. Its package is downstream, so the row says
  `external = "simple-autoware-safety-island"`; `last_run = "2026-10-26"`
  (phase-474, "The board halves, 2026-10-26"). It is grandfathered in the
  maintainer baseline at tier 3 pending a named owner.
- `FreertosPosix`'s `nightly_token` is `freertos`, the `just` module that runs
  it; `freertos_posix` named no nightly module, which `cadence = "daily"` asks.
- A `manual` row needs no Runtime cell (its evidence is the dated run), and a
  `runs = "none"` row may omit `breadth` (what `nros board new` writes).
- `cadence = "merge"` is checked as "host executable plus a swept nightly
  module", the old tier-1 rule, until W2 generates the lane's coordinates.

### W2 — The generator, proven by a byte-identical round-trip

- Write `scripts/gen-board-support.py`. Inputs: the registry and the
  descriptors it names. Outputs, each with a stamp line naming the generator
  and the registry hash: the `matrix::CELLS` region, the `interop::CELLS`
  smoke rows, the emulator `[[fixture]]` rows, one JSON probe matrix the
  workflow and `run-bootstrap-probe.sh` both read, the 2169 `ROWS` include,
  `book/src/reference/board-support-levels.md`, one
  `book/src/reference/boards/<name>.md` per row, and the registry-backed
  rows of `supported-boards.md`.
- First milestone, before any content change: regenerate today's cells,
  fixture rows, probe matrix and `ROWS` from the W1 registry and get an empty
  `git diff`. Family feature tables are added to the registry only as far
  as that round-trip needs them.
- Fixture rows whose build arguments are not a function of descriptor plus
  family default go on an allowlist file the generator reads; the generator
  checks them (coordinate present, locator port unique) and does not write
  them.

Acceptance: `gen-board-support.py --write` on the migrated tree produces no
diff; the allowlist is enumerated with a reason per row; the book pages
render under `just book`.

Decision step: if the allowlist holds more rows than the derived set and the
reasons show no path to shrinking (cmake `cmake_defs` and per-row `env` that
no descriptor field explains), apply RFC-0114's narrowing: fixture rows stay
authored and checked, and the RFC is amended. The cells, probe matrix, ROWS
and book pages are generated regardless of that outcome.

**Result (2026-11-01).** `scripts/gen-board-support.py` writes the
`matrix::CELLS` list (`packages/testing/nros-tests/src/matrix_cells.rs`, from
the registry's cell table `packages/boards/board-support-cells.toml`), the
probe track list `scripts/probe/probe-tracks.json` (read by
`run-bootstrap-probe.sh` through `scripts/probe/probe-track.py` and by
`probe-platforms.yml`'s matrix), issue 2169's `ROWS`
(`packages/cli/nros-cli-core/src/cmd/probe_boot_rows.rs`), the levels page,
one evidence page per board row, and the generated regions of
`supported-boards.md` and `SUMMARY.md`. Round-trip: the 263 cells the
generator writes equal, in order, the 263 hand-written ones on main
(coordinates, status, configurations and comment block; only alignment
whitespace differs), the generated `ROWS` are the eight hand-written tuples,
the probe tracks are the sixteen case-table entries and the fifteen workflow
entries, and a second `--write` produces no diff. `--check` runs inside the
`board-tiers` gate until W3 replaces it.

Decision step: the fixture-row allowlist would hold more rows than the
derived set (146 emulator rows, 68 with per-row `cmake_defs`, `env` or
`features` that no descriptor explains, and no family default to derive the
rest from), so RFC-0114's narrowing applies: fixture rows stay authored and
are checked, not written, and the RFC records the amendment. The
`interop::CELLS` smoke rows stay authored for the same reason.

### W3 — One gate, `check-board-registry`

- Replace `check-board-tiers`, `check-book-board-run-sections`, the 2169
  `ROWS` coverage test, `matrix_fixture_coverage.rs` G1–G4 and the probe
  case-list drift check with one gate, `check-board-registry`, a new recipe
  under `mod check`: regenerate into a
  temporary directory, fail on any byte difference, and refuse a board fact
  typed outside the registry where the layout lets it be seen (a `cell(...)`
  outside the generated region, a probe matrix entry with no row, a `ROWS`
  tuple with no row).
- Keep the retired gates' negative controls as the new gate's self-test:
  each mutation they caught must still be caught, by name.
- Put the gate on the fast line; it reads files and runs no build.

Acceptance: the mutation list passes (RFC-0114 evaluation step 2); the five
retired gates are deleted from `just/check.just` and
`check-default-gates-run-somewhere` is green; fast-line time grows by less
than two seconds.

Decision step: a mutation the gate cannot see is either made impossible by
layout (the generated region becomes the whole file) or printed by the gate
as a known gap. No silent gap.

**Result (2026-11-01).** `just check board-registry`
(`scripts/check-board-registry.py`) is on the fast line and replaced the
`board-tiers` and `book-board-run-sections` recipes. It regenerates every
view into a temporary directory and compares bytes, runs the predicates of
`check-board-tiers.py` and `check-book-board-run-sections.py` (each with its
own negative controls, still run on every call), and refuses a board fact
typed elsewhere: a `cell(...)` in matrix.rs, a hand track list in
`probe-platforms.yml` or `run-bootstrap-probe.sh` (the probe case-list drift
is now impossible by layout), a ROWS tuple in issue 2169's test, a `just
probe` recipe with no track, a QEMU `probe-run` block no track covers (the
static half of the 2169 test), and a fixture row whose `NROS_BOARD` names no
descriptor. Mutation list on the real tree, each caught with a message naming
what to edit: a `cell(...)` in matrix.rs, a probe matrix entry with no row, a
changed ROWS tuple, a track typed in the harness, a hand edit to the generated
cell list, and a registry row losing `runs`. Fast-line cost: the old pair took
155 ms sequentially and the new recipe takes 118 ms, so the line is 37 ms
cheaper.

Decision step, gaps the gate prints on every run instead of hiding: fixture
build arguments are authored (W2's narrowing) and only their board is
checked; `interop::CELLS` stays authored and keeps its own Rust gates; a
fixture locator port may repeat, because a talker and its listener share one
router (48 locators over 96 rows). Two of the five gates the phase doc named
are therefore not deleted: `matrix_fixture_coverage.rs` G1-G4 hold
`interop::CELLS`, which the narrowing keeps authored, so they stay; and issue
2169's Rust test keeps its comparison half (it needs `nros build`'s printed
command), while its coverage half is now also refused statically here.

### W4 — The book reads the registry

- `board-support-tiers.md` is replaced by the generated
  `board-support-levels.md`; `SUMMARY.md` and every link follow
  (`check-doc-refs`, `check-markdown-links`).
- The per-board evidence pages are linked from each getting-started chapter's
  board line and from `support-status.md`.
- The words "tier 1", "tier 2", "tier 3" leave every user-facing page; a
  grep of `book/src` for `[Tt]ier [123]` finds only RFC-0112's framework
  tiers, which stay until that RFC is revised.
- Remove `tier` and `execution_class` from the registry and the old
  predicates from the gate; the migration is complete.

Acceptance: `just book` builds; the generated levels page matches the W1
migration table row for row; the RFC-0114 evaluation step 5 (manual-stale
degradation on the FVP row) is demonstrated in both directions.

Decision step: the page is reviewed against the migration table by the
maintainers before the old page is deleted. A surprise is a registry fix or
a predicate fix, never a hand edit of a generated page.

**Result (2026-11-01).** `board-support-tiers.md` and its generator are
deleted; `board-support-levels.md` replaces them in `SUMMARY.md` and every
link. Each getting-started chapter names its boards' evidence pages in a
"Support level" line, and `support-status.md` links the levels page. A grep
of `book/src` for `[Tt]ier [123]` finds nothing: the board tier words became
evidence words, and the CI lane tiers of the contributor pages read "the
tier-1 lane". `tier` and `execution_class` are gone from the registry and the
predicates: `check-board-tiers.py` now checks the primitives and keys the
maintainer rule on the derived level (Verified needs 2, Boots or Builds 1),
with the baseline re-keyed to levels; `ci_lane`'s tier-1 pool is compared
with the `cadence = "merge"` rows (`--print-cadence`); the "explicitly not
supported" exemptions of issue 1305's walk and `check-managed-msg-deps` read
the derived "Not supported" from the new generated
`packages/boards/board-support-status.json`; `tier-health.py` derives its
lane tier from `cadence`. The levels page matches the W1 migration table row
for row. Evaluation step 5, both directions: on 2026-11-01 the FVP row reads
"Boots, manual (2026-09-06)", and `check-board-tiers.py --print-status
--today 2027-02-01` (or `gen-board-support.py --today 2027-02-01 --out DIR`)
reads "Builds (last booted 2026-09-06)".

Decision step: the generated views evaluate staleness on no clock, because a
view that changed on a day nobody edited anything would turn the byte gate
red for every pull request after day 120 (issue 1226's uniformly-red lane).
They print the date the degradation starts ("reads *Builds* from
2027-01-05") instead, and the live readers (`--print-status`, and W5's
`nros board`) apply the real date. `nros-board-mps3-an536-freertos` reads
Builds, a level that owes a maintainer where its old scaffold row owed none,
so it joins the maintainer baseline at `builds`; the maintainer must name an
owner for it, as for the S32K344 island.

### W5 — The CLI reads the registry

- `nros board list` prints the status word and cadence beside each board it
  already lists; `nros board show <name>` prints the evidence page's facts.
- `nros doctor --report` prints the registry status of every board the
  workspace's images name, so an industrial adopter has one command whose
  output they can file.
- The CLI reads the registry through the SDK root ladder (RFC-0099 D3), so a
  released toolchain carries its own copy and an installed user sees the
  same words as the book.

Acceptance: `board_key_table.rs` and the `nros board` tests cover the new
columns; the installed probe track prints a status word for its board;
`check-user-surface-ids` (phase-523 W1), once it exists, finds no tier
number in the output.

Decision step: if the installed toolchain's copy of the registry and the
checkout's disagree (a released `nros` against a newer checkout), the CLI
prints both and says which it read; it never picks silently.

## Acceptance for the phase

- No board fact is typed outside `board-support.toml` and the descriptors it
  names; `check-board-registry` holds that on the fast line.
- Every row of the migration table is present with its status word, and the
  generated levels page shows it.
- The five retired gates are gone and their mutation lists are the new
  gate's self-test.
- phase-524 can add a row in the new shape with one registry edit plus a
  descriptor, and the cells, probe track, ROWS entry and book pages appear
  from the generator.

## Left

- A downstream board package's own registry row (RFC-0114 open question);
  not needed until an out-of-tree BSP asks for it.
- Measured numbers per board (time to first output, install minutes) on the
  evidence page: the adoption front-doors phase decides whether they live
  here or in the probe records.
