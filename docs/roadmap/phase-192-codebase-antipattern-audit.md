# Phase 192 — Codebase antipattern audit + remediation

**Goal.** Catalog and remediate maintainability/correctness antipatterns found in a
full first-party sweep: magic numbers, hardcoded paths, hand-math that should be
derived, over-long names/arg-lists, internal values that should be configurable,
and assorted debt (silent error-swallowing, god functions, dead code).

**Status.** Not Started (audit complete 2026-05-28; remediation pending).

**Priority.** Mixed — **192.1 (silent truncation) + 192.2 (wire-framing consts)
are correctness-grade (P1)**; the rest are maintainability (P2/P3).

**Depends on.** None (cross-cutting). Note: audited on `feature/phase-172`; some
findings sit in code other agents are actively editing — coordinate before
touching `nros-node/executor/*`, the codegen orchestrator, and `nros-c/cpp`
action paths.

**Method.** Read-only sweep of `packages/`, `examples/`, `scripts/`, `just/`,
`cmake/`, `justfile`, `build.rs`. Excluded `third-party/`, `*/generated/`,
`*/target/`, `*-workspace/`, `build/`, `scripts/zephyr/sdk/`. Sanctioned patterns
(per CLAUDE.md: `heapless` caps, the `nros::sizes::*` probe, `-D<SDK>_DIR=` /
`CMAKE_PREFIX_PATH` injection, `cmake/platform/` glue walk-ups, per-platform
zenohd port map, cbindgen sizes) were **not** flagged.

**Baseline (the good news).** The codebase is largely disciplined: the
zenoh/smoltcp/node config layer routes nearly every size/count/timeout through
`NROS_*`/`ZPICO_*` env vars with documented defaults (`nros-rmw-zenoh/build.rs`,
`nros-smoltcp/build.rs`, `nros-node/build.rs`, `zpico-sys/build.rs`); opaque
storage uses the `nros::sizes` probe + `div_ceil(8)` consistently. Findings
cluster in the paths that *didn't* adopt that discipline — the C/C++ + Cyclone
backends, the action wire-framing, and the board `build.rs` files.

---

## Work Items

### 192.1 — [P1, correctness] Stop silently truncating `heapless` strings
~66 `let _ = <heapless::String>.push_str(...)` / `.push(...)` in **runtime** paths
discard capacity-overflow: an over-long node name / namespace / topic **silently
truncates** → wrong key expressions on the wire (silent mis-routing), not an error.

**Files**
- `packages/core/nros-node/src/node.rs` (node name/namespace `:180,183`; FQN
  `:214-218`; topic names `:232,260`; ~throughout)
- `packages/core/nros-node/src/lifecycle_services.rs:87,109,260,544,559,612`
- `packages/core/nros-c/src/transport.rs:95,120` (teardown errors discarded, no log)

- [x] **Topic/endpoint names** (`node.rs` `create_publisher`/`create_subscriber`)
      return `NodeError::TopicNameTooLong` on overflow instead of truncating —
      these are already `Result`-returning (zero ripple). Added `NodeError::{
      TopicNameTooLong,NameTooLong,NamespaceTooLong}`.
- [x] **Node name/namespace + `fully_qualified_name` → `Result`** — done. The
      earlier "~28 callers / invasive" was an over-count (the grep conflated the
      heapless `Node`/`StandaloneNode` with `LifecyclePollingNode`, the executor
      `Node`, and rclrs-example `Node`). The real heapless-`Node::new(config)`
      callers are ~5, all test/bench (`node.rs` tests, `cdr-roundtrip-qemu`,
      `wcet-cycles-qemu`); `fully_qualified_name` has 1 (a test). So:
      `Node::new -> Result` (validates name/namespace → `NameTooLong`/
      `NamespaceTooLong`), `fully_qualified_name -> Result` (namespace 64 + `/` +
      name 64 = 129 > the `String<128>`), `Default` stays infallible
      (`.expect("default … fits")`, the `"nros_node"`/`"/"` literals always fit),
      callers updated with `.unwrap()`. `cargo check -p nros-node --tests` clean
      under `-D warnings`.
