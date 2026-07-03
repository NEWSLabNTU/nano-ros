# Phase 277 — Examples UX overhaul (landing-user experience)

Status: **Complete — 2026-07-03** (all waves landed on branch
`examples-ux-overhaul`; three runtime lanes remote-CI-gated, see Outcome) ·
Implements RFC-0026 (layout), RFC-0031 (RMW selection surface) · Informs issue
#102 (example coverage), RFC-0024/0025 (workspaces).

> **Goal.** A newcomer landing on nano-ros can (1) follow one canonical setup story,
> (2) read any talker/listener/service/action example and see the same behavior and
> wording as the official ROS 2 demos, (3) copy an example out as their own project
> and build it with documented steps, and (4) browse an examples tree whose
> organization explains itself. Driven by a 2026-07-03 three-way audit (standalone
> tree, workspaces + ROS 2 parity, onboarding docs).

## Why

The audit found five systemic gaps:

1. **Copy-out promise false.** `examples/README.md` claims "copy any directory out
   and it builds", but 97 example `Cargo.toml` files path-walk
   `../../../../packages/…`, 111 `.cargo/config.toml` files patch `std_msgs` to a
   gitignored `generated/` dir, and every C/C++ `CMakeLists.txt` walks up four
   levels to `cmake/NanoRosWorkspace.cmake`.
2. **ROS 2 parity gaps.** Every talker/listener publishes `std_msgs/Int32` counters
   with `Published:/Received:` logs; official demos use `std_msgs/String`
   "Hello World: N" with `Publishing:` / `I heard:`. C/C++ node names carry
   `c_`/`cpp_` prefixes (Rust examples and ROS 2 use bare `talker`/`listener`).
   Service clients hard-code request batches; action log wording diverges.
3. **Boilerplate and gating in example source.** 23 Rust example files carry
   `#[cfg(...)]` (RMW link-force statics, a dual `fn main` on `safety-e2e`, a
   second `app_main` entrypoint on ThreadX); embedded C talkers hand-roll CDR
   bytes with magic headers; the native C talker is a 202-line kitchen sink with
   unrelated clock and parameter demos; E2E-harness readiness markers live inside
   example bodies.
4. **Docs drift.** Root README teaches the pre-`nros setup` onboarding model and
   mixes two GitHub org URLs; `integration-zephyr.md` shows a fabricated C API
   (`nros_init` / `nros_create_node` — no such symbols); three broken doc links;
   `SUMMARY.md` orphans; `templates/README.md` documents 4 of 11 templates.
5. **Organization noise.** Tracked POC/phase-named dirs sit among canonical
   examples (`entry-poc`, `phase216-rtic-e2e`, `qemu-baremetal-main-e2e`, three
   `-poc` C++ dirs); `zephyr/cpp/talker-typed` duplicates `talker`; Cargo.lock
   policy is split (55 committed, rest ignored); the `workspaces/` two-layer
   naming scheme (base `{rust,c,cpp,mixed}` + `ws-<topic>-<lang>`) is undocumented;
   19 of 27 `ws-*` workspaces have no README.

The embedded Node-pkg examples (zephyr / freertos / nuttx / threadx-linux / esp32)
are the quality bar: near-identical across platforms, cfg-free, declarative.
This phase converges everything else onto that bar.

## Work items

Each wave is separately committable with `just ci` green.

### W1 — Docs correctness (no tree changes)

- [x] W1.a `README.md`: rewrite onboarding to the book's canonical flow
      (`just setup-cli` → `source ./activate.sh` → `nros setup <plat>`); drop the
      git-dep snippet and `build/zenohd` model; org URL → `NEWSLabNTU` everywhere
      (also `book/src/concepts/comparison-vs-microros.md`,
      `book/src/user-guide/message-generation.md`); fix broken links
      (`docs/guides/getting-started.md` ×3, `docs/reference/embedded-integration.md`).
- [x] W1.b `book/src/getting-started/integration-zephyr.md:97-113`: replace the
      fabricated C API block with the real surface from
      `examples/zephyr/c/talker/src/Talker.c`.
- [x] W1.c Small verb fixes: `first-node-c.md` `nros_init()` → `nros_support_init()`;
      `concepts/ros2-comparison.md` + `porting/custom-rmw.md` `add_subscription` →
      `create_subscription`; `threadx.md` `setup.bash` → `activate.sh` (+
      `config.toml` → `nros.toml` drift).
