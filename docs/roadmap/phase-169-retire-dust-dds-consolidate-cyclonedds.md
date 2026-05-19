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

- [ ] **169.1 — Audit dust-dds dependents.** Grep every
      workspace + example for `dust_dds`, `nros-rmw-dds`, and
      the `rmw-dds` feature; list every consumer that needs a
      replacement RMW. Record in this doc as a checklist.

- [ ] **169.2 — Re-target test fixtures + examples.** Every
      `nros-rmw-dds` dep in `examples/**/Cargo.toml` and
      `packages/testing/**/Cargo.toml` flips to either
      `nros-rmw-zenoh` (zenoh-pico backend already on every
      RTOS) or `nros-rmw-cyclonedds` (POSIX-host-only today).
      Phase 117's ESP32-S3 example crates flip to zenoh-pico
      since Cyclone Xtensa port doesn't exist yet.

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
