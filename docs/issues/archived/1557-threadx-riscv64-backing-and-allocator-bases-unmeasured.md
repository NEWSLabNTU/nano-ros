---
id: 1557
title: "What issue 1145 left behind: `threadx-riscv64` states no executor-backing
  size, so its image still reserves the backing twice — and the allocator BASES
  the pairings subtract from (ThreadX's 4 MiB byte pool, FreeRTOS's 3 MiB
  cyclone/XRCE heap) were never derived"
status: resolved
resolved: 2026-10-01
resolved_in: fix/1557-rv64-delivery
type: tech-debt
area: boards, threadx, freertos
severity: low
found: 2026-09-29
related: [1145, phase-392, phase-448, 1171, 1355, 1197, 1388, 1590]
---

## Resolution (2026-10-01)

**Item 1 closed: the rv64 backing is stated (phase-472 F5, below) and the images
now DELIVER.** Why they did not: the six rust leaves build through
`nros_threadx_rv64_rust_app` (since phase-369 W2, for zenoh too), whose app
thread ran on the board crate's `Config::default()` — static `192.0.3.10` and a
router at `tcp/192.0.3.1:7447`, the old threadx-linux TAP-bridge plan. Every
other part of this board already used QEMU slirp's DEFAULT plan: the harness
launcher (`start_riscv64_virt`, plain `-netdev user`), both `NROS_APP_CONFIG`
emitters that NetX is brought up from (`10.0.2.40/.41`, gw `10.0.2.2`), every
C/C++ fixture row (`NROS_ENTRY_LOCATOR = tcp/10.0.2.2:95xx/96xx`), the rust
leaves' own `system.toml` (`tcp/10.0.2.2:94x0`), and the entry lane's default
(`NANO_ROS_LOCATOR_ENTRY_DEFAULT`). Slirp-default is also what the majority of
QEMU boards with this harness use (NuttX arm/riscv, mps2-an385 bare-metal, the
FreeRTOS Rust entries); the `192.0.3.0/24` + `host=192.0.3.1` plan is the
FreeRTOS C/C++ and ESP32 minority. So the rust app thread was the one outlier,
and it now follows the board's convention:

- `startup.c` exports `nros_rv64_image_identity()`: the `NROS_APP_CONFIG`
  network bytes NetX was configured from, plus a per-image locator define.
- `nros_threadx_rv64_rust_app` resolves that locator — `-DNROS_ENTRY_LOCATOR`,
  else the leaf's `system.toml` `[image.*] locator` (through
  `nano_ros_read_leaf_system`), else `_nros_resolve_entry_locator` — and bakes it
  into `startup.c`, NOT into cargo env (the Corrosion cargo dir is shared across
  leaves, issue 0805, so a per-port env would race on one board rlib).
- `run_app_thread` reads it (`image_config()`); `Config::default()` / the C
  `cfg_*` defaults move to the slirp plan too.

Delivery, hand run with `start_riscv64_virt`'s exact QEMU arguments against the
paired `rmw_zenohd` (`nros_router_exec`):

- talker + listener on `tcp/10.0.2.2:9400`: listener banner
  `IP 10.0.2.41`, `nros entry ready`, **39 `I heard: [Hello World: N]`** in 40 s;
- action server + client on `:9420`: `Goal accepted`, 10 feedbacks,
  `Result received: [0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55]`.

**Item 2 closed: both bases are documented beside their definitions.**

- **ThreadX `BYTE_POOL_BASE_SIZE` (4 MiB)** — loaded peaks (session up, traffic
  flowing), rv-virt-threadx zenoh: action-server 725,088 B, action-client
  721,128, talker 706,136, listener 705,384, of a 4,105,752 B pool: 5.7x margin.
  KEPT, not derived down: every row is zenoh, the Cyclone rust images do not
  link (issue 1590) and the C/C++ images do not run the reporter, so a cut would
  be a guess about Cyclone in the unsafe direction. The table and that reason
  are in `threadx_hooks.c`. threadx-linux's 181,144 B (1145) is cited, not
  re-measured.