- [x] W1.d `book/src/SUMMARY.md`: add `integration-px4.md`; resolve
      `zephyr.md`/`nuttx.md` orphans (link as contributor-path pages or merge;
      retarget `reference/build-commands.md` link if deleting).
- [x] W1.e `examples/README.md`: `just qemu-baremetal` → `just qemu`;
      `nros ws sync` → `nros sync`; add `tt-zenoh-to-cyclonedds` to bridges; add
      `qemu-riscv-nuttx` platform row (fixes the 11-vs-10 count); phase-118/131
      links → `archived/`. (Interop snippet Int32→String lands with W4.)
- [x] W1.f `examples/templates/README.md`: document all 11 templates; retire the
      `west patch` reference in zephyr-byo.
- [x] W1.g `docs/reference/c-api-cmake.md`: document the two-variable contract —
      `-DNANO_ROS_RMW` (root `add_subdirectory` knob) vs `-DNROS_RMW`
      (`NanoRosWorkspace.cmake` standalone-example shorthand).

### W2 — Test-harness + library prep (no example behavior change)

- [x] W2.a `packages/testing/nros-tests/src/output.rs`: named log-line constants +
      parsers; port ~250 raw `"Published:"`/`"Received:"` literals in
      `tests/*.rs` and `tests/zephyr/run-c.sh` so later format flips are one-file.
- [x] W2.b `nros-board-native`: expose the private `register_backend()` as
      `pub fn register_linked_rmw()` (idempotent; ThreadX twin if W2.d needs it).
- [x] W2.c `nros-node`: executor-open `log::info!` readiness line to replace
      in-example harness markers.
- [x] W2.d Spikes: (i) `nros_find_interfaces(LANGUAGE C)` under an embedded
      example CMake; (ii) unconditional `nros_platform_critical_section` on the
      ThreadX zenoh path; (iii) Cyclone unbounded-string sample on RTOS heap.
- [x] W2.e CLI: extend `nros_crate_path_lookup()` (`cmd/ws.rs`) with board crates.

### W3 — Zero cfg/ifdef in example source (23 files → 0)

- [x] W3.a 13 `native/rust/*`: board-crate dep + manifest feature forwarding +
      one unconditional `register_linked_rmw()` call; delete `#[used]` blocks;
      collapse listener dual-main; move `param-services`/`header`/`safety-e2e`
      variants to `packages/testing/nros-tests/bins/`; retarget
      `examples/fixtures.toml` + affected tests.
- [x] W3.b 6 `qemu-riscv64-threadx/rust/*`: unconditional `extern crate alloc` +
      critical-section (per W2.d); `app_main` carrier → board crate if collision.
- [x] W3.c 3 `px4/rust/xrce/*`: non-optional dep, delete vestigial cfg;
      `ws-safety-rust`: unconditional feature.
- [x] W3.d Acceptance: `grep -r '#\[cfg' examples/ --include='*.rs'`
      (excl. `generated/`) returns nothing.

### W4 — Chatter parity (one atomic change across all platforms)

- [x] W4.a All standalone talkers/listeners → `std_msgs/String`
      "Hello World: N" (count from 1), logs `Publishing: 'Hello World: N'` /
      `I heard: [Hello World: N]`. Rust `heapless::String` + `write!`; C
      `snprintf` into generated `std_msgs_msg_string`; C++ typed publisher.
- [x] W4.b Embedded C: generated `std_msgs_msg_string_serialize()` replaces
      hand-rolled CDR; type string → `"std_msgs::msg::dds_::String_"`; Cyclone
      `.msg` descriptor generation Int32 → String.
- [x] W4.c Node names: `c_talker`/`cpp_talker`/`c_listener`/`cpp_listener` →
      bare `talker`/`listener` (standalone only; workspaces keep distinct names).
- [x] W4.d Delete `"Publishing messages"` harness markers (readiness = W2.c
      line); flip W2.a constants; update `tests/zephyr/run-c.sh`; examples/README
      interop snippet → `std_msgs/msg/String`.
- [x] W4.e Fallback if W2.d(iii) fails: Cyclone-embedded examples keep Int32 +
      README note (surface the exception in examples/README matrix).

### W5 — Service/action parity + cleanliness