- [x] `lifecycle_services.rs` labels (`:87,109`) + the runtime `available_states`
      push (`:260`) — these take a fixed, closed set of short literals (always fit)
      and the builders are infallible (`-> MsgState`/`MsgTransition`, no `Result`),
      so a `debug_assert!(...push...().is_ok(), …)` replaces `let _ =`: loud on a
      future capacity regression, no-op in release, no API ripple. Test-only
      `let _ = …push_str` (`:544,559,612`) left as-is. `cargo check -p nros-node`
      clean under `-D warnings`.

### 192.2 — [P1, correctness] Shared CDR / action wire-framing constants
The CDR header (`CDR_HEADER_LEN=4`, `nros-serdes/src/lib.rs:49`) + GoalId
(`UUID_LEN=16`, `nros-core/src/action.rs:156`) + the seq-length prefix (`4`) are
re-inlined as raw `4`/`8`/`16` and re-derived inconsistently across C, C++ and
Cyclone. Code comments document a real off-by-N bug this caused.

**Files**
- `packages/dds/nros-rmw-cyclonedds/src/service.cpp` — raw offsets `:296` (hand
  `(pos+3)&~3` align), `:312,347-358,394-452` (`wire_cdr+4`, `+8`, `out_buf+4/12/20/24/28`)
- `packages/core/nros-cpp/src/action.rs:101,1099,1495,1744` —
  `CDR_HEADER_LEN + 4 + UUID_LEN` vs `4 + 4 + UUID_LEN` vs `CDR_HEADER_LEN + 4 + 16` (inconsistent)
- `packages/core/nros-c/src/action/{client,server}.rs` — `GOAL_ID_SEQ_PREFIX_LEN`

- [x] Canonical **`GoalId::SEQ_PREFIX_LEN`** (nros-core, next to `UUID_LEN`) — the
      single source for the `4`-byte seq prefix. `nros-cpp/src/action.rs` (4
      inlined `+ 4 +` / `4 + 4 +` / `+ 16` sites → `CDR_HEADER_LEN +
      GoalId::SEQ_PREFIX_LEN + GoalId::UUID_LEN`) and `nros-c` client/server
      (`GOAL_ID_SEQ_PREFIX_LEN = GoalId::SEQ_PREFIX_LEN`, de-duped) all route to
      it. Behavior-preserving (every site = 24). `cargo check -p nros-core nros-cpp
      nros-c` clean.
- [x] **Cyclone `service.cpp`** raw offsets named (other agent moved to Phase 193,
      so the action path was free to edit). Added a wire-framing const block —
      `kEncapLen` (= `sizeof(kCdrLeHeader)`), `kGuidBytes`/`kSeqBytes`
      (`kHeaderBytes = kGuidBytes + kSeqBytes`), `kCdrLenPrefix`, `kStatusFieldLen`,
      `kGoalUuidLen` — kept **deliberately distinct even where they equal 4**, since
      conflating the CDR encap header / a CDR length prefix / the GetResult status
      field is the exact off-by-N class this section targets. `(pos+3)&~size_t{3u}`
      → `cdr_align4()` helper. All `wire_cdr+4/+12`, `out_buf+4/12/20/24/28`,
      `+ 4 + kHeaderBytes …` strip-offsets, `8`-byte guid/seq copies, and the `16`
      goal-id length byte routed to the consts across `strip_goal_id_len_at`,
      `insert_goal_id_len_at`, `build_wire_with_header`, `split_wire_header`,
      `write_typed`, `take_typed_wire`, and both Fibonacci GetResult encode/decode
      helpers. Behavior-preserving (every offset numerically unchanged — status 20,
      count 24, data 28, min wire 28). Verified: `just cyclonedds build-rmw` clean +
      all 12 ctests pass (incl. `service_roundtrip` and the stock-ROS-2
      `ros2_srv_e2e`). Closes 192.2.

