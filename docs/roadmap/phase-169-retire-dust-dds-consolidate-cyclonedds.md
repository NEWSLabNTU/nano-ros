# Phase 169 — Retire dust-dds; consolidate on Cyclone DDS

**Goal.** Remove `dust-dds` as a supported RMW backend. Cyclone DDS
(`packages/dds/nros-rmw-cyclonedds/`, Phase 117 in `CLAUDE.md`) becomes
the sole DDS implementation. dust-dds was the Rust-native DDS port
that targeted `no_std + alloc` for embedded RTPS; in practice the
nostd-runtime actor-mailbox shape repeatedly blocked bring-up
(Phase 101.7, 117.2e, 117.2g, 117.2h, 166.F) and the recurring fix
cost outweighs maintaining a second DDS lineage.

**Status.** Not Started.

**Priority.** P1 — closes a long tail of vendored-submodule bugs
(166.F, 117.2d, 117.2h) by deletion. Frees Phase 117 to retarget
ESP32-S3 onto Cyclone DDS once that backend has an Xtensa port.

**Depends on.** Nothing structurally — `nros-rmw-cyclonedds` already
ships pub/sub + services on POSIX (Phase 117 in CLAUDE.md, work
items 117.1 through 117.9 in that lineage). The `cyclonedds` backend
satisfies the `rmw-dds` slot directly via the `nros_rmw_cffi`
vtable.

## Background

`dust-dds` was adopted as the Rust-native DDS implementation
(`packages/dds/dust-dds/` submodule, version 0.15.0, Phase 71). The
appeal: pure Rust, `no_std + alloc`, OMG-certified RTPS
interoperability. Reality after Phase 117 bring-up:

- **Phase 101.7** — ESP32-C3 `DcpsDomainParticipant::new` calls
  `handle_alloc_error` because the static heap budget can't fit
  ~13 builtin actors + history caches + mailboxes.
- **Phase 117.2e** — `block_on(create_participant)` hangs on
  `xtensa-esp32s3-none-elf` because the `noop_waker` poll loop is
  fused by LLVM into a single iteration; required a three-layer
  fusion barrier (cs + clock_ms + black_box + static atomic
  fetch_add + xtensa `asm!("")`) inside
  `NrosPlatformRuntime::block_on_boxed` to even get past
  participant creation.
- **Phase 117.2g** — `Executor::open`'s `Self::from_session`
  Executor return slot overflows the esp-hal default main-task
  stack on ESP32-S3.
- **Phase 117.2h / Phase 166.F** — `Actor<DcpsStatusCondition>::poll`
  hangs during the first `CreateTopic` mailbox handler. Nested
  `critical_section::with` in dust-dds's mpsc / oneshot impls
  collides with esp-hal's non-reentrant
  `critical-section[default]` restore-state on Xtensa LX7.

Each fix was load-bearing on the specific platform that hit it.
The cumulative complexity (a custom dust-dds fork carrying the
`portable-atomic-util::Arc` substitution, the regex removal, the
fusion barriers, plus the open 166.F patch) is not
maintainable long-term. Cyclone DDS — a mature C++ implementation
with an explicit `nx_bsd_*` BSD-sockets surface and a documented
embedded port story — sidesteps all four issues.

## Architecture

### What gets deleted

- `packages/dds/dust-dds/` (submodule)
- `packages/dds/nros-rmw-dds/` (the cffi shim)
- `packages/dds/nros-rmw-dds-staticlib/` (Corrosion sibling, if
  still present)
- Every `[dependencies] dust_dds = ...` and `nros-rmw-dds = ...`
  edge in the workspace
- Every `rmw-dds` Cargo feature on consumer crates (`nros`,
  `nros-node`, etc.) — replaced by the existing `rmw-cyclonedds`
  feature (or a renamed `rmw-dds` that now points to Cyclone)
- The `NROS_RMW=dust-dds` selector path in the runtime registry
- `book/src/internals/rmw-backends.md` table entry for dust-dds

### What survives

- `packages/dds/nros-rmw-cyclonedds/` (standalone CMake project,
  ships pub/sub + services + raw-CDR over Cyclone)
- `third-party/dds/cyclonedds/` submodule (pinned tag `0.10.5`)
- `just cyclonedds {setup,build,build-rmw,test,doctor,clean}`
  recipes