- [x] W5.a Service client: native takes argv `a b` (default 2 3), logs
      `Result of add_two_ints: N`; embedded sends one fixed request; drop
      hard-coded batches. Server logs the two-line `Incoming request` form.
- [x] W5.b Action logs → `action_tutorials` wording (`Sending goal` /
      `Received feedback` / `Result received:`); `NROS_ACTION_CONCURRENT`
      alternate path moves out of the example.
- [x] W5.c `native/c/talker/main.c` slims to node+pub+timer+spin; clock/param
      demos move to `nros-tests/bins/` or a dedicated parameters example;
      cpp listener `if (false)` block becomes a compile-only fixture.
- [x] W5.d Workspaces: `ws-realtime-cpp` `Ctrl.cpp` + `ws-lifecycle-c`
      `Talker.c` raw CDR → typed/generated serializers.

### W6 — Copy-out self-containedness

- [x] W6.a Flip 97 example `Cargo.toml` path-deps → `version = "*"`
      registry-style; refresh 111 `.cargo/config.toml` via `nros sync`
      (`# nros-managed`).
- [x] W6.b Standardize the CMake root guard across example CMakeLists:
      `-DNANO_ROS_ROOT` / `$ENV{NROS_REPO_DIR}` → walk-up only as last resort.
- [x] W6.c examples/README copy-out wording states the real contract (copy out →
      `nros sync` with `NROS_REPO_DIR`, or the vendored pattern per
      `templates/multi-package-workspace`).
- [x] W6.d Copy-out smoke: `cp -r` native/rust/talker outside the repo,
      `nros sync`, `cargo build`; one C example with `-DNANO_ROS_ROOT`.

### W7 — Organization + docs-of-record

- [x] W7.a Cargo.lock policy: gitignore `examples/**/Cargo.lock`, `git rm
      --cached` the 55 tracked locks (first adjust
      `scripts/ci/dep-chain-check.sh` + `scripts/check-version-lockstep.sh`).
- [x] W7.b Moves (one dir per commit; fixtures.toml + `binaries/mod.rs` in
      lockstep): `phase216-rtic-e2e` → `nros-tests/bins/rtic-run-plan-e2e`;
      `qemu-baremetal-main-e2e` → `nros-tests/bins/`; `entry-poc` →
      `nros-tests/bins/`; merge `zephyr/cpp/talker-typed` into `talker`;
      delete `px4/rust/uorb` placeholder + retire `build-sitl` recipe.
- [x] W7.c Deferred moves: `component{,-node}-poc`/`transform-poc` → phase-242
      close-out; `_entry` → `-entry` rename waits on phase-275 (else bless the
      exception in RFC-0026).
- [x] W7.d READMEs, three tiers: matrix page (+ full workspaces table);
      per-platform READMEs (~10; prereqs, just module name, RMW knob, run steps,
      case table); ≤40-line per-example READMEs only for
      variants/bridges/`ws-*`/templates — priority: the five `ws-realtime-cpp*`
      variants. Presence linted via `scripts/check-example-matrix.sh` extension.
- [x] W7.e RFC-0026 refresh (changelog entry): workspaces two-layer scheme, RMW
      flag two-layer contract, rust aemv8r carve-out, `_entry` decision, README
      tiers, Cargo.lock policy, `qemu-riscv-nuttx` partial platform.
- [x] W7.f `docs/issues/` entry for deferred naming polish (`Talker` vs
      `TalkerNode`, C++ namespace word order, `setvbuf` inconsistency).

## Status / Outcome (2026-07-03)

What landed, per wave (commit ranges on `examples-ux-overhaul`):

- **W1 — docs correctness** (`1b707fff1..85b7d1b3d`): README onboarding
  rewritten to the canonical flow; fabricated Zephyr C API block replaced with
  the real `Talker.c` surface; verb/link/SUMMARY fixes; examples/README +
  templates/README refreshed; `c-api-cmake.md` two-variable RMW contract.
- **W2 — harness + library prep** (`8b8266e75..a222740ff`, `19f02bdd6`,
  `e9e1d9718`): `nros_tests::output` log-line constants (+ `run-c.sh`
  mirrors); `register_linked_rmw()`; executor-open readiness log; W2.d spike
  verdicts (typed C gen-interfaces on Zephyr YES; ThreadX unconditional
  critical-section SAFE; Cyclone unbounded string SAFE on ddsrt); CLI board
  crate lookup.
