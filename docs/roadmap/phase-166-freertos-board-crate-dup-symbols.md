# Phase 166 — Code regressions surfaced by audit-reader agents

**Goal.** Land the code-side fixes for issues the multi-platform
audit-reader pass found while following starter pages. The book-side
docs are corrected as the audits run; this phase tracks the
underlying code / build bugs.

## Open issues

| # | Module / file | Symptom | Severity | Status |
|---|---|---|---|---|
| 166.A | `nros-board-freertos` + `nros-board-mps2-an385-freertos` | Duplicate `nros_platform_*` symbols at link time | P1 — blocks FreeRTOS DDS build | **Fixed** — mps2 build.rs no longer re-emits `cargo:rustc-link-lib` mirrors |
| 166.B | `nros-log` on `riscv32imc-none-elf` | `AtomicPtr::compare_exchange` not available — needs `portable-atomic` | P1 — blocks esp-hal Rust build | **Already-fixed / worktree artifact** — in-tree `nros-log` already uses `portable_atomic::*`; esp32 talker builds clean on `main` |
| 166.C | `examples/native/{cpp,c}/zenoh/talker/` CMake | Transitive submodule fetch pulls dust-dds + px4-rs even for posix+zenoh; first build aborts | P2 | **Worktree artifact** — submodules present + populated on `main` |
| 166.D | `examples/threadx-linux/rust/zenoh/talker/Cargo.toml` (and siblings) | Missing empty `[workspace]` table — `cargo build` from inside the repo discovers parent workspace + crate not listed → error | P2 | **Worktree artifact** — all example Cargo.tomls verified on `main` (either in root workspace `members` or have own `[workspace]`) |
| 166.E | `integrations/nuttx/` template | `external-Kconfig.in` + `external-Make.defs.in` staging step not wired into `just nuttx setup`; user following the shell-symlink instruction can't reach example apps via menuconfig | P3 | **Fixed** — `just nuttx setup` now invokes `scripts/nuttx/stage-external-apps.sh` |
| 166.F | ~~`packages/dds/dust-dds/dds/src/dcps/actor.rs` + nros-rmw-dds nostd runtime~~ | ~~`Actor<DcpsStatusCondition>::poll` blocks during the first `CreateTopic` mailbox handler on `xtensa-esp32s3-none-elf` (Phase 117.2h). Blocks two-instance ESP32-S3 QEMU DDS E2E.~~ **Won't-Fix — superseded by Phase 169** (Retire dust-dds; consolidate on Cyclone DDS). Closed by deleting the dust-dds submodule, not by patching it. | Closed |

---

## 166.A — Duplicate `nros_platform_*` symbols across FreeRTOS board crates

Eliminate the duplicate-symbol linker error when both
`nros-board-freertos` (common FreeRTOS overlay) and a board-specific
overlay (`nros-board-mps2-an385-freertos`, eventual STM32F4 / NXP /
TI variants) are pulled into the same binary. The `platform.c` C
body that exports the canonical `nros_platform_*` ABI is being
compiled twice with non-weak linkage.