- **FreeRTOS `NROS_FREERTOS_HEAP_KB 3072`** — NOT measured, and the define now
  says why: only a build of `nros-board-freertos` without `rmw-zenoh` reaches it,
  and the one in-tree instance is the S32Z270 Cyclone C++ workspace entry, a
  link-only witness (no emulator). The mps2-an385 Rust Cyclone fixture is retired
  (`#[ignore]`) and no FreeRTOS XRCE image exists. The comment names the
  `nros: heap peak` line to record when one boots.

## Background

[Issue 1145](1145-executor-backing-static-unpaired-with-rtos-heap.md)
asked every RTOS port to stop reserving the executor backing twice after
phase-392 W6 moved it into the `.bss` static
`nros_node::executor::backing::EXECUTOR_BACKING`. Every port it covered is
resolved (Zephyr, NuttX, FreeRTOS, ESP32, `threadx-linux`), and 1145 is closed.
This issue is the two items it could not close, split out so a resolved issue
does not carry an open list.

Neither item is a defect in a running image. Both are about bytes reserved on
purpose-built headroom nobody measured.

## 1. `threadx-riscv64` states no backing size

1145's ThreadX section, verbatim:

> The mechanism is board-agnostic and the rv64 descriptor can state its own rung
> the moment someone measures it. It is NOT stated here because the number has to
> come from that board's own images (`nm -S` on a riscv64 build), and building the
> rv64 lane was not affordable in this session. Unstated is the safe state: no
> define, the C `#ifndef` default of 0 applies, and the pool stays at its base —
> the image pays twice, exactly as it does today.

Where it stands at 2026-09-29:

- `packages/boards/nros-board-threadx-linux/nros-board.toml:105-106` states
  `[board.knobs.executor] backing_u64s = 11069` (raised from 1145's `4494` by
  issue 1388, because the claim must cover the largest default `nros-node`
  compiles on that board, including the synthesised `nros_ws_runtime` umbrella).
- `packages/boards/nros-board-threadx-qemu-riscv64/nros-board.toml` has **no**
  `[board.knobs.executor]` table. So `threadx_hooks.c:107-109` applies its
  `#ifndef NROS_EXECUTOR_BACKING_U64S` / `#define … 0` default, and the pool is
  the whole `BYTE_POOL_BASE_SIZE` beside a full-size `EXECUTOR_BACKING`.
- `scripts/check-executor-backing-arena-pairing.py`'s `PORTS` ledger records
  `threadx-riscv64` as `{"kind": "rung", "site": "threadx"}`: the mechanism is
  wired and gated, only the statement is missing.

**Cost:** one double reservation of the backing (tens of KiB) on a QEMU-only
board. **Blocker:** [issue 1355](../1355-threadx-riscv64-builds-the-linux-threadx-port.md)
— the `threadx_riscv64` lane builds ThreadX's linux/gnu port and its board crate
does not compile, so there is no image to measure yet.

## 2. The allocator bases are undeclared numbers

Every pairing is `base - 8 * backing_u64s`. The subtrahend is stated once and
gated; the base is not derived from anything.

- **ThreadX byte pool, 4 MiB** —
  `packages/boards/nros-board-common/c/threadx_hooks.c:105`:
  `#define BYTE_POOL_BASE_SIZE     (4 * 1024 * 1024)`. The comment above it
  (lines 93-104) already says so: "the 4 MiB it is subtracted FROM was chosen by
  nobody who wrote down why", the number dates from phase 152's "4 MB byte pool",
  and "The RISC-V board is where the base has to be judged, and it has not been."
  The one measurement: a hosted `threadx-linux` talker touches **181,144** bytes
  of the pool (`nros: byte pool peak`, reporter in
  `packages/boards/nros-board-threadx/src/entry.rs:322`) — but that board's NetX
  init is a no-op overlay, so the packet pool and IP/ARP/BSD stacks that are the
  pool's other consumers allocate nothing there.