### 192.3 — [P2] build.rs source-tree walk-ups → `DEP_*`/env injection
Board/`zpico-sys` `build.rs` reach sibling first-party crates + `third-party/` by
counting `..`, no env override — the forbidden CLAUDE.md pattern. The fix pattern
exists in-repo (`nros-node` `links=`/`OUT_DIR` export).

**Files**
- `packages/boards/nros-board-freertos/build.rs:130,132`
- `packages/zpico/zpico-sys/build.rs:673,1437,1611`
- `packages/boards/nros-board-mps2-an385-freertos/build.rs:42,65,89` (incl `third-party/tracing/...` — add `TBAND_DIR`)
- `packages/boards/nros-board-nuttx-qemu-arm/nros-nuttx-ffi/build.rs:13`
- `packages/testing/nros-tests/bins/logging-smoke-nuttx-qemu-arm/build.rs:65-66`
- `packages/boards/nros-board-threadx-qemu-riscv64/build.rs:37`, `nros-board-threadx/build.rs:64`
- `packages/core/nros-cpp/CMakeLists.txt:125` (per-crate `DEFER` walk-up to root — comment/relocate to root)

- [x] DONE. First-party sibling include/src dirs centralized in **just/sdk-env.just**
      (the sanctioned home for repo-relative defaults) + read from env in build.rs
      (env_path/`env::var`): NROS_PLATFORM_{CFFI_INCLUDE,FREERTOS_SRC,POSIX_SRC,
      THREADX_SRC}, NROS_{C,CPP}_INCLUDE, NROS_LAN9118_LWIP_DIR, NROS_VIRTIO_NET_NETX_DIR;
      third-party tband via **TBAND_DIR** (`<SDK>_DIR` convention). Covered
      nros-board-{freertos,mps2-an385-freertos,nuttx-qemu-arm/nros-nuttx-ffi,
      threadx-qemu-riscv64,threadx}, zpico-sys, logging-smoke-nuttx, the shared
      `threadx_sources` helper, and nros-cpp's CMake DEFER (`${NANO_ROS_ROOT_DIR}`
      set by the root CMakeLists instead of `../../..`). Verified end-to-end:
      `just freertos build`, `just threadx_riscv64 build`, `just threadx_linux build`,
      host `cargo check -p zpico-sys` + `-p nros-board-common`.
- [x] `git grep` for `../../../packages` / `../../drivers` in `build.rs` is clean.

### 192.4 — [P2] Expose baked internals as env / `-D` / config
Backends bake values the zenoh path makes tunable; defaults even disagree.

**Files**
- `packages/core/nros-c/src/service.rs:736,2179` — `5000` ms vs zenoh's
  `NROS_SERVICE_TIMEOUT_MS` default `30000` — unify (read the env, agree on default)
- `packages/dds/nros-rmw-cyclonedds/src/session.cpp:83-119` — `kEmbeddedCycloneConfig`
  bakes buffer/stack sizes, `MaxAutoParticipantIndex 20`, the `127.0.0.1` peer —
  honor `CYCLONEDDS_URI` / `NROS_CYCLONE_*`
- `packages/dds/nros-rmw-cyclonedds/src/service.cpp:656,836,991` — `5` ms poll +
  two `5000` ms match deadlines → `NROS_CYCLONE_MATCH_{TIMEOUT,POLL}_MS`
- `scripts/qemu/setup-network.sh:47-51` — bridge name/IP/subnet → `NROS_QEMU_*`
- `scripts/setup-verus.sh:45` — `/tmp/verus-*.zip` → `${TMPDIR:-/tmp}` / project `tmp/`

- [x] C service timeout reads `NROS_SERVICE_TIMEOUT_MS`; default matches zenoh.
  nros-c `build.rs` bakes `SERVICE_DEFAULT_TIMEOUT_MS` from the env var (default
  `30000`, same as `nros-rmw-zenoh/build.rs`); `service.rs` const points at it.
  Fixed the stale "default 10000" comments in the zenoh path (code was already
  `30_000`).
