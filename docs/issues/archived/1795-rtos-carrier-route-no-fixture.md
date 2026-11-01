---
id: 1795
title: "No lane builds an RTOS TYPED carrier image (`nano_ros_node_register(... TYPED DEPLOY threadx|nuttx)`): every in-tree ThreadX/NuttX C/C++ image takes `nano_ros_entry()`, so the carrier route's link has no coverage"
status: resolved
type: tech-debt
area: [build, cmake, testing, threadx, nuttx]
severity: low
found: 2026-10-11
related: [issue-1753, issue-1742, rfc-0043, phase-505]
resolved_in: "PR #1938"
---

**Home: [phase-505](../../roadmap/archived/phase-505-ci-lanes-reach-and-infra-robustness.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

## What was measured

`cmake/NanoRosNodeRegister.cmake` has an RTOS carrier arm for ThreadX
(`_NRC_DEPLOY` with `NROS_PLATFORM threadx`) and one for NuttX. Each
synthesises a single-node C++ entry TU and links the image. No row in
`examples/fixtures.toml` reaches either arm: every ThreadX and NuttX C/C++
example and workspace builds through `nano_ros_entry()`.

That route failed silently twice:

- **Issue 1742.** The carrier applied no panic policy, so the `nros-c`
  staticlib stopped with `#[panic_handler] function required`.
- **Issue 1753.** Once 1742 was fixed, the rv-virt-threadx carrier failed at
  the final link. Its generated entry holds `static ::rclcpp::Node`, which
  needs the C++ static-object ABI. That ABI was missing on the board.

Each was found only by building a carrier image by hand, while working on
something else.

## Why it matters

The two routes differ in exactly the way that broke. The carrier's TU is C++
with a static object that has a destructor, while an entry-route C image has
none. So the entry route's coverage says nothing about the carrier's link.
Issue 1753's gate (`check-crt-free-runtime-symbols`) now covers that class
statically, but no gate covers the route itself. The next divergence between
the two routes will not be caught either.

## Direction

Add one build fixture per RTOS carrier arm: the rv-virt-threadx carrier
(the `pkg_c_talker` shape issue 1753 used, with `DEPLOY threadx`), and a
NuttX one. Each row also needs the matching `matrix::CELLS` entry, or a
compile-check row if a link is the whole assertion. That is the minimum. A
boot cell is better, since issue 1753's image boots and publishes.

**Acceptance:** each carrier arm is built by a lane. Then removing
`cxx_static_abi.c` from `nano-ros-board-rv-virt-threadx.cmake` fails that
lane's build, independently of the static gate.

## Resolution (2026-10-11)

### The first lane build found a third defect: no lane-built carrier ever fired

The new fixture, `examples/rv-virt-threadx/c/typed-carrier`, failed inside
`just threadx_riscv64 build-fixtures`. The failure was not at the link 1753
fixed. It was earlier: `nros-c` stopped with
`error: #[panic_handler] function required, but not found`, and the leaf
produced no image at all.

The cause is the platform spelling. The recipes configure every rv-virt-threadx
leaf with `-DNROS_PLATFORM=threadx_riscv64`, and the toolchain file's
usage line documents the same. The three RTOS carrier arms in
`NanoRosNodeRegister.cmake` compared the raw value with the bare family token
(`STREQUAL "threadx"` / `"nuttx"` / `"freertos"`). So on any lane-built leaf
the ThreadX carrier never ran: configure succeeded, no executable was
declared, no image applied a panic policy, and the default build compiled a
`nros-c` staticlib with no `#[panic_handler]`.

The hand reproductions in 1742 and 1753 passed `-DNROS_PLATFORM=threadx`,
the one spelling that worked, so they never met this.

`nano_ros_entry()` already normalised the variant spellings, with its own
inline regex. The carriers were the second road and had none. Same class as
issue 0946's two locator ladders.

### Fixed

- **`cmake/NanoRosPlatformFamily.cmake`: `nros_platform_family()`** is the one
  variant-to-family map (`threadx_riscv64`/`threadx_linux` → `threadx`,
  `freertos_armcm3` → `freertos`, `nuttx_armv7a` → `nuttx`).
  `nano_ros_entry()` calls it in place of its inline copy. Its FreeRTOS
  app-config branch, which compared the raw `"freertos"`, now compares the
  family. All three carrier arms dispatch on the family, and they pass the
  family, not the raw value, to `_nros_resolve_entry_locator`. The raw value
  had no rung there, so configure stopped with "no rung for platform
  'threadx_riscv64'".
- **`_nros_resolve_entry_locator(node-register …)`** now honours
  `NROS_ENTRY_LOCATOR` first. That is the name every cmake fixture row, the
  entry road and `system.toml` use. The carrier read only
  `NROS_{THREADX,NUTTX,FREERTOS}_LOCATOR`, so a `-DNROS_ENTRY_LOCATOR` on a
  carrier leaf was silently ignored and the image dialled the default port.
  The per-RTOS names stay as the fallback rung, and nothing in the tree sets
  them.
- **Coverage.** The leaf has a `[[fixture]]` row
  (`NROS_ENTRY_LOCATOR = tcp/10.0.2.2:9581`). A new matrix workload,
  `Workload::TypedCarrier` (port offset 81, giving
  `alloc::port_of(ThreadxRiscv64, C, TypedCarrier)` = 9581), has one
  `ThreadxRiscv64/C/Zenoh/Example/Runtime` cell. The checker's evidence is
  `output::TYPED_CARRIER_SENT_PREFIX`. In
  `tests/threadx_riscv64_qemu.rs`, `test_threadx_riscv64_typed_carrier_boots_and_publishes`
  boots the image under QEMU slirp against `rmw_zenohd` on the cell's port and
  waits for `sent: 0..2`. A tripwire test,
  `typed_carrier_cases_cover_every_matrix_cell`, fails if the cell set and the
  test disagree.
- **Gate `check-road-dispatch-platform-family`** (fast line):
  - **R1:** neither dispatcher compares `NROS_PLATFORM` with a bare
    family token.
  - **R2:** no file under `cmake/` re-spells the normaliser's regex.
  - Mutations: putting the raw `"threadx"` compare back gives rc=1; putting
    the inline normaliser back into `NanoRosEntry.cmake` gives rc=1; the clean
    tree gives rc=0.

### Measured

- `just threadx_riscv64 build-fixtures`: every row builds, including the
  carrier (`c_typed_carrier`).
- `just threadx_riscv64 test-all`: 6/6 `threadx_riscv64_qemu`, including both
  new tests (the carrier publishes 3 messages in ~5 s), and 6/6 `rtos_e2e`
  ThreadxRiscv64.
- **Acceptance.** With `cxx_static_abi.c` removed from
  `nano-ros-board-rv-virt-threadx.cmake`, `just threadx_riscv64
  build-fixtures` fails (rc=2, `FAILED: c_typed_carrier`, `rust-lld:` on
  libc's `__call_atexit.o`). The lane now catches 1753's defect on its own,
  without the static gate.

### NuttX

The same family fix makes the NuttX carrier fire on lane builds
(`NROS_PLATFORM=nuttx_armv7a`). Built by hand first, it linked, booted,
and published `[typed_carrier] sent: 0..7` through `rmw_zenohd` under QEMU
(arm virt, MMIO virtio-net, the harness's `start_nuttx_virt` shape).

It now has its own leaf, `examples/qemu-armv7a-nuttx/c/typed-carrier`, with a
`[[fixture]]` row (`alloc::port_of(NuttxArm, C, TypedCarrier)` = 8381) and a
`NuttxArm/C/Zenoh/TypedCarrier/Example/Runtime` cell.

Both platforms' cases live in one file, `tests/typed_carrier_e2e.rs`: a shared
boot-and-publish body, one test per platform, each lane-scoped with
`require_platform_in_lane`, and a tripwire that holds the TypedCarrier cells to
exactly the covered platforms. That file has a `.config/nextest.toml` override
for a cold QEMU boot (120 s, retries 1, the same shape as `param_store_carve`).

Verifying it found issue 1801, which this branch also fixes. A NuttX
WORKSPACE entry built with an empty `NUTTX_DIR` because the board module
locates the tree inside the nano-ros subdirectory scope. It predates this
change and is on main.

### Measured (both platforms)

- `just nuttx build-fixtures-arm`: every row builds, including
  `workspaces/c` (issue 1801) and the NuttX carrier.
- `just threadx_riscv64 build-fixtures`: every row builds.
- `cargo nextest run --test typed_carrier_e2e`: 3/3 (ThreadX boot, NuttX boot,
  tripwire).
- NuttX `rtos_e2e`: 9/9 (no regression from the family dispatch).

## Closed (2026-10-18)

The resolution above landed in PR #1938 (with issue 1801), and its acceptance
was met there. Both carrier arms have a `[[fixture]]` row:

- `examples/rv-virt-threadx/c/typed-carrier`
- `examples/qemu-armv7a-nuttx/c/typed-carrier`

Both rows are built by their platform lane, each has a `matrix::CELLS` cell,
and removing `cxx_static_abi.c` fails the ThreadX lane's build. The status was
not flipped when it merged. The scheduled matrix lanes giving no verdict
(they currently stop in the build) is phase-466's subject, not a gap in this
issue's coverage.