- **FreeRTOS heap** — 1145 recorded this as "FreeRTOS's 2 MiB". That literal is
  GONE for the zenoh lane: issue 1197 / phase-448 W4 replaced it with
  `default_heap_bytes(app_stack_bytes)`
  (`packages/boards/nros-board-common/src/freertos_config.rs:267`, selected in
  `packages/boards/nros-board-freertos/build.rs:177-178` when `rmw-zenoh` is on),
  every term of which is measured. What remains undeclared is the
  **cyclone/XRCE** lane, which does not enable that feature and keeps
  `packages/boards/nros-board-freertos/config/FreeRTOSConfig.h:63-65`:
  `#define NROS_FREERTOS_HEAP_KB 3072` — justified in the comment above it only as
  "sized for that heavy boot path (within the 4 MiB MPS2-AN385 SRAM budget)".
  `build.rs`'s own comment: "they keep `FreeRTOSConfig.h`'s 3 MiB, which this PR
  did not measure."

(`nros-board-freertos-posix`'s `configTOTAL_HEAP_SIZE` of 4 MiB is NOT in scope:
heap_3 wraps the host `malloc`, so it is not a budget the port enforces — its own
`FreeRTOSConfig.h:53-55` says so.)

## What closing looks like

1. **riscv64 backing** (after 1355): build the `threadx-riscv64` Rust leaves,
   `just mem-report <elf> --json` each, take the largest `EXECUTOR_BACKING` any
   compiled unit needs (issue 1388's rule, not the heaviest role's), and state
   `[board.knobs.executor] backing_u64s` in the rv64 descriptor. Confirm
   `backing + byte_pool_storage == BYTE_POOL_BASE_SIZE` via `nm -S`, that
   `just check executor-backing-arena-pairing` and `just check node-std-tests`
   accept the claim, and that the images **boot and deliver** under QEMU — not
   merely link.