- [x] Cyclone runtime profile + match timeouts tunable without recompiling.
  `session.cpp` honors a user `CYCLONEDDS_URI` (inline XML / `file://`) over the
  baked embedded profile; `service.cpp` reads `NROS_CYCLONE_MATCH_TIMEOUT_MS`
  (5000) + `NROS_CYCLONE_MATCH_POLL_MS` (5) via an embedded-safe `env_u64`
  helper (no function-local statics).
- [x] QEMU bridge subnet overridable (CI subnet-collision safe).
  `setup-network.sh` reads `NROS_QEMU_{BRIDGE,TAP_PREFIX,HOST_IP,NETMASK,NUM_TAPS}`.
  `setup-verus.sh` zip path honors `${TMPDIR:-/tmp}`.

### 192.5 — [P3] Name magic numbers / replace hand-alignment
- `packages/core/nros-cpp/src/lib.rs:596,598,728,730,829,832` — `[u8; 64]` `name`
  buffer has **no** `NROS_CPP_NAME_LEN` (namespace does); inline at 6 sites + not
  tied to `nros_node::limits` (where namespace is 128 ≠ 64 here — latent mismatch).
  `:735` `[0u8; 7]` padding.
- Inline scratch buffers: `nros-cpp/src/action.rs:103` `[0u8;512]`;
  `nros-c/src/action/client.rs:532,634` (`1024`/`512`), `server.rs:217` (`512`);
  `nros-c/src/service.rs:1631-1632` (`4096`); `nros-node/src/lifecycle_services.rs:530`
  (`4096`); `node.rs:191-192` (`1024`, should reuse `DEFAULT_TX_BUF`);
  `executor/action_core.rs:162` + `node.rs:606` (`256`, duplicated).
- Alignment: `nros-node/src/executor/spin.rs:2044` `(x+7)&!7` → `next_multiple_of`
  /`align_of::<u64>()`; `nros-rmw-cffi/src/rust_adapter.rs:64,66` `SLOT_ALIGN=16`
  vs `#[repr(align(16))]` — add `const_assert!`.
- Duplicated default: `5000` ms service timeout exists as both
  `NROS_DEFAULT_SERVICE_TIMEOUT_MS` (nros-c) and `SERVICE_DEFAULT_TIMEOUT_MS`
  (nros-rmw-zenoh) — single source.

- [x] **`NROS_CPP_NAME_LEN`** + `NROS_CPP_NODE_RESERVED` added (nros-cpp/lib.rs);
      all raw `[u8;64]`/`[0u8;64]` (name) + `[u8;7]` sites named; namespace sites
      use `NROS_CPP_NAMESPACE_LEN`. The `nros_node::limits` size mismatch is
      *documented* (not changed — it's a `#[repr(C)]` ABI value; reconcile in a
      follow-up if they must match).
- [x] **`nros-rmw-cffi`** `const _: () = assert!(align_of::<Slot>() == SLOT_ALIGN)`
      links the `#[repr(align(16))]` to the const.
- [x] **`nros-node/node.rs`** `NODE_TX_BUF_LEN`/`NODE_RX_BUF_LEN` name the `1024`
      tx/rx buffers (field decl + initializer). `cargo check` clean under `-D warnings`.
- [x] **Tail done.** `spin.rs` arena bump-alloc hand-alignment `(x+7)&!7` *and* the
      `&!(align-1)` entry align → `usize::next_multiple_of` (the trailing align spelled
      `align_of::<u64>()`). Scratch buffers named: `action_core.rs` `cancel_buffer`
      256 → `CANCEL_BUF` (decl+init de-duped) + status-array 512 → `STATUS_ARRAY_BUF`;
      `nros-cpp/action.rs` 512 → `GOAL_USER_BUF`; `nros-c` `service.rs` 4096 →
      `BLK_BUF_LEN`, `action/client.rs` 1024 → `BLK_RESULT_BUF_LEN` + feedback 512 →
      `FB_USER_BUF`, `action/server.rs` 512 → `GOAL_CB_BUF`; `lifecycle_services.rs`
      test 4096 → `ROUND_TRIP_BUF`. Each const carries a one-line rationale. Verified
      `cargo check` clean under `-D warnings` for nros-node (lib + default tests),
      nros-cpp, nros-c. The `5000` ms service-timeout dedup already folded into
      **192.4**. (Note: a pre-existing `crate::mock` unresolved-import in the
      `rmw-cffi + lifecycle-services + --tests` combo reproduces with these changes
      stashed — unrelated to 192.5, left for its owner.)

