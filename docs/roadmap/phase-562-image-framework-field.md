# Phase 562 — An image's entry framework is its own field

**Status (2026-11-15). OPEN: W1, W2, W3, W4 and W8 done. Owner: nros 142.** Opened by
the user's ruling on phase-555's platform-board-rtos brief (P5 (a), 2026-11-14):
"open a phase for [RFC-0111](../design/0111-image-framework-axis.md)'s seven
steps, with [RFC-0112](../design/0112-framework-support-tiers.md) D5 as one of
its items and the RTIC dispatch-lint gap as an item." W1 to W7 are RFC-0111's
"Migration and work" steps, in its order; W8 is RFC-0112 D5; W9 is the lint
gap; W10 re-promotes RTIC to Tier 1.

Each item is one PR, claimed with `just claim phase-562-wN` before it starts.

## Why

`rtic-mps2-an385` is the MPS2-AN385 bare-metal board with the RTIC entry
framework chosen, and the choice rides in the board word. Four consumers
derive "which framework" from that string on their own (RFC-0111, "What the
word does today"). Measured on main on 2026-11-14:

- `framework_for_board_key` is still the resolver
  (`packages/core/nros-orchestration-ir/src/lib.rs:264`); there is no
  `image_framework`.
- No board descriptor carries `frameworks =`.
- `rtic-mps2-an385` appears in 37 tracked files outside `docs/`.
- `FRAMEWORKS` (`nros-orchestration-ir/src/lib.rs:164`) is
  `["owned-spin", "rtic", "zephyr", "px4"]`: `zephyr` is still a framework
  a user could name, which RFC-0112 D5 retires.
- The framework × dispatch lint in `nros ws check`
  (`packages/cli/nros-cli-core/src/cmd/check_workspace.rs`) reads the board
  CRATE's `[package.metadata.nros.board] framework`. The mps2-an385 crate
  deliberately does not declare one, so the RTIC-requires-Deferred rule is off
  for every RTIC image in the tree.

## Work items

### W1 — Schema

- **Owner:** nros 142.
- Add `framework` to the image table in both readers: `leaf_system` and
  `nros-cli-core`'s `deny_unknown_fields` `DeployBlock`. Add `frameworks` and
  `entry_types` to the board descriptor schema (RFC-0111 D1, D2).
- **Acceptance:** both readers accept `framework` and refuse an unknown key;
  a descriptor with no `frameworks` key reads as `["owned-spin"]`.
- **Status:** DONE (2026-11-15, branch `feat/phase-562-w1-schema`)

Landed: `ImageBlock::framework` (with the `[image_defaults]` overlay; the
brief's `DeployBlock` is `ImageBlock` now), `LeafSystem::framework`, and
`BoardDescriptor::{frameworks, entry_types}` with `bootable_frameworks()`.
Nothing reads them yet; W2's resolver is the first consumer. Tests:
`framework_is_an_image_key`, `an_image_framework_is_read_over_the_defaults`,
`frameworks_default_to_owned_spin_and_keep_their_order`.

### W2 — One resolver

- **Owner:** unassigned.
- `nros_orchestration_ir::image_framework(image, descriptor)` (RFC-0111 D3).
  Move `framework_for_board_key`'s rows into descriptors, then delete it.
  Rewire the macro, `entry_surface_features` and the dispatch lint to ask it.
- **Acceptance:** `framework_for_board_key` has no definition and no caller;
  every consumer in RFC-0111's table calls `image_framework`.
- **Status:** DONE (2026-11-15, branch `feat/phase-562-w2-resolver`, with W8)

Landed: `nros_orchestration_ir::image_framework(declared, &BoardBoot)` and
`BoardBoot { frameworks, entry_shape }`. The CLI builds `BoardBoot` from the
descriptor (`BoardDescriptor::image_framework`: `bootable_frameworks()`, and
`entry_kind = "zephyr-staticlib"` as the Zephyr entry shape, so the Zephyr rows
needed no new descriptor key). `nros::main!` and the Rust entry pack have no
descriptor and build it from the board key's family
(`nros_entry_lower::entry_shape_for_key`) plus, out of tree, the crate's
`NROS_BOARD_FRAMEWORK` as a one-entry list. `board_key_table.rs` holds the
two views to one answer for every descriptor name, and holds every descriptor
to the default list the macro assumes. `entry_surface_features` and
`framework_entry_deps` take the resolved framework, and `nros build` resolves
it for every image before generating anything. `framework_for_board_key` is
deleted. The `rtic-mps2-an385` word keeps RTIC through
`TRANSITIONAL_FRAMEWORK_KEYS`, which W3 deletes. Not done here: the dispatch
lint still reads the board crate's metadata (W9). Tests:
`an_absent_framework_is_the_boards_first`,
`a_framework_outside_the_boards_list_is_refused_naming_the_list`,
`zephyr_is_refused_because_the_board_decides_it`,
`the_zephyr_shape_is_the_boards`,
`the_transitional_rtic_word_still_resolves_rtic`,
`an_image_naming_zephyr_is_refused_and_the_board_still_decides` (macro),
`the_entry_surface_follows_the_resolved_framework_not_the_search_order`,
`the_framework_agrees_with_the_descriptors_entry_kind` (rewritten).

For W3: when the mps2 descriptor gains `frameworks = ["owned-spin", "rtic"]`,
`board_key_table.rs` goes red until the macro can read the list (it reads none
for an in-tree board today). Two other key-keyed RTIC facts remain for W3/W4:
`nros::main!`'s `rtic_board_spec_for` and `BOARD_PATHS`' `RticMps2An385` row
(D3's `entry_types`). `RTIC_DEVICE_CRATES` is already keyed by the
descriptor's name as well.

### W3 — The MPS2-AN385 descriptor

- **Owner:** unassigned.
- `frameworks = ["owned-spin", "rtic"]` plus `entry_types`; drop
  `rtic-mps2-an385` from `names`.
- **Acceptance:** `board = "mps2-an385-baremetal"` with no `framework` builds
  direct-exec, and with `framework = "rtic"` builds the RTIC entry.
- **Status:** DONE (2026-11-15, branch `feat/phase-562-w3-mps2-descriptor`)

Landed: the descriptor states `frameworks = ["owned-spin", "rtic"]` and
`entry_types = { owned-spin = "Mps2An385", rtic = "RticMps2An385" }`. One
source for the list: `nros::main!` now reads the image's board descriptor
FILE, the `NROS_BOARD_TOML` the cargo road already names for every image
(the file the CLI resolved the board to), through
`nros_orchestration_ir::DescriptorBoot::read`, and tracks it for rebuilds.
The CLI keeps its typed reader; `board_key_table.rs` now holds the two
readers to one `frameworks`/`entry_types` answer over every shipped
descriptor, and the macro's and CLI's resolution to one answer for every
framework each board lists (the old "every list is the default" assertion is
gone). The board TYPE follows the framework: `board_type_for(key, framework,
entry_types)` puts the `entry_types` row in the crate the key's one
`BOARD_PATHS` row names, and `rtic_board_spec_for` is keyed on that type
(`::nros_board_mps2_an385::RticMps2An385`), not on a board word. Without
`NROS_BOARD_TOML` (a plain `cargo build`), the macro's refusal of
`framework = "rtic"` says the list was not read.

`rtic-mps2-an385` stays in `names` (deferred to W5, not dropped here): it is
the same board, so keeping it as a name keeps ONE name-resolution path
(catalog, `package.xml` announcements, the board-vocabulary gates), and the
`TRANSITIONAL_FRAMEWORK_KEYS` row keeps its meaning (RTIC only). Dropping it
now would need an alias from the word to the board in every name lookup, a
second path that W5 would delete again; W5 removes the name, the transitional
row and the word's `BOARD_PATHS` row together, as the refusal lands.

Proof (2026-11-15, a copy of `examples/workspaces/rust` under `tmp/`, with one
added image `board = "mps2-an385-baremetal"` + `framework = "rtic"`):
`nros build pubsub_bringup:baremetal_talker` built the direct-exec entry
(board feature `board-entry`, `__nros_cortex_m_reset` in the ELF, no RTIC
symbol); the added image built the RTIC entry (board feature `rtic`, the
`rtic` crate, `__nros_app::__rtic_*` symbols); the transitional
`baremetal_talker_rtic` still built RTIC. Tests:
`a_two_framework_descriptor_boots_the_first_by_default_and_the_named_one`,
`the_board_type_follows_the_framework_through_entry_types`,
`the_descriptor_reader_matches_by_name_and_refuses_a_mistyped_list`,
`the_descriptors_framework_list_decides_for_an_in_tree_board`,
`one_mps2_board_expands_direct_exec_or_rtic_by_the_image_framework` (macro),
`the_framework_agrees_with_the_descriptors_entry_kind` (rewritten).

### W4 — Migrate the authored sites

- **Owner:** unassigned.
- RFC-0111 step 4, re-measured 2026-11-14 (`git grep -l rtic-mps2-an385
  -- ':!docs'`, 37 files). The RTIC images now live in the Rust workspace's
  bringups (`examples/workspaces/rust/src/{pubsub,service,action}_bringup/system.toml`,
  phase-535), not in per-example leaves. The rest: `bins/rtic-run-plan-e2e`,
  `examples/fixtures.toml`, `book-internals/src/realtime/rtic.md`, the CLI,
  entry-lower, macro and resolver code and tests, the mps2-an385 board crate,
  `board-support.toml`, `board-support-cells.toml`, the matrix cells,
  `.config/feature-lanes.toml`, the two board-vocabulary gates and the
  `NanoRosBareMetalPlatform.cmake` comment.
- **Acceptance:** `git grep -l rtic-mps2-an385 -- ':!docs/**/archived'`
  lists only the retired-name ledger and the refusal row of W5.
- **Status:** DONE (2026-11-15, branch `feat/phase-562-w4-migrate`)

Landed: the six `baremetal_<role>_rtic` images of the Rust workspace's
pubsub, service and action bringups and the `rtic-run-plan-e2e` image say
`board = "mps2-an385-baremetal"` + `framework = "rtic"`. Image ids, fixture
rows, `target_dir`s and test locators are unchanged, so no artifact path
moved (`matrix_fixture_coverage` green). The RTIC Internals chapter shows the
new spelling (`nros new` has no framework flag, so the chapter adds the line
by hand). Comments, docs and test inputs that used the word as an example of
"the RTIC board" now say `framework = "rtic"` or "the RTIC row"; mentions of
the folded crate `nros-board-rtic-mps2-an385` say "the separate RTIC board
crate". `RTIC_DEVICE_CRATES` lost its word row (every name list of the board
carries `mps2-an385-baremetal`). Tests that used the word only to reach RTIC
now state the framework: the entry-surface test, the macro's `before_boot`
and `param_services` refusals (through the descriptor, as the cargo road
does), the per-image board-fact test (`baremetal` + `framework = "rtic"`).

What still names the word is W5's to delete or keep, by file:
`nros-board-mps2-an385/nros-board.toml` (`names` and its note) and
`package.xml` (the announcement `check-provider-announcements` holds equal to
`names`); `nros-orchestration-ir` (`TRANSITIONAL_FRAMEWORK_KEYS`, the
`BOARD_PATHS` row, and the tests pinning both); `nros-entry-lower`'s
`BOARD_KEYS` row (every descriptor name is an entry key of its family);
`nros-macros` (the transitional pins in `in_tree_board_keys_resolve_to_an_emit_shape`,
`an_image_naming_zephyr_…`, `the_descriptors_framework_list_…`, and the
`BoardBoot::for_key` doc line); `nros-cli-core` (`image_framework`'s doc,
the alias test in `board_descriptor.rs`, the substring-victim row in
`sched_task_resolver.rs`, and `emit_rust.rs`'s pinned refused set, which
iterates `BOARD_PATHS`). Under `docs/` and `changelog.d/`: RFC-0111, RFC-0093,
the RFC index, RFC-0004's old deploy-schema example, the retired-names ledger,
phase-535's history, this doc and three changelog fragments.

Proof (2026-11-15): `nros build pubsub_bringup:baremetal_talker_rtic` in a
copy of `examples/workspaces/rust` under `tmp/` built with board feature
`rtic`, the `rtic` crate, and `__nros_app::__rtic_*` symbols in the ELF;
`nros build qemu_rtic_main_e2e_bringup:mps2-an385-baremetal` in
`rtic-run-plan-e2e` linked with `__rtic` symbols. (Its metadata probe's
"unrecognized instruction mnemonic" is not new: the old word prints the same
line.)

### W5 — Retire the word

- **Owner:** unassigned.
- Drop `rtic-mps2-an385` from the descriptor's `names` (W3 kept it, see its
  note), delete `TRANSITIONAL_FRAMEWORK_KEYS` and the word's `BOARD_PATHS` row.
- A row in `nros_entry_lower::RETIRED_BOARD_KEYS` whose refusal text names
  both replacements, generated from an optional `framework` replacement
  column (RFC-0111 D4); a ledger row, `retired-names.md`, a `.breaking.md`
  changelog entry, and the amendment note in RFC-0093 §3.
- **Acceptance:** `board = "rtic-mps2-an385"` is refused with the two-line
  replacement; `check-board-name-reach` carries the row.
- **Status:** OPEN

### W6 — Tests

- **Owner:** unassigned.
- A test per refusal (the old word, a framework the board does not list, a
  framework on a C image); `nros ws check` accepting a direct-exec image of the
  same board. Each mutation-checked (RFC-0111 step 6).
- **Acceptance:** each test fails when its refusal is removed.
- **Status:** OPEN

### W7 — Proof

- **Owner:** unassigned.
- All bare-metal rows build (`just build qemu`), `just test qemu` passes, and
  the RTIC run-plan tests and the RTIC cells (`test_qemu_rtic_*_e2e`) boot
  and deliver (RFC-0111 step 7).
- **Acceptance:** the runs above, on fixtures built from the merged tree.
- **Status:** OPEN

### W8 — `zephyr` leaves the framework vocabulary (RFC-0112 D5)

- **Owner:** unassigned.
- `FRAMEWORKS` keeps only Tier 1 values a user may choose (`owned-spin`,
  `rtic`, `px4`). `zephyr` becomes a platform entry shape the board descriptor
  derives (`entry_shape`), which the macro reads from the board and the image
  `framework` field refuses (RFC-0111 D2, RFC-0112 D5). The `zephyr` rows of
  `framework_for_board_key` move in the same change as W2, so the table and the
  descriptors never both answer.
- **Acceptance:** `FRAMEWORKS` has no `zephyr`; `framework = "zephyr"` is
  refused saying the board decides it; every Zephyr image still builds.
- **Status:** DONE (2026-11-15, branch `feat/phase-562-w2-resolver`, with W2)

Landed: `FRAMEWORKS = ["owned-spin", "rtic", "px4"]`; `zephyr` is in
`ENTRY_SHAPES`, which the board imposes. `image_framework` refuses
`framework = "zephyr"` (and `esp32`) on any image, saying the board decides it,
and refuses any `framework` on a board whose platform imposes a shape. The
spelling chosen for the descriptor side is the existing `entry_kind =
"zephyr-staticlib"`, read by `BoardDescriptor::entry_shape`, rather than a new
`entry_shape` key that would restate it. An out-of-tree board crate may still
state `framework = "zephyr"` in its metadata: that is the board deciding.

### W9 — The dispatch lint covers RTIC images

- **Owner:** unassigned.
- The framework × dispatch lint reads the IMAGE's resolved framework, not the
  board crate's metadata, so the RTIC rule fires for an RTIC image of a board
  that also boots direct-exec (RFC-0111 D3, "The dispatch lint is off for
  RTIC").
- **Acceptance:** `nros ws check` refuses an RTIC image whose dispatch the
  matrix rejects, and accepts a direct-exec image of the same board;
  mutation-checked.
- **Status:** OPEN

### W10 — RTIC meets Tier 1 criterion 5, and is re-promoted

- **Owner:** the nros 241 manager's W10 session.
- RTIC was demoted from Tier 1 on 2026-11-14 (RFC-0112 D3, issue 2540): the
  RTIC branch of `nros::main!` emits fixed priorities (the run task and custom
  tasks at 1, the tick and RX tasks at 2) with every callback inline in the run
  task, and its CI cells assert delivery only.
- **Acceptance:** the RTIC branch generates its task priorities from the
  sched-task contract (RFC-0113 D2), and an RTIC CI cell asserts a measured
  bound (idle wake count, or arrival-to-dispatch latency) and fails when the
  bound is exceeded. When both land, RFC-0112 D3 re-promotes RTIC to Tier 1
  and issue 2540 is resolved.
- **Design (approved 2026-11-17/18):**
  - The RTIC arm runs the image's derived sched tasks as levels, one RTIC
    software task each, most urgent first. The least urgent level boots at
    RTIC priority 1 and keeps the transport and the executor's chores. The
    hardware tasks (RX, level wake) sit one priority above the top level.
  - The level count is bounded by the board's dispatcher count and its NVIC
    priorities (`rtic_level_plan`). Tied priorities, too many levels, and a
    board with no level wake are refused, naming the board.
  - The executor lock is a BASEPRI ceiling at the top level
    (`RticBoardEntry::level_lock*`). It is recursive by depth, and its holder
    is identified by IPSR. The executor releases it around each callback and
    around its park, so a callback runs preemptible at its own level.
- **Deviation from "each level runs a `spin_band_once` loop" (approved):**
  - A non-boot level RUNS TO COMPLETION: one band pass per wake. A
    threaded RTOS's level parks on its waiter, but an RTIC task that blocked
    at a high priority would starve every level below it, the transport
    included.
  - Its waiter's `wake` spawns its task, and `wait` returns at once.
  - Its timers get their own wake: one board one-shot armed at the earliest
    non-boot deadline (`RticBoardEntry::arm_level_wake`; on mps2 the CMSDK
    dual timer, named only in the board crate). Without it, a higher band's
    due timer would wait out the boot level's park, which is bounded by the
    boot band's timers only. Spawning the levels from the boot loop instead
    would let a long boot-level callback delay them, a priority inversion.
  - Each band's deadline is read at the end of that level's own pass
    (`Executor::next_timer_deadline_of_level_us`), the one moment it is exact.
- **Stated bound:** a non-boot band's timer runs within [deadline,
  deadline + the level-wake ISR + dispatcher entry + the longest
  executor-internal BASEPRI section]. The boot level's state, idle in WFI or
  busy in a long callback, does not enter it.
- **Status:** DONE (2026-11-18, #4197 and the levels-cell change after it)
- **Done in two changes:**
  - The mechanism (board hooks, mps2 lock and level wake,
    `nros::levels::RunToCompletionLevels`, `nros::level_wake`, the macro's
    plan and emission, and a compile-time check that the macro's dispatcher
    list equals the board's), with the multi-level RTIC refusal still in
    place (#4197).
  - The proof and the acceptance together: the refusal lifted; the Rust
    workspace's `levels_bringup` (`urgent_pkg` at 100 Hz above `burner_pkg`,
    5 Hz and 50 ms of CPU a callback) and its `baremetal_levels_rtic` image;
    `test_qemu_rtic_levels_preempt_and_wake_on_time`, run by
    `just qemu test-all`; and RFC-0112 D3's re-promotion.
- **What the second change found:**
  - **The derivation had no RTIC target.** A bare-metal board's sched-task
    key is `""`: no priority plan, one level, so the contract derived
    nothing. An RTIC image now derives under its own key, `rtic`, out of the
    board's RTIC plan (`rtic_priority_plan`: priorities 1 up to the
    dispatcher count, capped below the NVIC count, with the hardware tasks
    above). The key has no authorable `[tasks.<name>.rtic]` sub-table:
    [issue 2698](../issues/2698-rtic-sched-task-not-authorable.md).
  - **The mps2 clock was not reentrant.** `clock_ns` read the counter, then
    `LAST_VALUE`, then stored, in three steps. A reader preempted between
    them read its own older value as a wrap, and the clock jumped forward
    2^32 ticks (~171.8 s) for good. A preempting level hit it within seconds.
    The read is now one critical section.
    [Issue 2703](../issues/archived/2703-mps2-clock-not-reentrant.md) records
    who else could hit it.
  - **The executor refuses a NULL lock context.** The board's lock keeps its
    state in its own statics, so the macro hands it the runner's address.
  - **QEMU's idle clock carries host jitter.** At `-icount shift=5`, a busy
    guest's time is its instruction count, and the urgent level ran within
    ~200 us of every deadline across every burn. While the guest idles in
    `wfi`, `sleep=on` (which the network needs) advances its clock at wall
    speed. A few idle activations per run were then 1 to 12 ms late, and the
    late wake was the hardware interrupt itself, not the software path. The
    cell asserts the bound for 95 % of idle activations and a 20 ms cap on
    all of them.
  - **Mutations** (both turn the cell red on its order assertion):
    every level on one RTIC priority (the urgent level never runs inside a
    burn, 668 of 668 burns), and the passes not arming the level wake (the
    urgent level is never woken again, 672 of 672 burns).

## Related

- [issue 2540](../issues/archived/2540-rtic-fails-tier1-realtime-criterion.md): RTIC
  does not meet RFC-0112's fifth Tier 1 criterion. The ruling of 2026-11-14
  demoted it and put its re-promotion here, as W10.
- RFC-0093 §3: its last framework word retires with W5.