- All DDS-shape integration tests retargeted onto the Cyclone
  backend (the `NROS_RMW=cyclonedds` runtime path is already
  there, just needs to become the default and only path)

### What gets re-tagged for future work

Phase 117's ESP32-S3 platform / board / test infrastructure is
NOT thrown away — it's salvageable as a future
"Cyclone-DDS-on-Xtensa" effort. See Phase 117 doc revision: the
toolchain (117.0), platform crate (117.1), board crate (117.2),
example crates (117.3 — retargeted to a non-DDS RMW like zenoh
or to a future Cyclone Xtensa port), test infra (117.4), and
test fixture (117.5) all keep their content; only the
`nros-rmw-dds` dependency line in the example crates' Cargo.toml
flips to `nros-rmw-zenoh` (interim) or a future Cyclone Xtensa
build.

## Work Items

- [x] **169.1 — Audit dust-dds dependents.** Done 2026-05-19.
      Catalog landed in this doc (see "Audit results" below).

### Audit results (2026-05-19)

309 hits across the tree (excluding `build/` and `target/`
artifacts).

**A. Submodule + retire-by-deletion crates (169.4).**

- `packages/dds/dust-dds/` — entire submodule fork (carries the
  `portable-atomic-util::Arc` substitution, regex removal,
  fusion barriers, all Phase 117.2 follow-up bug fixes). Delete
  the submodule + `.gitmodules` entry.
- `packages/dds/nros-rmw-dds/` — cffi shim impl (20 `src/*.rs`
  files + tests + Cargo.toml).
- `packages/dds/nros-rmw-dds-staticlib/` — Corrosion staticlib
  sibling.

**B. Workspace root (169.4).**

- `Cargo.toml` lines 34, 36, 57, 93, 104–105, 349 — workspace
  `members` entries, dust-dds comments, and the
  `nros-rmw-dds = { ... }` workspace dep declaration.

**C. Consumer Cargo deps (169.2).**

- `packages/core/nros-cpp/Cargo.toml` — **real dep**.
  `rmw-dds-cffi` feature + `nros-rmw-dds?/platform-{posix,
  zephyr,freertos,nuttx,threadx}` + `nros-rmw-dds?/ros-{humble,
  iron}` forwards + optional workspace dep. Drop the feature +
  dep; replace `rmw-dds-cffi` callers with
  `rmw-cyclonedds-cffi`.
- `packages/core/nros/Cargo.toml` — `rmw-dds-portable-atomic`
  feature (already inert) + two prose comments. Drop feature;
  clean comments.
- `packages/core/{nros-c,nros-node,nros-platform,
  nros-platform-api,nros-platform-critical-section,
  nros-rmw-cffi}/Cargo.toml` — comments only. Clean prose.
- `packages/boards/nros-board-{esp32-qemu,mps2-an385,
  mps2-an385-freertos}/Cargo.toml` — heap-budget prose
  references. Generalize to "DDS heap budget".
- `packages/platforms/nros-platform-{esp32-qemu,mps2-an385}/Cargo.toml`
  — same prose pattern.

**D. Example Rust crates (169.2 — retarget).**

Nineteen DDS Rust example crates pulling `nros-rmw-dds`:

| Path | Replacement |
|------|-------------|
| `examples/native/rust/dds/{talker,listener,service-server,service-client,action-server,action-client}/` | Cyclone (POSIX). |
| `examples/qemu-arm-baremetal/rust/dds/{talker,listener}/` | Zenoh interim. |
| `examples/qemu-arm-freertos/rust/dds/{talker,listener}/` | Zenoh interim. |
| `examples/qemu-arm-nuttx/rust/dds/{talker,listener}/` | Zenoh interim. |
| `examples/qemu-esp32-baremetal/rust/dds/{talker,listener}/` | Zenoh interim. |
| `examples/qemu-esp32s3-baremetal/rust/dds/{talker,listener}/` *(on `phase-117.0-esp32s3-toolchain`)* | Zenoh interim. |
| `examples/qemu-riscv64-threadx/rust/dds/{talker,listener}/` | Zenoh interim. |
| `examples/threadx-linux/rust/dds/{talker,listener}/` | Cyclone (ThreadX-Linux runs Linux ELF). |
| `examples/zephyr/rust/dds/{talker,listener,service-server,service-client,service-client-async,action-server,action-client}/` | Zenoh interim (Cyclone-on-Zephyr is the open follow-up from archived Phase 117). |

