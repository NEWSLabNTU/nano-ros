# Phase 562 — An image's entry framework is its own field

**Status (2026-11-14). OPEN: no item has started. Owner: unassigned.** Opened by
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

- **Owner:** unassigned.
- Add `framework` to the image table in both readers: `leaf_system` and
  `nros-cli-core`'s `deny_unknown_fields` `DeployBlock`. Add `frameworks` and
  `entry_types` to the board descriptor schema (RFC-0111 D1, D2).
- **Acceptance:** both readers accept `framework` and refuse an unknown key;
  a descriptor with no `frameworks` key reads as `["owned-spin"]`.
- **Status:** OPEN

### W2 — One resolver

- **Owner:** unassigned.
- `nros_orchestration_ir::image_framework(image, descriptor)` (RFC-0111 D3).
  Move `framework_for_board_key`'s rows into descriptors, then delete it.
  Rewire the macro, `entry_surface_features` and the dispatch lint to ask it.
- **Acceptance:** `framework_for_board_key` has no definition and no caller;
  every consumer in RFC-0111's table calls `image_framework`.
- **Status:** OPEN

### W3 — The MPS2-AN385 descriptor

- **Owner:** unassigned.
- `frameworks = ["owned-spin", "rtic"]` plus `entry_types`; drop
  `rtic-mps2-an385` from `names`.
- **Acceptance:** `board = "mps2-an385-baremetal"` with no `framework` builds
  direct-exec, and with `framework = "rtic"` builds the RTIC entry.
- **Status:** OPEN

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
- **Status:** OPEN

### W5 — Retire the word

- **Owner:** unassigned.
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
- **Status:** OPEN

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

- **Owner:** unassigned.
- RTIC was demoted from Tier 1 on 2026-11-14 (RFC-0112 D3, issue 2540): the
  RTIC branch of `nros::main!` emits fixed priorities (the run task and custom
  tasks at 1, the tick and RX tasks at 2) with every callback inline in the run
  task, and its CI cells assert delivery only.
- **Acceptance:** the RTIC branch generates its task priorities from the
  sched-task contract (RFC-0113 D2), and an RTIC CI cell asserts a measured
  bound (idle wake count, or arrival-to-dispatch latency) and fails when the
  bound is exceeded. When both land, RFC-0112 D3 re-promotes RTIC to Tier 1
  and issue 2540 is resolved.
- **Status:** OPEN

## Related

- [issue 2540](../issues/2540-rtic-fails-tier1-realtime-criterion.md): RTIC
  does not meet RFC-0112's fifth Tier 1 criterion. The ruling of 2026-11-14
  demoted it and put its re-promotion here, as W10.
- RFC-0093 §3: its last framework word retires with W5.