### 192.6 — [P2] Tame long arg-lists / combinatorial API names
- `codegen/.../orchestration/planner.rs:1500` `build_node_instance` — **14 params**
  → `NodeInstanceSpec` + context struct.
- `bridge/nros-bridge/src/cffi.rs:178` `nros_pubsub_bridge_create` — 11 params, two
  `(node,rmw,topic)` triples → C `nros_bridge_endpoint_t` struct.
- `codegen/rosidl-codegen/src/generator/cpp.rs:186` `render_ffi_rs` — 10 params.
- `nros/src/component_metadata.rs:699` `entity_metadata` — 7 params (adjacent `&str`).
- `nros-node/src/executor/action.rs:41-1158` — `register_action_{server,client}` ×
  `{_raw,_sized,_raw_sized,_raw_sized_on,_raw_sized_inner}` suffix explosion (8-arg
  each) → spec struct + private impl; public surface `register_action_server[_raw]`.

- [ ] Functions with ≥8 params take a spec struct; the action `_on`/`_inner` split
      moves behind a private impl.

### 192.7 — [P2] Split god functions / dedup build scripts
- `nros-node/src/executor/spin.rs:3210` `spin_once` — **~736 lines** → extract
  `drive_io`/`dispatch_ready`/`service_timers`/`compute_wait_budget`.
- `boards/nros-board-esp32/src/node.rs:85` `init_hardware` (261), `nros-board-stm32f4/src/node.rs:284`
  `setup_hardware` (206) — split the sequential bringup.
- `zpico-sys/build.rs:322` `main` (494) + helpers; `nros-cpp/build.rs:159` +
  `nros-c/build.rs:131` `generate_config` (333/295, **structurally duplicated**) →
  shared `nros-build-support` helper crate.
- `drivers/nros-smoltcp/src/bridge.rs:667` `poll` (168) — separate drain/poll/dispatch.

- [x] **`SmoltcpBridge::poll` decomposed** — phase 1 (stage→socket TX drain +
      connect kickoff) and phase 3 (socket RX → staging + post-poll TCP state
      reconcile) extracted into private `drain_socket_tx(&mut Interface, &mut
      SocketSet)` / `drain_socket_rx(&mut SocketSet)` assoc fns (verbatim moves).
      `poll` now reads as the 5-step pipeline: `drain_multicast_joins` →
      `drain_socket_tx` → `iface.poll` → `drain_socket_rx` → ACK-flush `iface.poll`
      (168 → ~30 lines). Verified `cargo check` clean on `thumbv7m-none-eabi`
      (default + `embedded-nal`) and `riscv32imc-unknown-none-elf`.
