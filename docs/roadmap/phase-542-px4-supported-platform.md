# Phase 542 — PX4 becomes a supported platform: registry evidence, a user-tier build, and every PX4 cell in a lane

**Status (2026-11-06). OPEN — W0 (the registry questions) first; nothing
started.** Implements the 2026-11-06 revision of
[RFC-0039](../design/0039-px4-integration-architecture.md), whose D1 states
what "PX4 supported" means in RFC-0114 D10 status words: PX4 SITL reads
**Verified on host (daily)**, and Pixhawk hardware reads **Boots on hardware,
manual** at most until a hardware runner exists. Shares
[issue 2289](../issues/2289-px4-release-ships-no-posix-archives.md) with
[phase-541](phase-541-px4-posix-prebuilt-archives.md), its home: phase-541
ships the archives (option 1), and W3 here closes the issue's second gap.

Depends on:

- [issue 2243](../issues/2243-px4-sitl-tests-boot-two-daemons-at-once.md)
  (the nightly `px4 sitl` job boots two PX4 daemons in one build dir). It stays
  homed on [phase-466](phase-466-scheduled-lanes-produce-verdicts.md), because
  the red is the harness's, not PX4's. W5 here cannot start until it is fixed;
- PR #3169 (`registry-px4-row`, open): the `px4-sitl@Px4` registry row and the
  `px4` probe track. W1 plans the gate rule that PR is failing on; this phase
  does not edit the PR's branch;
- [phase-537](phase-537-book-user-guide-organization.md) W4 ("PX4 SITL gets a
  starter only when its probe track replays") and W9 (Platforms by support
  level), which place the book page this phase earns.

---

## Why

PX4 is a Tier 1 framework (RFC-0113 D9, admitted under RFC-0112 D3 on
2026-11-03), and phase-521 delivered the framework and the nightly job. On
`main` the support promise still has no evidence a reader can check:

- **No registry row.** RFC-0113 D9 records PX4's owner "in PX4's entry in
  `board-support.toml`"; no such entry exists on `main`. PR #3169 adds one and
  fails `check-flavour-lanes`, because `derive()` (`scripts/check-flavour-lanes.py`)
  reads a board crate for every row with a `matrix_platform`, and
  `packages/boards/px4-sitl/Cargo.toml` does not exist. The other `external`
  row, `s32k344-safety-island`, passes only because it carries no
  `matrix_platform`.