2. **Bases**: derive each base from measured terms, as `default_heap_bytes` did
   for FreeRTOS/zenoh — the rv64 image's `nros: byte pool peak` for ThreadX (the
   board where NetX actually allocates), and the cyclone/XRCE images' `nros: heap
   peak` for FreeRTOS — or, where a derivation is not worth it, DOCUMENT the base
   beside its definition with the measured peak and the margin it carries. Either
   way the number stops being one "nobody wrote down why".

## Progress (2026-09-30, phase-472 F5) — item 1 stated and measured; boot reached, delivery not; item 2 open

With issue 1355's current blocker (a missing `nros/app_config.h` in the rust
leaves) fixed on main by `9d0393bfd5`, the board's own image could be measured:

- `riscv-none-elf-nm -S` on `rv-virt-threadx/rust/listener` (zenoh, cmake leaf):
  `nros_node::executor::backing::EXECUTOR_BACKING` = `0x159e8` = **88,552 B =
  11,069 words** — the unnarrowed 64-bit default, the same number threadx-linux
  states (issue 1388's rule: cover the largest default any unit compiles).
- Stated: `[board.knobs.executor] backing_u64s = 11069` in
  `nros-board-threadx-qemu-riscv64/nros-board.toml`, and the pairing gate's
  `BOARD_TOML_TARGETS` knows the board (64-bit, `riscv64gc-unknown-none-elf`).
- Rebuilt and re-measured: `byte_pool_storage` = `0x3ea618` = 4,105,752 B, and
  **4,105,752 + 88,552 = 4,194,304 = `BYTE_POOL_BASE_SIZE`** — the backing is now
  reserved once. `just mem-report` on the image: 4,630,144 B RAM, of which the
  pool is 88.7 % and the backing 1.9 %.
- `check-executor-backing-arena-pairing` (incl. `--claims`, which now emits the
  rv64 claim) and `just check node-std-tests` (the claim compared against the
  measured default, cross-checked for the target) pass.
- **Boot:** under QEMU the image reaches `c_app_main` and prints
  `nros: byte pool peak 598952 of 4105752 bytes (3506800 free)`.
  **Deliver: NOT verified** — no zenoh session reached a host router in 60 s
  under either slirp plan (see issue 1355's 2026-09-30 progress entry: the
  board's static `192.0.3.10` plan versus the launcher's).

Item 2 (the bases) is NOT closed: the one rv64 byte-pool peak above was taken
from an image that did not get its session up, so it is not the loaded peak
this item asks for, and the FreeRTOS cyclone/XRCE 3 MiB base was not measured.


## Resolution (2026-10-01)

**Item 1 — closed.** The rv64 statement (`backing_u64s = 11069`) landed with
the progress entry above; re-measured here on a fresh build of all six Rust
roles: every image carries `EXECUTOR_BACKING` = `0x159e8` = 88,552 B and
`byte_pool_storage` = `0x3ea618` = 4,105,752 B, which sum to
`BYTE_POOL_BASE_SIZE` exactly — the backing is reserved once.
`check-executor-backing-arena-pairing` (incl. `--claims`, which emits
`packages/boards/nros-board-threadx-qemu-riscv64/nros-board.toml 11069 threadx
64 riscv64gc-unknown-none-elf`) covers the board. The missing half was
"boot AND DELIVER": the images now deliver under QEMU — talker→listener 52 and
49 samples, `add_two_ints` = 5, Fibonacci order 10 result `[0, 1, …, 55]` —
but only with the zenoh locator changed in a measurement-only build (not
committed). Why the shipped image cannot reach a router is a separate defect,
filed as [issue 1619](../1619-threadx-rv64-rust-cmake-path-dials-board-default-locator.md)
(NetX on `10.0.2.40` from `NROS_APP_CONFIG`, zenoh dialling `192.0.3.1:7447`
from `Config::default()`, captured with a QEMU `filter-dump`). It is what issue
1355 recorded as "no session in 60 s".

**Item 2, ThreadX — closed by DOCUMENTING the base** (the issue's second
allowed outcome). Measured `nros: byte pool peak` on QEMU riscv64, pool
4,105,752 B:

| role | peak (B) |
| --- | --- |
| boot, no session | 598,952 |
| talker (session, publishing) | 705,792 / 706,136 |
| listener (49 samples received) | 705,384 |
| service server / client (one exchange) | 706,576 / 708,216 |
| action server / client (goal + feedback + result) | 725,656 / 720,904 |

Worst: 725,656 B = 17.7 % of the pool; ~5.6x margin. Written beside
`BYTE_POOL_BASE_SIZE` in `packages/boards/nros-board-common/c/threadx_hooks.c`.
The base was NOT lowered: Cyclone does not link on this board yet (issue 1590),
and a DDS participant is what the old number was sized for — lowering it is a
policy change this measurement enables, not one it makes.

**Item 2, FreeRTOS — split to [issue 1624](../1624-freertos-cyclone-heap-base-unmeasurable-in-qemu.md).**
The 3 MiB non-zenoh default reaches exactly one in-tree image,
`workspace-cpp-s32z270-freertos`, on hardware with no QEMU model (mps2-an385
has no xrce/cyclone board feature and no such fixture row; mps3-an536 states
its own 32 MiB). Documented beside the `#define`; the pairing gate's note now
points at 1624.

NOT measured: the cyclonedds rv64 roles (they do not link, issue 1590); C and
C++ rv64 images (their executors are file-scope statics and never drew on the
pool); peaks under sustained high-rate traffic (each run was the example's own
cadence for ~40-90 s).

Measurement commands: `riscv-none-elf-nm -S -C <elf> | grep -E
'EXECUTOR_BACKING|byte_pool_storage'`; QEMU `-netdev user` with
`rmw_zenohd` on host `127.0.0.1:7553`, reading each image's `nros: byte pool
peak` line.