**E. Example C / C++ bridges (169.2).**

- `examples/native/c/bridge/xrce-to-dds/CMakeLists.txt` —
  `nros-rmw-dds-staticlib` → Cyclone.
- `examples/native/cpp/bridge/zenoh-to-dds/CMakeLists.txt` —
  same.
- `examples/bridges/native-rust-zenoh-to-dds/Cargo.toml` +
  `src/main.rs` — flip to Cyclone.

**F. Tests (169.3).**

Ten `packages/testing/nros-tests/tests/*.rs` + Cargo.toml:

| Test | Retarget |
|------|----------|
| `baremetal_qemu_dds.rs` | Zenoh OR `#[ignore]` pending Cyclone Cortex-M3. |
| `bridge_zenoh_to_dds_e2e.rs` | DDS half → Cyclone. |
| `dds_api.rs` | Cyclone (host). |
| `dds_ros2_interop.rs` | Already exercises Cyclone; verify path. |
| `esp32_qemu_dds.rs` | `#[ignore]` post-retire. |
| `multi_rmw_bridge.rs` | DDS slot → Cyclone. |
| `server_available_e2e.rs` | DDS slot → Cyclone. |
| `threadx_riscv64_qemu_dds.rs` | Zenoh OR `#[ignore]`. |
| `zephyr.rs` | Zenoh interim. |
| `src/qemu.rs` | Drop `dust_dds` helpers. |

**G. Build orchestration (169.4).**

- `justfile` — `nros-rmw-dds` build target (line 148), four
  `--exclude nros-rmw-dds-staticlib` switches (557, 573, 602,
  620), feature comment (1330). Drop them.
- `just/zephyr.just` line 435 — comment. Clean prose.
- `scripts/check-decoupling.sh` line 10 — backend list comment.
  Drop dust-dds from list.

**H. Source-level cross-refs in non-dust-dds crates (169.4).**

| File | Nature | Action |
|------|--------|--------|
| `packages/core/nros-cpp/src/lib.rs` | `rmw-dds-cffi` cfg | Drop feature + ctor call. |
| `packages/core/nros-cpp/CMakeLists.txt` | `rmw-dds` fragment | Drop. |
| `packages/core/nros-node/src/executor/handles.rs` | Cfg refs | Drop. |
| `packages/core/nros-platform/src/{lib,resolve}.rs` | Cfg refs | Drop. |
| `packages/core/nros-platform-api/src/lib.rs` | Comment | Clean. |
| `packages/core/nros-platform-critical-section/src/lib.rs` | Comment | Clean. |
| `packages/core/nros-rmw-cffi/src/rust_adapter.rs` | Backend lookup | Drop dust-dds path. |
| `packages/dds/nros-rmw-cyclonedds/src/vtable.cpp` | Cross-ref comment | Clean. |
| `packages/xrce/nros-rmw-xrce-cffi-staticlib/src/lib.rs` | Comment | Clean. |
| `packages/platforms/nros-platform-{esp32-qemu,mps2-an385}/src/memory.rs` | Heap-size comment | Generalize. |
| `packages/boards/nros-board-nuttx-qemu-arm/src/node.rs` | Heap-size comment | Generalize. |

**I. Docs (169.6).**

- **Live (sweep prose):**
  `book/src/concepts/{no-std,platform-model,ros2-comparison}.md`,
  `book/src/getting-started/troubleshooting-first-10-min.md`,
  `book/src/internals/{platform-c-abi,rmw-backends}.md`,
  `book/src/introduction.md`,
  `book/src/porting/{custom-transport,vendor-overlay}.md`,
  `book/src/reference/{nros-toml,rmw-api}.md`,
  `book/src/user-guide/{configuration,cross-backend-bridges,rmw-backends}.md`,
  `docs/design/{rt-execution-model,zero-copy-raw-api}.md`,
  `docs/development/crates-io-metadata-audit.md`,
  `docs/research/{phase-111-B1-crates-io-metadata-audit,rmw-c-abi-coverage}.md`.
