# Phase 560 — Follow the contract revision on the RTOS side

**Status (2026-11-13). W1-W5 DONE** (rlm v0.3.15, play_launch v0.32.0,
#3745); W6 and W7 BLOCKED on releases not yet tagged; W8 DONE (every level
charged since #3758); W9 (the CAN frame driver, RFC-0132) OPEN, RFC accepted
2026-11-17, W9.a, W9.b, W9.c and W9.d DONE. nano-ros's
share of two play_launch phases: phase 103 (defects the contract grammar review found in shipped
behaviour) and phase 104 (the grammar revision itself). The design of
record is rlm `docs/design/grammar-revision/` (an overview and one file per
topic, T1-T10). Nothing here starts before the rlm release it follows.

## The division

| Repository | Role |
|---|---|
| rlm | The runtime-agnostic specification: the grammar, the SystemModel, and what every key and rule means. |
| play_launch | The Linux realizer: resolves launch trees, checks them, observes the running system. Its resolver's layer 2 (no ROS linked) is what nano-ros builds to resolve launch trees. |
| nano-ros | The RTOS realizer: reads the model into a board image and generates its monitors; the board's platform facts live in `system.toml`. |

The review found that three meanings were implemented on each side with no
rlm text to hold them together. Two of them are in nano-ros: what entering a
mode runs (`ftti_budget::reaction_path`) and the default of an omitted `on:`
(`ftti_budget::classes`). For each, rlm states the meaning first, and both
realizers then follow it. nano-ros does not choose its own reading.

## Work items

Each item names the upstream unit it follows. The upstream state on
2026-11-13: rlm v0.3.15 and play_launch v0.32.0 are the latest tags; play_launch
phase 103 (D1-D9) and phase 104 S1 and S2 shipped in them; phase 104 S3 and S4
have no release.

### W1 -- Entering a mode

- **Status:** DONE (2026-11-13, #3745)

Follows play_launch 103 D4, after rlm states the meaning.
`ftti_budget::reaction_path` used to take the entered mode's own `reaction` and,
with none, treat entering as the reaction, while the checker walked the mode's
`fallback` levels. rlm v0.3.15 states that the entered mode is the one taken
away and that its last fallback level, the minimal risk condition, is charged
against `ftti`. `reaction_path` resolves that level, so the budget and the
observer rows follow it. Tests: `a_mode_reaction_resolves_through_the_fallback`
(fails on the old reading) and `a_mode_without_fallback_has_no_reaction`.

### W2 -- The omitted `on:`

- **Status:** DONE (2026-11-13, #3745)

Follows play_launch 103 D5. `ftti_budget::classes` returned nano-ros's own copy
of the five pre-v0.3.13 classes, rlm's doc said every class, and play_launch's
message said omission, late and loss. rlm v0.3.15 defines `FaultKind::DEFAULT`
as those five. The local copy is deleted, and the budget reads
`HazardContract::classes()`, so no budget moves.

### W3 -- Constant predicates

- **Status:** DONE (2026-11-13, #3745: tag bump)

Follows play_launch 103 D1 (rlm model `Constant { constant, value }`). No
nano-ros code reads `when`, `commands` or a predicate's value. If the image ever
judges a value, it reads `value`, never the constant's name.

### W4 -- Criticality through the resolver

- **Status:** DONE (2026-11-13, #3745: submodule bump; no table changes)

Follows play_launch 103 D3: criticality is now derived through the same ladder
resolution as the reaction walk, so a hazard that enters a mode credits its
reacting nodes. The derivation reads hazards, and no nano-ros contract declares
one: none of the 34 tracked `*.contract.yaml` writes `hazards:`, `modes:`,
`enters:` or `fallback:` (measured on main after #3745). So no generated
criticality can change here, and there is nothing to review. The review is owed
when the first nano-ros contract declares a mode-form hazard.

### W5 -- Removals

- **Status:** DONE (2026-11-13, #3745: tag bump; `migrate` dry run clean)

Follows play_launch 104 S1. None of the removed keys (`external_topics:`, a
topic's `max_transport`, `paths.<p>.input`, `exit.on`, a mode in
`hazards.<h>.reaction`, `links.<l>.monitor`, `tolerance`) is written in
nano-ros's contracts or read by its code. Measured with play_launch v0.32.0's
`ros-launch-resolve migrate --check` (rlm v0.3.15): 34 of 34 tracked contracts
and 182 of 182 tracked `system.toml` files scanned, 0 to rewrite.

rlm v0.3.16 and play_launch v0.33.0 (#3765) finished S1: a path's `tolerance`
is refused, and a subscriber's inline `max_transport` is a
`deprecated-spelling` warning, refused at the next rlm release. nano-ros
writes neither key and reads neither model field (`git grep` over tracked
contracts and non-vendored Rust, 2026-11-14), so that refusal costs a tag
bump.

### W6 -- Renames and relocations

- **Status:** BLOCKED (rlm's rename release R, play_launch 104 S3; not tagged as of rlm v0.3.16)

Tag bump, then `migrate` over the contracts. Rates become `Hz` values (44
values); `entity_census.rs` messages and fix-its spell `trigger.timer.rate_hz`
(`:850-897`); inline test YAML. The model keeps its field names, so no model
reader changes. `supervision { tick, switch }` leaves the contract for the
platform file; nano-ros already states the tick per board in `system.toml`.
Size M.

### W7 -- Additions

- **Status:** BLOCKED (each play_launch 104 S4 addition; none released as of rlm v0.3.16)

Each lands when a contract needs it. A new fault class adds one arm to the
exhaustive match in `ftti_budget.rs`. End-to-end protection (a counter field,
T7) is the image's to check if it enforces one. Several hosts (T10): the image
reads one model per board, unchanged.

The CAN frame driver under T7's CAN FD links (issue 2633, RFC-0132) is not
blocked on a release; it is W9.

### W8 -- E7, the same answer as play_launch

- **Status:** DONE (2026-11-14, rule `hazard-level-runtime`)

The image reports each level its ladder becomes active on, by index and name.

The build-time budget charges every level (#3758, 2026-11-15). `ftti_budget`
used to charge only the minimal risk condition, so on the L4 island contract
it named B4 (21 s) where play_launch's binding row is B3 (80 s). It now
follows rlm v0.3.18's "Which levels a hazard runs, and which rows are
judged": one row per level in the model's `degradation_levels` (skipped
levels absent), detection + the dwelling levels waited above (route +
charged dwell, a range's `max`) + the level's route + its settle, judged
against `ftti` and the level's own `within` (a dwelling level against
`within` only), and the binding row is `ros_launch_manifest_derive::levels`,
the code play_launch calls. The route is the model's `route_ms`, the quantity
the image's rung monitor judges, so the build-time and run-time halves use
one number. Fixture (#3759): `packages/cli/nros-cli-core/tests/fixtures/mode_ladder`,
a reduced L4 ladder with three hazards (every level run, two skipped by
`requires`, one skipped by `requires_lost`); every row, binding row and the
`level-within` refusal equal play_launch 0.34.0's
(`tests/mode_ladder_rows.rs`).

Not charged by the image, as before: a class with no executor-side detector
(`reported` among them), so a hazard whose only class is `reported` has no
row here where play_launch, which counts the publisher's own report, has one.

Follows play_launch 102 M5 E7, which play_launch's report records as not run.
The image reports the fallback level it fell to, so the evaluation can compare
nano-ros's verdict with play_launch's on an omission fault. It needs W1's
meaning, which shipped in #3745. Size M.

### W9 -- The CAN frame driver (RFC-0132)

- **Status:** OPEN (RFC-0132 accepted 2026-11-17)

Issue 2633 (GitHub #4012), consumer ASI; the portable layer under issue 2573's
FlexCAN helper. The items land in this order, each in its own pull request,
and each states its acceptance before it lands.

- **W9.a -- The header, the bindings, the mock.** DONE (2026-11-17): the mock's
  handle state measures 8 bytes (alignment 4) against 128.
  `packages/platform/nros-platform-api/include/nros/platform_can.h` (RFC-0132
  D2), added to `scripts/gen-abi-bindings.sh`'s platform wrapper with the
  regenerated `nros-platform-cffi/src/generated.rs` committed; the
  `PlatformCan` trait and the `nros_platform_export_can!` macro; the
  `packages/drivers/can/nros-can-mock` crate (host-only, `compile_error!` on
  `target_os = "none"`).
  *Accepted when:* `just check abi-bindings` and `just check
  platform-abi-mirror` pass with `platform_can.h` in `HEADERS_REQUIRE_MACRO`,
  and a deliberately unlisted symbol fails the mirror gate (its self-test);
  `check-opaque-storage-guards` lists the header as an emitter and the mock's
  `assert!` as a guard, and fails when the assert is removed; a host test
  sends classic and FD frames through the mock and reads them back, and
  refuses a 9-byte classic frame and a 13-byte FD frame; the mock crate fails
  to build for `thumbv7m-none-eabi`. The RFC records the measured `sizeof` of
  the mock state beside `NROS_PLATFORM_CAN_OPAQUE_U64S`.
- **W9.b -- The `zephyr` backend.** DONE (2026-11-18): both `native_sim`
  shapes and the `mr_canhubk3` compile pass `scripts/test/can-frame-zephyr.sh
  --s32k344`. `nros-platform-zephyr/src/can.c` over the
  `nros,can` chosen, else `zephyr,canbus`; `NROS_PLATFORM_CAN_SEND_WAIT_TX`
  through Zephyr's blocking send. `can_start.c` sets `CAN_MODE_FD` at
  bring-up when the node states a data-phase bitrate, and starts the
  `nros,can` controller too (RFC-0132 revision of 2026-11-17, user-approved).
  *Accepted when:* a `native_sim` image whose overlay names `nros,can` with a
  data-phase bitrate sends a classic and an FD frame that `candump vcan0` on
  the host reads byte for byte; an image whose controller states no data
  bitrate opens with `want_fd`, reports `fd_active` false, sends its classic
  frame and gets `NROS_PLATFORM_CAN_ENOFD` for the FD one (nothing FD on the
  bus); the backend's guard is in the gate's `GUARD_SITES`; the S32K344 build
  (issue 2573's board) compiles the backend.
- **W9.c -- The `socketcan` backend.** DONE (2026-11-17): both probes' frames
  read back by `candump` on vcan0; the classic-interface (MTU 16) check is
  GATED (no classic interface on the build host), reported NOT VERIFIED by
  `just linux test-can-frames` until one is given.
  `packages/boards/nros-board-linux/c/can_socketcan.c` with a new `build.rs`,
  compiled by the same path into `nros-board-freertos-posix` and
  `nros-board-threadx-linux`; `PF_CAN`, `CAN_RAW` and `linux/can.h` added to
  `check-posix-platform-purity`'s `FORBIDDEN`; `can` added to those two
  boards' `links`.
  *Accepted when:* `just check posix-platform-purity` passes and its self-test
  refuses a `PF_CAN` line planted in `nros-platform-posix/src`; a `linux` and a
  `freertos-posix` image each send a classic and an FD frame on `vcan0` that
  `candump` reads; `fd_active` is false and `want_fd` refused on a classic
  `vcan` (MTU 16).
- **W9.d -- The device table and its carrier.** DONE (2026-11-18): the
  refusals are `resolve_can`'s unit tests in
  `nros-cli-core/src/orchestration/image_net.rs` (the two mock cases over the
  tracked `nucleo-f767zi-zephyr` and `mps2-an385-baremetal` descriptors), the
  workspace road through `resolve_system`, and the codec round trip in
  `nros-board-common/src/image_net.rs`. Found on the way, and refused at
  configure in the same pull request: a zenoh `can/` or `isotp/` locator on
  `freertos-posix` and `threadx-linux` configured and then failed at the
  final link (`undefined reference to _z_open_can`; zenoh-pico's CAN arms are
  `ZENOH_LINUX` and `ZENOH_ZEPHYR` only). The zenoh port rows now state
  `links` (`[port.posix]`, `[port.zephyr]`), and
  `nros-cli-core/src/orchestration/link_arms.rs` refuses an endpoint on a link
  the image's platform row lists no arm for; `just check zenoh-port-links`
  derives each row's list from zenoh-pico's `_z_open_<link>` guards and fails
  on drift in either direction. The FD-at-open split is decided
  (user ruling, 2026-11-18) and tabled in RFC-0132 D2: Zephyr opens with
  `want_fd` and reports `fd_active`, socketcan and the mock refuse it at open,
  and an FD send on a handle without FD is `ENOFD` everywhere. W9.c's GATED
  classic-interface cell now runs in a private network namespace
  (`unshare -rn`) when no classic interface is given.
  `[image.<id>.can]` (`device`,
  `driver = "mock"`) in the `system.toml` schema; `[canN]` rows (`bitrate`,
  `dbitrate`, `frames`) in the D4 file parser (`system_net.rs`); the `can`
  row kind in `nros-board-common/src/image_net.rs`; the RFC-0132 D5 and D6
  refusals in `nros configure`.
  *Accepted when:* CLI tests refuse, each with the message naming the right
  home: `device` on an `"os"` board, `device` on a `"board"` board, a `[canN]`
  table on a host board, `driver = "mock"` on `nucleo-f767zi-zephyr` (has
  `[board.flash]`) and on `mps2-an385` (an emulator board), two unmarked
  `[canN]` tables, and a device on a board whose `links` lacks `can`; the
  image-net codec round-trips a `can` row in its unit tests.
- **W9.e -- Delivery on every road.** The board decode of the `can` row and
  the hand-off to `open(NULL)`.
  *Accepted when:* a non-default probe (`[image.<id>.can] device = "vcan7"`)
  reaches `open(NULL)` on the cargo road and on the cmake road (an image built
  each way reports the device it opened), and a `native_sim` image whose
  overlay sets `nros,can` opens that node on the west road; no new `NROS_*`
  environment variable or Kconfig symbol appears
  (`python3 scripts/nros-build-wiring.py` shows no new knob source).
- **W9.f -- The `board` hook and the consumer.** Weak fail-loud defaults and
  the `s32z270-freertos` strong symbols over the NXP RTD driver (GitHub
  #4015); ASI's `platform_can.h` `#if` chain replaced; the follow-up issue
  for rebasing the zenoh-pico CAN link onto this driver (RFC-0132 open
  question 4) filed against RFC-0080.
  *Accepted when:* an image on a board with no strong symbols fails at
  `open` with "not supported", naming the board; the `s32z270-freertos` image
  builds with the RTD backend; ASI's host census image builds with the mock
  and ASI sets `[census] on_missing = "error"`; the follow-up issue exists.

The five open questions of the first draft are resolved in RFC-0132 itself
(send completion, polled receive, consumer backends through the `board` hook,
sharing with the link, one default device); none is left to this phase.

## Acceptance

- W1, W2: one rlm sentence per meaning, cited in the code comment that
  implements it; a test that fails on today's reading.
- W5, W6: `migrate` over every nano-ros contract, and the gates report zero
  `deprecated-spelling` warnings.
- Every item lands in a pull request that bumps rlm and the play_launch
  submodule together. Its manifests name the tag the submodule pins.
- W9: each sub-item meets its own "accepted when" above; W9 is DONE when
  W9.a-W9.f are, and issue 2633 closes with W9.f.

## Issues homed here (2026-11-15)

The L4 Safety Island's plan items that land on the RTOS realizer, filed from
their GitHub reports.

- [issue 2570](../issues/archived/2570-ftti-budget-charges-only-last-ladder-level.md): `ftti_budget` charges only the ladder's last level (NR-15). W8 made the image report its level; the build-time half landed in PR #3853 (resolved).
- [issue 2571](../issues/archived/2571-mode-form-contract-fixture.md): no fixture declares `hazards:` with a `modes:` ladder (NR-19). Resolved by PR #3853.
- [issue 2572](../issues/2572-timer-silence-watchdog-from-on-violation.md): a timer watchdog for subscriber silence, generated from `on_violation.mechanism: timer` (NR-12). W7; blocked on the rlm addition.
- [issue 2573](../issues/2573-can-fd-e2e-profile-and-flexcan-helper.md): a CAN FD fixed-length E2E profile and a raw FlexCAN helper (NR-6). W7's end-to-end protection.
- [issue 2633](../issues/2633-board-owned-can-frame-driver.md): a board-owned CAN frame driver (`open`/`send`/`send_fd`/`fd_active`/`recv`), the portable layer under 2573's FlexCAN helper (GitHub #4012, consumer ASI). W9; design [RFC-0132](../design/0132-board-can-frame-driver.md), Draft, review-gated.
- [issue 2574](../issues/2574-pluginlib-free-mode-plugin-registration.md): pluginlib-free registration for the mode decider and switcher plugins (NR-17). Homed here with its siblings; it is a registration question, not a contract one.