- [~] **`spin_once` decomposition — WON'T-DO (feature-conditional hazard).** The
      736-line body is dense `#[cfg]`-gated across ≥3 wake-path variants (std +
      rmw-cffi × {zephyr/freertos/nuttx/threadx} vs not, poll-only vs async-wake),
      with locals (`was_woken`, `primary_drive_timeout_ms`, `spin_start`) threaded
      across the whole body. Extract-method here would have to replicate every cfg
      gate per helper and is exactly the per-combo-breaks-silently class that bit
      192.8 (the default/all-features build passes while a partial platform combo
      fails under the examples' `-D warnings`). It is also the hottest concurrency
      path on a shared live branch (max conflict + behavior-risk surface). Not
      worth it for a P2 readability refactor.
- [~] **build.rs `generate_config` dedup — WON'T-DO (premise overstated).** The two
      bodies are *not* structurally duplicated beyond the high-level skeleton
      (probe → compute opaque sizes → write header). `diff` shows they diverge
      substantially: nros-cpp carries `CPP_CONTEXT_OVERHEAD`, action server/client
      storage, and the fat-LTO hand-math fallback (`target_pointer_bytes()`-based);
      nros-c carries the C-API knobs (`NROS_LET_BUFFER_SIZE`, service-timeout).
      A shared `nros-build-support` crate would extract a thin skeleton behind many
      params for marginal DRY while adding `[build-dependencies]` churn to two core
      crates on the shared branch. Net-negative; the duplication audit-flag was a
      false positive.
- [ ] **Board init splits** (`nros-board-esp32` `init_hardware`, `nros-board-stm32f4`
      `setup_hardware`) — deferred. Genuine extract-method targets (linear
      sequential bringup, no concurrency subtlety) but embedded-only with moved
      per-peripheral `dp.*` fields threaded through `#[cfg(feature = "ethernet")]`;
      can't be cheaply host-verified, and a mis-split breaks an embedded build the
      audit can't quickly catch. Left for a focused board-crate pass.

### 192.8 — [P3] Resolve the Phase-110 scheduler dead-code cluster
~25 `#[allow(dead_code)] // Phase 110.B.a — wired in 110.B.b …` whose referenced
sub-phases appear complete, yet the allows persist — either the integration never
landed or the annotations are stale.

**Files**
- `nros-node/src/executor/sched_context.rs` (14×), `executor/ready_set/mod.rs` (11×)
- `boards/nros-board-common/src/manifest.rs` (13× on `ArchEntry` & siblings)

- [~] Determine reachability — **WON'T-DO (the allows are load-bearing).**
      Attempted to drop all 38 (`d2a0f9a61`); **reverted** (`<this commit>`) after
      they broke the threadx example build. Root cause: the scheduler items are
      **feature-conditional dead code** — each `scheduler-*` combo leaves a
      *different* subset unused (e.g. `BucketedEdfSet::{pop_next,is_empty}`,
      `EdfReadySet::{set_bits,bits}`, a set's `{clear,is_empty,contains}` are dead
      under the minimal fifo config the threadx examples select), and the example
      crates build with `-D warnings`, so each dead item is a hard error there.
      The `#[allow(dead_code)]` suppress exactly that across the combos; the
      audit's "stale" premise was wrong. A `-D dead_code` check on nros-node's
      *default + all-scheduler* features is clean (both ends used everything) but
      **misses the partial combos** the per-platform example builds use. Properly
      removing these needs per-method `#[cfg(feature = "scheduler-…")]` gating
      (far more invasive than the annotations) and a per-platform `-D warnings`
      build matrix to verify — not worth it. Lesson: validate executor-feature
      changes with `just <plat> build` (the examples deny warnings), not just
      nros-node default/all-features.

### 192.9 — [P3] Harden runtime `unwrap`/`expect`; catalog TODO debt
- `nros-node/src/executor/spin.rs:3331` `wake_mu.lock().expect("poisoned")` (hot
  loop), `:4902` worker-spawn `expect`; `executor/handles.rs:992,1004,1725,1742`
  invariant `expect`s — recover/typed-error or document `// SAFETY-invariant`.
- TODO debt: `nros-rmw-cffi/src/rust_adapter.rs:829` (events/liveliness vtable) —
  **DONE (192.9)**: was a *stale* "TODO: wire through" header; the trampolines are
  actually wired (`register_subscriber_event`/`register_publisher_event`/
  `assert_publisher_liveliness`/`next_deadline_ms` + the 115.L event bridge) — stale
  comment removed,
  `nros-cpp/include/nros/{options,subscription}.hpp` (`message_info` not wired),
  `nros-platform-posix/src/platform.c:356` (signalfd wake), `nros-c/src/support.rs:48`
  (fault string lost), `rosidl-codegen/src/idl_generator.rs:308` (multi-dim arrays),
  `cargo-nano-ros/src/scaffold.rs:71-92` (template diversification no-op).

- [x] **Hot-path `expect`s recovered or documented.**
      - `spin.rs` `wake_mu.lock().expect("poisoned")` (the hot spin-loop wait) →
        `unwrap_or_else(|e| e.into_inner())`: the mutex guards `()` (companion to
        `wake_cv`, no shared state), so a poison cannot have corrupted anything —
        recover instead of aborting the loop.
      - `spin.rs` `OsPriorityWorker::spawn` `.expect("os-priority worker spawn")` →
        kept (the fn returns `Self`, infallible) but documented `// SAFETY-invariant`:
        spawn failure = OS thread exhaustion, runs once per priority at lazy setup
        (not a hot path), and a runtime that can't create its worker has no correct
        continuation → fail fast.
      - `handles.rs` ×4 (`PublishLoan::{as_mut,commit}`, `RecvView` `Deref`/`AsRef`)
        → `// SAFETY-invariant` comments: each `Option` is `Some` for the whole
        handle lifetime (only by-value `commit`/`discard` or `Drop` take it), so the
        `expect` is correct-by-construction, not a runtime fault.
      - Verified `cargo check -p nros-node --features rmw-cffi,rmw-lending` clean
        under `-D warnings`.
- [x] **TODO debt triaged.** All five flagged TODOs are real *feature gaps* (not
      antipatterns remediable inside 192); three already have phase/milestone homes,
      two (codegen, in the `packages/codegen` submodule) need tracking:
      | TODO | Status / home |
      |------|---------------|
      | `nros-cpp` `options.hpp:84` / `subscription.hpp:386` — `message_info` not wired | Reserved under **milestone M3.4** (the `with-info` arena path); flag exists, ignored today. Tracked. |
      | `nros-platform-posix/src/platform.c:356` — ISR-safe wake not forwarded | Already named **Phase 124.B.7.c** (signalfd/eventfd self-pipe). Tracked. |
      | `nros-c/src/support.rs:48` — backend fault string lost on catch | Real gap, no home → **needs a follow-up** (surface the panic message through the C error path). |
      | `rosidl-codegen/idl_generator.rs:308` — only first array dimension handled | Real **codegen correctness** gap (multi-dim IDL arrays) → follow-up in colcon-nano-ros. |
      | `cargo-nano-ros/scaffold.rs` — template diversification is a no-op | UX gap (every flavor emits publisher+timer); waits on the `templates/` tree → follow-up in colcon-nano-ros. |

### 192.10 — [P3] Misc infra
- `just/native.just:650` zenohd `tcp/127.0.0.1:7447` → `${ZENOH_LOCATOR}`.
- `scripts/debug/*` hardcode `tcp/127.0.0.1:7447` → read `ZENOH_LOCATOR`.
- `codegen` scaffolds emit `tcp/10.0.2.2:7447` — surface as a scaffold parameter.
- `reference/stm32f4-porting/*/src/main.rs` `ZENOH_ROUTER` literal — add "configure me" note.

- [x] Infra/debug/scaffold endpoints read the existing `ZENOH_LOCATOR` or a param.
      DONE (codegen `f3ffd13` + super): `just native zenohd` + `scripts/debug/*`
      read `${ZENOH_LOCATOR:-…}`; the scaffold nros.toml + stm32f4 porting refs
      carry a CONFIGURE-ME note. (Test fixtures in plan/planner/deploy left as-is.)

## Acceptance

- [x] 192.1 + 192.2 (correctness) landed + covered by tests. 192.1: heapless
      `Node::new`/`fully_qualified_name` + topic/lifecycle name pushes return
      `Result` on overflow (no silent truncation). 192.2: shared
      `GoalId::SEQ_PREFIX_LEN` (nros-core/cpp/c) + Cyclone `service.cpp` wire-framing
      const block; framing constants exercised by the Cyclone `service_roundtrip` /
      `ros2_srv_e2e` ctests (all 12 pass).
- [ ] `git grep` for the flagged walk-up / `/tmp` / drifted-default patterns is clean.
- [ ] No new antipatterns introduced; `just ci` green.

## Notes

- Findings are concrete (`file:line`) but not exhaustive — they're the top ~30/category
  from a fan-out audit; treat as the prioritized worklist, not a closed set.
- Sequence: do 192.1/192.2 first (correctness), coordinate `executor`/codegen/`nros-c`
  edits with the agents working `feature/phase-172`, then the maintainability items.