**Status.** Fixed 2026-05-19. Root cause was `nros-board-mps2-an385-freertos`'s
build.rs re-emitting `cargo:rustc-link-lib=static=...` lines for the
four archives compiled by `nros-board-freertos` (kept in for "link-order
control"). With Rust's default `+bundle` on static link-libs, the
mirror lines cause cargo to bundle the same `.a` into BOTH rlibs,
yielding the `duplicate symbol` linker error. Fix: drop the mirror
lines; let cargo's normal dep-chain propagation handle them.

**Priority.** P1 — blocked `just freertos build-fixtures` for the
Rust DDS example.

**Depends on.** Nothing.

---

## Symptom

```
$ just freertos build-fixtures
…
rust-lld: error: duplicate symbol: nros_platform_clock_ms
>>> defined at platform.c
>>>            91eeb4584ba792b3-platform.o:(nros_platform_clock_ms)
       in archive .../libnros_board_mps2_an385_freertos-…rlib
>>> defined at platform.c
>>>            91eeb4584ba792b3-platform.o:(nros_platform_clock_ms)
       in archive .../libnros_board_freertos-…rlib

rust-lld: error: duplicate symbol: nros_platform_task_init
>>> …
… (~20 more `nros_platform_*` symbols)
…
error: could not compile `qemu-freertos-dds-listener`
       (bin "qemu-freertos-dds-listener")
```

## Root cause

Both crates compile `packages/core/nros-platform-freertos/src/platform.c`
into their respective rlibs. Each definition exports the
`nros_platform_*` C ABI without `weak` linkage, so when the linker
walks both rlibs it sees two strong defs for every function.

- `packages/boards/nros-board-freertos/build.rs` invokes the
  platform compile.
- `packages/boards/nros-board-mps2-an385-freertos/` depends on
  `nros-board-freertos` AND also compiles `platform.c` (via its
  Cargo dep on `nros-platform-freertos` whose `build.rs` rebuilds
  it).

End state: two strong defs reach the linker, rust-lld refuses to
pick one.

## Fix options

Pick exactly one:

1. **Only the board-specific overlay emits the C body.**
   `nros-board-freertos` stops compiling `platform.c`; the
   board-specific crate that depends on it picks the compile up.
   Pro: minimal, board-specific build steps already exist.
   Con: every future board crate has to remember to compile it.

2. **Only `nros-platform-freertos` emits the C body.**
   `nros-board-freertos` stops compiling `platform.c` AND
   `nros-board-mps2-an385-freertos` does NOT add it either.
   `nros-platform-freertos`'s own `build.rs` produces a staticlib
   that every consumer links against.
   Pro: canonical — one platform crate, one C body.
   Con: needs build-script reshape to actually emit a staticlib,
   not just a `cc::Build` invocation.

3. **Gate `platform.c` emission behind a Cargo feature.**
   `nros-platform-freertos` exposes `emit-c-port` (default `on`).
   Board overlays that want to emit themselves opt out via
   `default-features = false`.
   Pro: flexible — opt-in / opt-out per consumer.
   Con: another feature axis to remember; misconfiguration leaves
   `undefined-symbol` instead of `duplicate-symbol`.

Recommend option **2** (canonical platform crate emits, board crates
consume). Matches the platform-cffi pattern documented in
`book/src/internals/platform-c-abi.md`.

## Work items

- [x] **166.1** Audit which crates currently compile `platform.c`.
      Outcome: only `nros-board-freertos/build.rs:138` invokes the
      compile. mps2 doesn't compile it but DOES re-emit the matching
      `cargo:rustc-link-lib=static=nros_platform_freertos` line,
      which (with `+bundle` default) causes cargo to bundle the
      `.a` from `nros-board-freertos`'s OUT_DIR into the mps2 rlib
      too — duplicate copies of `platform.o` end up in both rlibs.
- [x] **166.2** Fix: drop the four mirror `rustc-link-lib` lines
      from `nros-board-mps2-an385-freertos/build.rs`. Kept only the
      board-specific ones (`startup`, `lan9118_lwip`). Cargo's
      normal dep-chain propagation carries the generic archives.
- [x] **166.3** Verified via `just freertos build-fixtures` —
      all Rust zenoh + Rust DDS fixtures build cleanly.
- [x] **166.4** Audited all other RTOS overlay patterns 2026-05-19.
      Survey of `packages/boards/*/build.rs` × `Cargo.toml` deps:
      - `nros-board-threadx-linux → nros-board-threadx`: parent
        compiles `threadx_kernel` + `nros_platform_threadx`; child
        emits only `glue` + `nsos_netx`. No overlap. Verified
        `just threadx_linux build` links clean.
      - `nros-board-threadx-qemu-riscv64 → nros-board-threadx`:
        same parent; child emits `glue`, `virtio_net_netx`,
        `netxduo`, `threadx_port_asm`. No overlap. Verified
        `just threadx_riscv64 build` links clean.
      - `nros-board-nuttx-qemu-arm`: standalone (no sibling board
        dep). Compiles + emits its own `nros_platform_nuttx`.
        No risk.
      - `nros-board-orin-spe`: standalone. No risk.
      - `nros-board-fvp-aemv8r-smp`: Zephyr CMake path, no Cargo
        sibling-board dep. Out of scope.
      - `nros-board-stm32f4`, `nros-board-mps2-an385`,
        `nros-board-esp32{,-qemu}`: no `cc::Build::compile` chains
        or no sibling board deps. No risk.
      Only `nros-board-mps2-an385-freertos` (166.A) had the bug.
- [ ] **166.5** Regression test in `nros-tests`: assert no
      `cargo:rustc-link-lib=static=X` line names an archive that
      a transitive dep already compiles.

## Files (likely touched)

- `packages/core/nros-platform-freertos/build.rs` (new — turn
  current `cc::Build` into a staticlib emission)
- `packages/boards/nros-board-freertos/build.rs` (drop the
  platform.c compile)
- `packages/boards/nros-board-mps2-an385-freertos/Cargo.toml`
  (confirm it pulls in the staticlib via the platform crate, not
  via its own compile)

## Acceptance criteria

- [ ] `just freertos build-fixtures` runs clean — all 20 binaries
      (rust + c + cpp × {pubsub, service, action} + DDS pair) build.
- [ ] `cargo build` from each `examples/qemu-arm-freertos/<lang>/<rmw>/<ex>/`
      links without `duplicate symbol` errors.
- [ ] `nm libnros_board_mps2_an385_freertos-*.rlib` shows zero
      `nros_platform_*` symbols defined in the rlib (they should
      resolve from the platform crate's staticlib).
- [ ] No regression on other RTOS targets that exercise the same
      board-overlay pattern.

## Notes

- This regression most likely landed during one of the Phase
  121/129 platform-cffi reshape commits. Surfaced by the user-as-
  tester agent against the FreeRTOS starter on 2026-05-19.
- The symptom is masked when only ONE board crate participates
  in a binary (the Rust zenoh examples that bypass
  `nros-board-freertos` still build cleanly because they only
  pull in `nros-board-mps2-an385-freertos`).
- **Phase 88.16.H is blocked on this.** A direct
  `[plat_log_write] writer=0` probe confirmed that on the FreeRTOS
  C/C++ example chain, `nros_platform_log_write` reads
  `s_log_writer` from a *different* file-static than the one the
  Rust-side board crate's `register_log_writer` call writes. Two
  compilations of `nros-platform-freertos/src/platform.c` are
  live in each binary — one from
  `libnros_platform_freertos.a` (cmake) and one from
  `libnros_rmw_zenoh_staticlib.a` / `libnros_c.a` (Cargo via
  Corrosion). The linker dedups the EXPORTED
  `nros_platform_{register_log_writer,log_write}` to one archive,
  but the other archive's file-static `s_log_writer` is what
  the surviving function addresses. Phase 166 option-2 (canonical
  platform crate emits a staticlib, board + cmake build steps
  consume it) collapses the dup and unblocks 88.16.H's example
  migration with no further nros-log changes needed.

---

## Non-passing test inventory (snapshot 2026-05-19)

Cataloged during the Phase 88.16.B verification sweep. Pulled from a
full grep of `packages/testing/nros-tests/tests/*.rs`. Two classes:
hard-coded `#[ignore]` markers (test runner reports `ignored`) and
prerequisite skips through `nros_tests::skip!(...)` (test runner
reports `[SKIPPED]` via panic+prefix). None of these are caused by
Phase 88 / nros-log; they predate it.

### Permanently `#[ignore]`'d — needs upstream / other-phase fix

| Test | Reason | Tracking |
|---|---|---|
| `actions::test_action_server_client_communication` | blocking `zpico_get` in `send_goal` returns `Timeout` immediately on native | Phase 77 |
| `native_api::test_c_action_communication` | same root cause | Phase 77 |
| `native_api::test_c_rust_service_interop` | blocking `zpico_get` in service call returns `Timeout` | Phase 77 |
| `nuttx_qemu::test_nuttx_cpp_talker_builds` | NuttX C/C++ CMake build blocked by upstream libc missing `_SC_HOST_NAME_MAX` | NuttX upstream |
| `nuttx_qemu::test_nuttx_cpp_listener_builds` | same | NuttX upstream |
| `nuttx_qemu::test_nuttx_cpp_service_server_builds` | same | NuttX upstream |
| `nuttx_qemu::test_nuttx_cpp_service_client_builds` | same | NuttX upstream |
| `nuttx_qemu::test_nuttx_cpp_action_server_builds` | same | NuttX upstream |
| `nuttx_qemu::test_nuttx_cpp_action_client_builds` | same | NuttX upstream |
| `esp32_qemu_dds::test_esp32_qemu_dds_rust_talker_to_listener_e2e` | dust-dds `DcpsDomainParticipant` builtin entity count overflows ESP32-C3 heap budget; Phase 101 deferral | Phase 101 follow-up |
| `freertos_qemu_dds::test_freertos_dds_rust_talker_to_listener_e2e` | gates flipping 97.4.freertos to done; runtime smoke deferred | Phase 97.4 |

**Phase 77 status:** "In Progress (77.1–77.5 done)" per
`docs/roadmap/archived/phase-77-async-action-client.md`. Three
`#[ignore]` markers above clear when 77.6+ lands.

### Prerequisite-gated `nros_tests::skip!(...)` — environmental

These pass when the listed dependency is installed / running; they
print `[SKIPPED] <reason>` and exit otherwise. Skip frequency
across the suite (full grep, with-duplicates):

| Skip reason | Count | Unblocker |
|---|---|---|
| zenohd not found | 78 | `just zenohd build` (artefact at `build/zenohd/zenohd`) |
| Zephyr not available | 49 | `just zephyr setup` |
| XRCE agent not available | 33 | `just xrce setup` |
| ROS 2 not found | 20 | `source /opt/ros/humble/setup.bash` |
| cmake not found | 12 | distro-level `cmake` install |
| zenoh-pico arm build not available | 10 | `just qemu build-zenoh-pico-arm` |
| `require_nuttx_cpp` check failed | 6 | NuttX C/C++ block (see above) |
| ROS 2 DDS not available | 5 | ROS 2 install + rmw_cyclonedds_cpp |
| DDS talker / listener binary missing | 10 | `just <plat> build-fixtures` |
| west command not available | 4 | `pip install west` |
| socat not available | 3 | distro `socat` install |
| riscv32 target not available | 3 | `rustup target add riscv32imc-unknown-none-elf` |
| `require_esp32_networked` check failed | 3 | `just esp32 setup` |
| qemu-system-arm too old for `-netdev dgram unix` | 2 | `just qemu setup-qemu` (patched binary) |
| Patched qemu-system-arm not built | 2 | `just qemu setup-qemu` |
| ThreadX (Linux + RV64) DDS prereq | 2 | `just threadx_{linux,riscv64} setup` |
| `require_threadx{,_riscv64}` check failed | 2 | same |
| `require_nuttx` / `require_freertos` check failed | 2 | `just {nuttx,freertos} setup` |
| `qemu-system-riscv32` not available | 1 | distro `qemu-system-misc` install |
| `pio` CLI not on PATH | 1 | `pip install platformio` |
| Phase 138.6 zephyr cell deferred | 1 | Phase 139 work |
| openssl not available — cannot generate TLS certs | 1 | distro `openssl` install |
| NUTTX_DIR unset | 1 | env var |
| `idf.py` not on PATH | 1 | `just esp_idf setup` |
| `espflash` not available | 1 | `cargo install espflash` |
| `arm-none-eabi-gcc` not on PATH | 1 | distro `gcc-arm-none-eabi` install |
| bare-metal DDS prerequisites not available | 1 | `just qemu setup` |
| zenoh-pico arm build (bridge variant) | 1 | `just qemu build-zenoh-pico-arm` |

### Pure-fixture skip (separate failure mode)

`xrce::test_xrce_large_message_publish` fails (not skip!) with
`Test fixture binary not prebuilt: .../target/release/xrce-large-msg-test`.
Resolves with `just build-test-fixtures`. The xrce harness panics
on the missing path because the test was written before the
`skip!` macro existed.

### Survey gap — broader ignored / skipped set this snapshot
misses

This inventory was gathered from a static grep, not from a full
`just test-all` run. Real run-time counts vary by:
- which optional services are running (zenohd, xrce-agent),
- which platform toolchains are installed,
- which fixtures are prebuilt (`just build-test-fixtures`).

Run `cargo nextest run --workspace 2>&1 | tee tmp/nextest.log` and
grep for `SKIPPED` + `ignored` to refresh this snapshot before
treating it as canonical.

### Recommended dispositions

- **Phase 77 trio (3 tests)** — keep ignored; resolution gated on
  Phase 77.6+.
- **NuttX C/C++ block (6 tests)** — keep ignored; resolution
  gated on the upstream NuttX libc patch.
- **DDS RTOS smoke (2 tests)** — keep ignored; Phase 97.4 /
  Phase 101 cleanups; un-skip when their respective work items
  flip to done.
- **Environmental skips (77 + 49 + …)** — leave as is; these are
  desirable (CI / dev hosts without the dep should skip cleanly).
- **xrce large-message fixture** — convert from raw panic to
  `nros_tests::skip!("xrce-large-msg-test fixture not prebuilt; run
  `just build-test-fixtures`")` so the test reports `[SKIPPED]`
  instead of `FAILED` when fixture is missing. Trivial follow-up.

---

## 166.B — `nros-log` AtomicPtr CAS on `riscv32imc-none-elf`

**Status.** Closed 2026-05-19 — worktree artifact.

The audit-reader agent reported `AtomicPtr<Logger>::compare_exchange`
and `AtomicBool::compare_exchange` unavailable on `riscv32imc-unknown-none-elf`.
Verification on `main`:

- `packages/core/nros-log/src/lib.rs` lines 47 + 449 already import
  from `portable_atomic::{AtomicPtr, AtomicU8, AtomicBool}`, not
  `core::sync::atomic`.
- `packages/core/nros-log/Cargo.toml` declares
  `portable-atomic = { version = "1", default-features = false }`.
- `packages/boards/nros-board-esp32/Cargo.toml` enables
  `portable-atomic` with `features = ["unsafe-assume-single-core"]`
  plus `critical-section = "1"`, so feature unification gives the
  esp-hal binary a CAS polyfill via critical section.
- `cargo build --release` from
  `examples/esp32/rust/zenoh/talker/` succeeds on `main` with no
  errors (verified 2026-05-19, ~29s build, `riscv32imc-unknown-none-elf`).

The agent's failure was a worktree-isolation artifact (similar to
166.C/.D). No code change required.

### Work items

- [x] **166.B.1** Verified `portable-atomic` already in
      `nros-log` Cargo.toml.
- [x] **166.B.2** Verified `nros-log` already uses
      `portable_atomic::{AtomicPtr, AtomicBool}` at call sites.
- [x] **166.B.3** Verified `cargo build` from
      `examples/esp32/rust/zenoh/talker/` succeeds on `main`.
- [ ] **166.B.4** (Deferred follow-up) Add a CI matrix entry that
      cross-compiles `nros-log` for `riscv32imc-unknown-none-elf`
      so future regressions surface in CI rather than worktree
      audits.

---

## 166.C — Transitive submodule fetch on first CMake build

**Status.** Closed 2026-05-19 — worktree artifact.

Verified on `main`: `third-party/dust-dds/dds/Cargo.toml` +
`third-party/px4/px4-rs/tests/sitl/Cargo.toml` are present and
populated. The agent's failure was a worktree-isolation artifact —
the temporary worktree didn't inherit the parent's submodule state
the same way `git worktree add` does for tracked files.

If a fresh-clone user actually hits this, the canonical remedy is
`just setup` (which runs `git submodule update --init --recursive`
on the active tier's submodules). Documented in
`book/src/getting-started/installation.md`.

---

## 166.D — Standalone-example `[workspace]` table missing

**Status.** Closed 2026-05-19 — worktree artifact.

Verified on `main`: every example Cargo.toml is either listed in
the root `Cargo.toml` `[workspace.members]` array OR declares its
own `[workspace]` table. The agent's failure was a worktree-isolation
artifact — the temporary worktree's `Cargo.toml` snapshot differed
from `main`.

A CI lint asserting "every `examples/**/Cargo.toml` either appears
in root workspace `members` OR has its own `[workspace]` table" is
a reasonable follow-up (caught the failure mode that would surface
if a future commit broke the invariant).

---

## 166.E — NuttX `external-Kconfig.in` staging step missing

The NuttX integration shell at `integrations/nuttx/` carries
`external-Kconfig.in` + `external-Make.defs.in` templates that
need to be staged into `$NUTTX_APPS/external/` (alongside the
`nano-ros/` symlink) for example apps to appear under
`menuconfig → Application Configuration → External Modules`. The
book's "wire the shell once via symlink" instruction is incomplete
— users following it literally get the nano-ros library app but
not the example apps the page promises.

### Work items

- [x] **166.E.1** Fold the staging step into `just nuttx setup`.
      The existing `scripts/nuttx/stage-external-apps.sh` is now
      invoked from the setup recipe after `build-kernel`, gated
      on `$NUTTX_APPS_DIR/Make.defs` existing.
- [x] **166.E.2** Documented in
      `book/src/getting-started/integration-nuttx.md` — primary
      path is `just nuttx setup`; manual symlink path retained for
      vendored apps trees.

---

## 166.F — dust-dds `Actor<DcpsStatusCondition>` poll deadlock on Xtensa LX7

> **Won't-Fix 2026-05-19 — superseded by Phase 169 (Retire dust-dds;
> consolidate on Cyclone DDS).** Closed by deleting the dust-dds
> submodule rather than patching the actor mailbox shape. Diagnostic
> infrastructure on `phase-117.0-esp32s3-toolchain` (the
> `nros-rmw-dds[debug-esp-println]` chain, the `block_on_boxed`
> fusion barriers, the `NROS_EXECUTOR_*` env-var arena shrinkage)
> is preserved on the feature branch for archaeology — none of it
> survives Phase 169.4 (delete `nros-rmw-dds`). The hypothesis
> below stands as a historical record of why dust-dds nostd-runtime
> was structurally untenable.

During Phase 117.2h (`phase-117.0-esp32s3-toolchain` branch) the
ESP32-S3 QEMU DDS talker / listener both reach `Executor::open`
cleanly (after 117.2g's stack-overflow workaround + 117.2f's
triple LLVM fusion barrier in `NrosPlatformRuntime::block_on_boxed`)
but hang inside the first `node.create_publisher` /
`executor.register_subscription` call. The first `block_on(create_topic)`
enters `NrosSpawner::drain_until_quiescent`, which calls
`drain_tasks` once, which calls `task.as_mut().poll(&mut cx)` for
each of the ~20 spawned tasks in FIFO order. Task index 16
(a `dust_dds::dcps::actor::Actor<DcpsStatusCondition>::spawn`
closure per the type-name spawn probe) never returns from `poll`.

**Status.** Not Started. Blocks Phase 117 close-out (ESP32-S3
QEMU DDS E2E). Workaround / probes already landed on the
feature branch; this issue tracks the underlying root cause.

**Priority.** P2 — blocks one slice, not the whole DDS surface.

**Depends on.** Phase 117.2g + 117.2f (both landed). The fusion
barriers are necessary preconditions to even REACH this hang
point — without them `block_on_boxed` returns Pending exactly
once and never re-polls, masking the actor deadlock.

### Symptom

With `nros-rmw-dds[debug-esp-println]` on, the talker trace ends:

```
[talker] post Executor::open
[talker] post create_node
Declaring publisher on /chatter (std_msgs/Int32) over DDS
[block_on] iter
[block_on] post-poll Pending
[drain] enter max_passes=256 queue_len=20
[drain_tasks] poll task 0 done: pending=true
...
[drain_tasks] poll task 15 done: pending=true
[drain_tasks] poll task 16          ← never returns
```

`drain_tasks` is sync — the for-loop hangs inside `poll(&mut cx)`
for task 16. CPU spins (no Pending, no Ready).

### Hypothesis

Looking at `packages/dds/dust-dds/dds/src/dcps/actor.rs`'s actor
loop + `dcps/channels/{mpsc,oneshot}.rs`:

- `MpscSender::send` is sync and uses `critical_section::with`
  to push + wake the receiver's waker (`mpsc.rs:77`).
- `MpscReceiverFuture::poll` also uses `critical_section::with`
  to pop or store a waker (`mpsc.rs:113`).
- `Actor<T>`'s mailbox handler runs message processing inline
  during `MpscReceiverFuture::poll` — IF that message handler
  itself does another `participant_address.send(...)`, we have
  two nested `critical_section::with` calls.

`critical-section`'s default `restore-state-bool` impl on esp-hal
xtensa is non-reentrant — nesting two `with` calls on the same
core toggles `PS.INTLEVEL` such that the inner `with` returns
with interrupts enabled, then the outer `with` restores its saved
state (interrupts disabled) at scope exit. Functionally OK for
mutex but not OK if either `with` body re-enters a `Mutex<RefCell<…>>`
the outer is holding — re-borrow panic, or in the dust-dds shape
likely a spinlock contention loop on a `Mutex<RefCell<MpscInner<T>>>`
the outer holds.

Why only `Actor<DcpsStatusCondition>` and only on CreateTopic:
the topic-creation path attaches a status condition to the new
topic, which spawns a status-condition actor that immediately
processes a setup message that itself sends to the participant
actor — exactly the nested-send shape above.

### Work items

- [ ] **166.F.1** Confirm the hypothesis by instrumenting
      `Actor::spawn`'s mailbox loop in
      `packages/dds/dust-dds/dds/src/dcps/actor.rs` with
      `dbg_log!` probes (gated behind the existing
      `nros-rmw-dds[debug-esp-println]` chain that's already
      wired through nros-rmw-dds → nros-rmw-cffi). Show the
      nested `critical_section::with` entry / exit pattern.
- [ ] **166.F.2** Pick a fix path:
      - **Option A — Patch dust-dds:** restructure the actor
        mailbox loop so message handlers complete fully BEFORE
        the next outbound send (no nested `with` on the same
        `MpscInner`). Vendored submodule — needs an upstream
        contribution or a maintained fork patch.
      - **Option B — Replace nostd actor mailbox:** swap
        dust-dds's per-actor mailbox shape on `nostd-runtime`
        for a single cooperative dispatch loop that
        `NrosSpawner::drain_until_quiescent` already pumps.
        Bigger change but avoids the nested-CS shape entirely.
      - **Option C — Replace `critical-section[default]`:**
        switch esp-hal's `critical_section_impl` to a reentrant
        variant (esp-hal v1.0 has `xtensa-lx-rt` reentrant
        support behind a feature; verify it composes with
        embassy-sync's `critical-section[default]`).
- [ ] **166.F.3** Once a fix lands, re-run
      `cargo nextest run -p nros-tests --test esp32s3_qemu_dds
      --run-ignored=all` from `phase-117.0-esp32s3-toolchain`;
      expect `Publisher declared` → `Published: 0` → `Received: 0`
      → ≥80% delivery (Phase 117.5 acceptance bar).

### Diagnostic infrastructure (already landed on feature branch)

These features are kept gated-off in production but available
on `phase-117.0-esp32s3-toolchain` for follow-up:

- `nros-rmw-dds[debug-esp-println]` — block_on iter + create_participant
  + write_message traces.
- `nros-rmw-cffi[debug-esp-println]` — CffiRmw + CffiSession +
  `open_trampoline` boundary traces.
- `nros-node[debug-uart-raw]` — raw UART0 MMIO writes for
  `Executor::open` / `from_session` bisection. No transitive
  deps — avoids the esp-sync / embassy-sync /
  critical-section[default] cross-talk that re-breaks the
  fusion barriers in `block_on_boxed`.
- `nros = { … features = ["debug-uart-raw"] }` umbrella forward.
- `NROS_EXECUTOR_ARENA_SIZE` / `_MAX_CBS` / `_MAX_SC` env vars
  in talker / listener `.cargo/config.toml` (workaround for
  117.2g stack overflow; revert once `Executor` heap-boxing
  lands).
