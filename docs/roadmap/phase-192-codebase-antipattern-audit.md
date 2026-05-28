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
- [ ] **Node name/namespace** (`Node::new` `:180,183`) + `fully_qualified_name`
      (`:214-218`) — `Node::new` is infallible with ~28 call sites across
      heapless/lifecycle/executor `Node` types; converting to `Result` is an
      invasive API migration → deferred (coordinate on the shared branch; not done
      surgically to avoid conflicts).
- [ ] `lifecycle_services.rs` transition-label `push_str` (`:87,109`) — lower
      priority (labels, not wire keys); convert where the fn already returns `Result`.

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
- [ ] **Cyclone `service.cpp`** raw `4`/`8` offsets + `(pos+3)&~3` →
      `sizeof(kCdrLeHeader)` / `kGuidBytes` / a `cdr_align4()` helper — **deferred**:
      delicate C++ CDR/result bridge (validated in 184.8/186) in the action path
      other agents are actively editing; do as a coordinated follow-up.

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

- [ ] Sibling-crate includes injected via `links=`+`cargo:include=` → `DEP_*`, or
      `NROS_*_DIR` env from the recipe; `third-party` via `<SDK>_DIR`.
- [ ] `git grep` for `../../../packages` / `../../drivers` in `build.rs` is clean.

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
- [ ] **Deferred (hot files, agents' active area):** `spin.rs:2044` `(x+7)&!7` →
      `next_multiple_of`; scratch buffers in `action.rs`/`action/*`/`service.rs`/
      `lifecycle_services.rs` (`512`/`1024`/`4096`/`256`). The `5000` ms
      service-timeout dedup folds into **192.4**.

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

- [ ] `spin_once` decomposed; `generate_config` dedup'd into a shared crate.

### 192.8 — [P3] Resolve the Phase-110 scheduler dead-code cluster
~25 `#[allow(dead_code)] // Phase 110.B.a — wired in 110.B.b …` whose referenced
sub-phases appear complete, yet the allows persist — either the integration never
landed or the annotations are stale.

**Files**
- `nros-node/src/executor/sched_context.rs` (14×), `executor/ready_set/mod.rs` (11×)
- `boards/nros-board-common/src/manifest.rs` (13× on `ArchEntry` & siblings)

- [ ] Determine reachability; wire it in or delete + drop the allows.

### 192.9 — [P3] Harden runtime `unwrap`/`expect`; catalog TODO debt
- `nros-node/src/executor/spin.rs:3331` `wake_mu.lock().expect("poisoned")` (hot
  loop), `:4902` worker-spawn `expect`; `executor/handles.rs:992,1004,1725,1742`
  invariant `expect`s — recover/typed-error or document `// SAFETY-invariant`.
- TODO debt: `nros-rmw-cffi/src/rust_adapter.rs:829` (events/liveliness vtable),
  `nros-cpp/include/nros/{options,subscription}.hpp` (`message_info` not wired),
  `nros-platform-posix/src/platform.c:356` (signalfd wake), `nros-c/src/support.rs:48`
  (fault string lost), `rosidl-codegen/src/idl_generator.rs:308` (multi-dim arrays),
  `cargo-nano-ros/src/scaffold.rs:71-92` (template diversification no-op).

- [ ] Hot-path `expect`s recovered or documented; TODO debt triaged (file follow-up
      phases for the real gaps).

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

- [ ] 192.1 + 192.2 (correctness) landed + covered by tests (overflow → error;
      framing constants exercised by the action e2e suite).
- [ ] `git grep` for the flagged walk-up / `/tmp` / drifted-default patterns is clean.
- [ ] No new antipatterns introduced; `just ci` green.

## Notes

- Findings are concrete (`file:line`) but not exhaustive — they're the top ~30/category
  from a fan-out audit; treat as the prioritized worklist, not a closed set.
- Sequence: do 192.1/192.2 first (correctness), coordinate `executor`/codegen/`nros-c`
  edits with the agents working `feature/phase-172`, then the maintainability items.