- **Roadmap (live):**
  `phase-117-esp32s3-qemu-dds.md` (already banner-updated),
  `phase-145-cache-discipline-for-user-projects.md`,
  `phase-161-cpp-freertos-transport-error.md`,
  `phase-168-zephyr-rmw-collapse.md`.
- **Archived (leave frozen):** 20 archived phase docs
  reference dust-dds; no edits.

**J. CLAUDE.md (169.6).** Already retired in commit
`087e48f20` (rebased as `68e259e2c` on `main`).

### Summary

| Category | Count | Lineage |
|----------|------:|---------|
| Submodule + retire-by-deletion crates | 3 | 169.4 |
| Workspace root edits | 1 file | 169.4 |
| Consumer Cargo deps (real) | 1 (`nros-cpp`) | 169.2 |
| Consumer Cargo deps (prose-only) | 9 files | 169.2 |
| Example Rust crates | 19 dirs | 169.2 |
| Example C/C++ bridges | 3 dirs | 169.2 |
| Tests | 10 files + Cargo.toml | 169.3 |
| Build orchestration | 3 files | 169.4 |
| Source cross-refs | 12 files | 169.4 |
| Live docs | 22 files | 169.6 |
| Archived docs (no edit) | 20 files | n/a |

- [x] **169.2 — Rust DDS examples deleted (2026-05-19).**
      Original plan was a Cargo.toml flip from `nros-rmw-dds`
      to `nros-rmw-zenoh` or `nros-rmw-cyclonedds`, but
      Cyclone has no Rust shim (the backend is a CMake/C++
      project consumed via `nros_rmw_cyclonedds_register()`
      at the C/C++ ABI layer), and Zenoh retargeting would
      have duplicated existing `examples/{platform}/rust/zenoh/`
      siblings 1-for-1. Decision per user input: **delete all
      19 Rust DDS example dirs** + the one Rust bridge
      (`examples/bridges/native-rust-zenoh-to-dds/`). They get
      re-created in Phase 169.5 / 169.9 once a Rust→Cyclone
      shim crate (working name `nros-rmw-cyclonedds-sys`)
      lands.

      **Deleted (this commit):**
      - `examples/native/rust/dds/{talker,listener,service-server,service-client,action-server,action-client}/` — 6 dirs.
      - `examples/qemu-arm-baremetal/rust/dds/{talker,listener}/`
      - `examples/qemu-arm-freertos/rust/dds/{talker,listener}/`
      - `examples/qemu-arm-nuttx/rust/dds/{talker,listener}/`
      - `examples/qemu-esp32-baremetal/rust/dds/{talker,listener}/`
      - `examples/qemu-riscv64-threadx/rust/dds/{talker,listener}/`
      - `examples/threadx-linux/rust/dds/{talker,listener}/`
      - `examples/zephyr/rust/dds/{talker,listener,service-server,service-client,service-client-async,action-server,action-client}/` — 7 dirs.
      - `examples/bridges/native-rust-zenoh-to-dds/` — Rust bridge.

      Workspace `exclude` list cleaned (~25 entries removed).
      `cargo metadata --no-deps` validates.

      **NOT deleted (covered by other 169 work items):**
      - `examples/native/{c,cpp}/dds/*` — C/C++ DDS examples
        consume Cyclone via CMake; survive untouched until
        Phase 169.4 verifies the Cyclone link path.
      - `examples/zephyr/{c,cpp}/dds/*` — same.
      - `examples/native/c/bridge/xrce-to-dds/`,
        `examples/native/cpp/bridge/zenoh-to-dds/` — C/C++
        bridges link `nros-rmw-dds-staticlib` today; flipped
        to Cyclone in 169.4 (or marked Won't-Do if no
        equivalent staticlib shape exists).
      - `examples/qemu-esp32s3-baremetal/rust/dds/*` — exists
        only on the `phase-117.0-esp32s3-toolchain` archaeology
        branch; that branch never merges to main.

- [ ] **169.3 — Re-target integration tests.** Every
      `packages/testing/nros-tests/tests/*_dds.rs` that hits
      dust-dds gets retargeted onto Cyclone. The
      `esp32s3_qemu_dds.rs` / `esp32_qemu_dds.rs` tests need
      to either retarget onto zenoh-pico (lower-friction) or
      be marked `#[ignore]` until a Cyclone Xtensa port lands.