- **No green lane.** RFC-0112 D3's delivering cell is `(Px4, Cpp, Uorb,
  Pubsub)`, run by the hosted `px4-sitl` job in `nightly.yml`. It has not been
  green on any scheduled run (issue 2243). Even green, RFC-0114 D10 lets an
  automated cadence stand only on a Runtime cell of the row's platform, and
  both PX4 cells are `Gated` (the uORB cell also carries a
  `CarveOut(NO_LOAN_SLOT)`).
- **No user-tier build.** Until phase-541 the release shipped no PX4 POSIX
  archives or placed headers (issue 2289), so `make px4_sitl_default` on the
  installed track stopped at PX4's configure; phase-541 ships them. A module
  that names a `BRINGUP` still reaches the dev-tier verbs `nros dev image
  model-path` and `nros dev codegen entry` from CMake.
- **Cells nobody runs.** The XRCE companion cell `(Px4, Rust, Xrce, Pubsub)` is
  Gated because no CI runner carries the PX4 `.msg` tree. The bridge's ROS 2
  leg (`px4_bridge_ros2_e2e`, `px4_bridge_xrce_ros2_e2e` in
  `packages/testing/nros-px4-sitl-test/`) runs only through the `debug` recipes
  `just px4 test-bridge-ros2` and `just px4 test-bridge-xrce-ros2`, and has no
  cell.
- **A book page with no evidence for half of it.** `book/src/targets/px4.md`
  §"Build" and §"Flash a real board" (lines 114–162) teach a
  `px4_fmu-v5_default` build and upload that nano-ros has never run.

None of this had an owner before this phase. Phase-466 holds 2243, phase-537
places the page once a track replays, and phase-528 W5 deliberately keeps PX4
SITL on a direct spawn; none of them plans the rest.

## Work items

Ownership: the maintainer owns PX4 as a Tier 1 framework (RFC-0113 D9), so
each item's owner is the maintainer unless it names another.

### W0 — The registry questions RFC-0114 D10 leaves open (S)

RFC-0114 D10 carries a note (2026-11-06) that these are open and owned here.
Each needs a maintainer ruling, recorded in RFC-0114's decision log, before W1
or W5 can be written:

1. **Host + manual.** D10's "Boots, manual (date)" row covers `runs =
   emulator|hardware` only. A `runs = "host"`, `cadence = "manual"` row (what
   #3169 adds) has no status word. Options: widen "Boots, manual" to `host`, or
   add a "Boots on host, manual" word.
2. **An external row with no board crate.** `check-flavour-lanes` reads a Rust
   crate's std/no_std flavour per `matrix_platform`. A row whose board package
   lives in someone else's tree (`external = "PX4-Autopilot"`) has neither a
   `Cargo.toml` nor an `nros-board.toml` under `packages/boards/`. Options:
   exclude `external` rows the way `_is_declarative` excludes declarative ones,
   or let the row state its flavour.
3. **What backs `cadence = "daily"`.** Either a `Gated` cell whose Gated string
   names the workflow job that runs it counts as daily evidence (RFC-0122 D8/D9
   already require a cadence to name its job), or PX4 needs a `Runtime` cell.
   The second fails `domains_valid` at once: `alloc::domain_of` gives each
   platform a 21-wide window of the 232 DDS domains, which holds exactly 11
   platforms, and `PlatformId::Px4` sits at index 12
   (`packages/testing/nros-tests/src/matrix.rs`). The window would need
   narrowing, as that file's own comment says.

**Acceptance:** three rulings in RFC-0114's decision log, and D10's tables
amended to match.

### W1 — The `px4-sitl@Px4` row lands, with the gate rule W0.2 picks (S)

PR #3169 carries the row and the `px4` probe track. This item plans the
`check-flavour-lanes` change it is blocked on; the PR's author, or whoever
rebases it, implements it on the PR.

- `derive()` handles an `external` row with a `matrix_platform` and no board
  crate by W0.2's rule, and reports it in its own bucket rather than as
  unreadable.
- The row's status word comes from W0.1's rule.

**Acceptance:** `px4-sitl@Px4` and `[[probe]] track = "px4"` on `main`;
`check-flavour-lanes`, `check-board-registry` and `check-board-tiers` green;
`s32k344-safety-island` reads the same status word as before. RFC-0124 D8's
"Applied so far" record is updated to say so.

### W2 — The release ships the PX4 POSIX archives and their headers — MOVED to phase-541

Issue 2289's option 1, planned here first, is
[phase-541](phase-541-px4-posix-prebuilt-archives.md) W1–W3 (the maintainer's
pull request, #3189): the release stages `libnros_cpp.a` (uORB features),
`libnros_platform_posix.a` and the placed headers under
`<sdk-root>/prebuilt/px4-posix/`, and `NanoRosPx4Module.cmake` reads them.
Nothing is left for this item; the number is kept so W4's dependency reads.

### W3 — The user-tier PX4 configure reaches no dev-tier verb (S) — issue 2289, second gap

A module that names a `BRINGUP` calls `nros dev image model-path` and `nros dev
codegen entry` from CMake. Give that path a user-tier verb (or the ones the
other CMake roads already use), so PX4's configure never enters the developer
tier.

**Acceptance:** `integrations/px4/` names no `nros dev` verb, and a source
gate added with the change refuses one; a module template with a `BRINGUP`
configures on the installed track.

### W4 — The `px4` probe track replays; the book page earns its place (S)

Depends on W1, W3 and phase-541 W4 (the book session drops the track's
`awaits` on issue 2289 once the archives ship).

- The track drops its `awaits` and replays the SITL `make`, the headless boot
  and `px4-nano_ros_app start` green on the installed track (issue 2289's
  "Done when").
- The PX4 page states no cargo step.
- Phase-537 W4 then gives PX4 SITL a starter, and phase-537 W9 places the page
  by its status word. Both are phase-537's to write; this item only makes them
  possible.

**Acceptance:** the weekly probe run shows the `px4` track green with no
`awaits`; issue 2289 closes; RFC-0124 D8's exception to Principle 3 ends.

### W5 — The `px4-sitl` job counts as a `daily` lane (M)

Depends on issue 2243 being fixed (phase-466) and on W0.3.

- If W0.3 accepts a named Gated job: the row moves to `cadence = "daily"`, and
  `check-board-registry` accepts the `px4-sitl` job as the row's lane.
- If W0.3 requires a Runtime cell: narrow the domain window in
  `nros_tests::alloc` first, so `domains_valid` holds with `Px4` baking, and
  then flip the cell.
- The job carries its verdict: a red is reported by
  `just nightly-triage` like any other nightly cell.

**Acceptance:** seven consecutive scheduled `px4 sitl` runs green; the row
reads **Verified on host (daily)**; `domains_valid` green.

### W6 — A lane for the XRCE companion cell (M)

`(Px4, Rust, Xrce, Pubsub)` is Gated because no CI runner carries the PX4
`.msg` tree. The hosted `px4-sitl` job already provisions it (`nros setup
--source px4-autopilot`), so the companion test (`px4_xrce_e2e`, today run by
`just px4 test-sitl`) can run there or in a sibling job with the agent from
`just setup xrce`.

**Acceptance:** the cell's Gated string names the job that runs it, and that
job runs the test green on a scheduled run.

### W7 — A cell for the bridge's ROS 2 leg (M)

The bridge tests subscribe with a real ROS 2 node through a `px4_msgs` overlay
(`just px4 build-px4-msgs-ros humble`, `nros_tests::ros2` `topic_echo_in_overlay`;
archived phase-325 W3.4). They have no `interop::CELLS` row and no lane.

- Add the interop rows (zenoh, and XRCE beside PX4's own `uxrce_dds_client`),
  each bound with `interop::assert_test_bound`.
- Give them a lane that has ROS 2 and the overlay. The two share one SITL build
  dir, so they run one after the other (archived issue 1896).

**Acceptance:** both rows in `interop::CELLS`; `matrix_fixture_coverage.rs`
green; one scheduled run of their lane green.

### W8 — Pixhawk hardware (L) — W8.a now, W8.b deferred

- **W8.a — the book stops teaching what has no evidence.** `targets/px4.md`
  lines 114–162 build and upload `px4_fmu-v5_default`. Until W8.b lands, the
  page states that nano-ros has no run on a flight controller, or the section
  moves out of the user book. Coordinated with phase-537 W9, which owns the
  platform pages. **Acceptance:** no user page teaches a Pixhawk build or
  upload without a registry row behind it (phase-537 W9's own acceptance).
- **W8.b — deferred, pending a maintainer ruling on whether nano-ros takes on
  hardware.** Its scope: prebuilt NuttX archives for one `px4_fmu-*` target
  (issue 2289 calls these a separate, later question: they are per-board cross
  builds), a recorded hardware run, and a `runs = "hardware"`, `cadence =
  "manual"` row. **Acceptance when taken:** the row reads **Boots on hardware,
  manual (date)**.

### W9 — The two open design questions (S)

- **RFC-0003 §7, PX4 multi-component mapping** (one PX4 module per component,
  or one module hosting N). RFC-0113 D9's generated entry already answers it
  for sched tasks: one module, one `SchedTaskWorkItem` per sched task, and a
  node whose groups span two sched tasks is refused. Record that answer in
  RFC-0003, or rule otherwise.
- **RFC-0072 §4, per-board selectability.** An external module gets
  `EXTERNAL_MODULES_LOCATION`, all-or-nothing; a `<label>.px4board` opt-in
  needs in-tree code, as PX4 did for zenoh. Rule whether nano-ros pursues an
  in-tree PX4 module (an upstream contribution), or states all-or-nothing as
  the supported shape.

**Acceptance:** both RFCs carry the ruling; RFC-0039's open questions list
drops them.

## Deferred

- **uORB services.** RFC-0011 §7 defers them; the C++ backend's service slots
  return UNSUPPORTED (`packages/rmw/uorb/nros-rmw-uorb/src/service.cpp`). A
  PX4 user who needs services uses the XRCE companion. Not in this phase.
- **Pixhawk hardware (W8.b)**, until the maintainer rules.

## Order

W0 → W1. W3 in parallel with W1 (W2 moved to phase-541) → W4. W5 after issue 2243 and W0.
W6 and W7 are independent of the rest. W8.a with phase-537 W9. W9 any time.

## Acceptance (phase)

- `px4-sitl@Px4` on `main` reads **Verified on host (daily)**.
- The `px4` probe track replays with no `awaits`; issue 2289 is closed.
- Every PX4 cell names a job that runs it, and each job has a green scheduled
  run.
- No user book page teaches a PX4 build that has no registry row behind it.
- RFC-0039 moves to Stable, with its D1 met.

## Not in scope

- The harness fix for issue 2243 (phase-466 keeps it).
- Book page placement and starters (phase-537 W4/W9).
- uORB services (deferred above).