- **W3 — zero cfg in examples** (`5c5f1352a..ea825a341`): 23 cfg-carrying
  files → 0 (whole-tree acceptance grep empty); variant fixtures extracted to
  `nros-tests/bins/` (safety-chatter-*, param-chatter-talker,
  header-chatter-talker).
- **W4 — chatter parity** (`9646045b4..f78467600` + `3eddd79a1`): every
  talker/listener on every platform speaks `std_msgs/String`
  "Hello World: N" with `Publishing:`/`I heard:`; bare node names; generated
  serializers replace hand-rolled CDR (NuttX C pair documented fallback).
  Real bugs fixed en route: codegen C++ FFI fixed-string NUL over-read
  (`8e2076d81` + regression test), stale threadx-linux fixture resolvers,
  silent RTOS chatter, Int32 type strings in interop tests.
- **W5 — service/action parity** (`b80ef5aba..6c518db20` + `d39476007`):
  add_two_ints argv shape + two-line server logs; action_tutorials wording;
  native C talker slimmed; ws raw-CDR → typed serializers.
- **W6 — copy-out self-containedness** (`691bf81b1..84544b625`): registry-style
  deps + `# nros-managed` patch blocks everywhere; standard `NANO_ROS_ROOT`
  guard; tested copy-out contract documented; live copy-out smokes (Rust +
  C) passed.
- **W7 — organization + docs-of-record** (`31e67f6c0..`): Cargo.lock policy
  (56 locks untracked, `examples/**/Cargo.lock` ignored); fixture-bin moves
  (`rtic-run-plan-e2e` ex `phase216-rtic-e2e`, `qemu-baremetal-main-e2e`,
  `entry-poc` → `nros-tests/bins/`); `talker-typed` merged into `talker`;
  `px4/rust/uorb` placeholder + `build-sitl` retired;
  `zephyr/rust/service-client-async` leftover removed; three README tiers
  (11 platform + 20 ws + bridge + template READMEs) + README lint in
  `check-example-matrix.sh`; 31-row workspaces table; RFC-0026 refresh;
  issue #132 filed.

**Issues filed during the phase:** #127 (threadx-riscv64 NULL `c_app_main`
on rebuild), #128 (rust RTOS pubsub fixture resolvers point at unbuilt
binaries), #129 (ros2-interop tests soft-pass on zero received), #130
(nros-c AtomicU64 breaks riscv32 NuttX), #131 (native zenoh service/action
query path broken at origin/main), #132 (example naming drift — deferred
polish sweep).

**Deferred, with owners:**

- `component-poc` / `component-node-poc` / `transform-poc` dir moves —
  owned by in-flight **phase-242** close-out.
- `_entry` → `-entry` rename — waits on **phase-275**; RFC-0026 blesses the
  interim exception (tracked in #132).
- ws-* copy-out smoke coverage (W6 covered standalone examples only) —
  follow-up candidate.

**Remote-CI-gated** (env-limited on the dev box; each baselined as failing
identically pre-phase):

- Zephyr cyclonedds runtime lane (W4 String flip verified by pattern +
  C-twin only; local lane env-broken).
- ThreadX riscv64 runtime (#127 — green only on stale binaries locally).
- Native zenoh service/action lane (#131 — broken at origin/main; W5 tests
  stay correctly red on the positive path until it is fixed).

## Sequencing constraints

- W4 is atomic across platforms — the zenoh keyexpr embeds the type name, so a
  talker/listener pair split across commits breaks cross-platform e2e.
- `_entry` rename waits on phase-275; component-poc moves wait on phase-242.
- W1 is independent and lands first.

## Acceptance

- `just format` + `just ci` green per wave; `mdbook build book` clean for
  W1/W7; `scripts/check-example-matrix.sh` green (incl. new README lint).
- W4: platform sweeps (`just zephyr / freertos / nuttx / threadx_linux / qemu`)
  + ROS 2 interop spot-check (`ros2 topic echo /chatter std_msgs/msg/String`).
- Zero `#[cfg` in `examples/**/*.rs` (excl. `generated/`); no tracked
  `examples/**/Cargo.lock`; no raw `Published:`/`Received:` literals in tests.