- [ ] **169.4 — Delete `nros-rmw-dds` + sibling crates.** Once
      no consumer references them, remove
      `packages/dds/nros-rmw-dds/`,
      `packages/dds/nros-rmw-dds-staticlib/`, and the
      `dust-dds` submodule at `packages/dds/dust-dds/`.
      Update workspace root `Cargo.toml` members list.

- [ ] **169.5 — Promote Cyclone DDS to "the DDS backend".**
      Rename `nros-rmw-cyclonedds` → `nros-rmw-dds` (or keep
      the current name and add an alias for the `rmw-dds`
      slot — pick the less invasive). Update the registry
      naming + `book/src/internals/rmw-backends.md`. Either
      way the `NROS_RMW=dds` selector must work.

- [ ] **169.6 — Update CLAUDE.md.** Drop the "dust-dds=Rust"
      entry from the RMW host-language policy table.
      Consolidate the Phase 117 cross-reference (the Cyclone
      DDS line referenced in CLAUDE.md is the canonical
      Phase 117 lineage; the ESP32-S3 lineage retargets onto
      zenoh in 169.2).

- [ ] **169.7 — Update Phase 117 doc.** Add a banner: "DDS
      pubsub bits retired (Phase 169). ESP32-S3 platform +
      board + test infra preserved; example crates retarget
      onto zenoh until a future Cyclone Xtensa port lands."
      Mark 117.2d / 117.2h as **Won't-Do** with cross-ref to
      Phase 169. Leave 117.0–117.5 + 117.2b/c marked done
      since the infrastructure stands on its own.

- [ ] **169.8 — Close Phase 166.F.** Mark Won't-Fix with
      cross-ref to Phase 169. Same for any other
      dust-dds-rooted open issue.

- [ ] **169.9 — Cyclone DDS Xtensa port (deferred).** Track
      separately when the demand arises. Cyclone DDS has an
      existing embedded port story via `nx_bsd_*`-style BSD
      sockets; the work is a Phase-117-equivalent for the
      Cyclone backend (platform crate, board crate, esp-idf
      build glue, QEMU smoke). NOT in scope here.

## Acceptance Criteria

- [ ] `cargo tree -p nros-rmw-dds` fails (crate removed).
- [ ] `git submodule status` shows no `dust-dds` entry.
- [ ] Workspace `Cargo.lock` contains no `dust_dds` package.
- [ ] `just ci` green on all retargeted tests / examples.
- [ ] `book/src/internals/rmw-backends.md` lists only
      `cyclonedds`, `zenoh-pico`, `xrce` (`dust-dds` row
      gone).
- [ ] Phase 117 doc reflects the retargeting; 117.2h marked
      Won't-Do.
- [ ] Phase 166.F marked Won't-Fix.

## Notes

- **Why not patch dust-dds.** Two paths to closing 117.2h were
  recorded in 166.F: patch the actor mailbox shape, or swap
  esp-hal's critical-section impl. Both are load-bearing on a
  vendored submodule we don't control and would only fix ONE
  of four open dust-dds bugs (101.7 heap budget, 117.2g
  Executor stack overflow, 117.2h actor poll deadlock, 117.2d
  PSRAM atomic). Retirement closes all four by deletion.
- **Why Cyclone over dust-dds long-term.** Cyclone is a mature
  C++ implementation with stock `rmw_cyclonedds_cpp` wire
  interop already validated (Phase 117 lineage in CLAUDE.md).
  Embedded port story exists (NetX Duo BSD shim, esp-idf
  ports in the wild). nano-ros's `nros_rmw_cffi` vtable
  already wraps it via `packages/dds/nros-rmw-cyclonedds/`
  (Phase 117.1–117.9 done).
- **Why not zenoh for everything.** Zenoh remains a peer
  transport (rmw-zenoh) — different wire protocol (zenoh
  protocol over UDP/TCP/serial), different broker
  requirement (zenohd router). DDS / RTPS is the ROS 2
  interop wire; we need a DDS lineage for stock ROS 2 peer
  interop. Cyclone fills that slot; zenoh fills the
  brokered / embedded-only slot.
