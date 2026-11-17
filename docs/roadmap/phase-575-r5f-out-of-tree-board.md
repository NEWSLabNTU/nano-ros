# Phase 575 — what an out-of-tree TI R5F board needs from nano-ros

**Status (2026-11-17). OPEN: W1–W7 are open and assigned; nothing is built yet.**
Owners per item, assigned by the manager on 2026-11-17. Filed from a downstream
project that runs nano-ros on the Cortex-R5F cores of a TI TDA4VH (J784S4)
EVM, under FreeRTOS, with lwIP over TI's Enet LLD.

## Why

The user ruled on 2026-11-17 that the TI board package is NOT built in
nano-ros. The downstream project builds it in its own repository, as an
out-of-tree board found through `NROS_EXTRA_BOARD_PATH`. nano-ros takes only
what is generic: missing features and bugs any board of this kind would hit.
nano-ros collects the board later only if it meets the support criteria
(registry evidence, the maintainer rule).

This phase is that generic half. Each item stands without the TI board: it
has its own test on a board or an emulator nano-ros already has, and the
TI board is its first user, not its only test.

What main had on 2026-11-17 (each checked):

- The `cortex-r5` arch profile in
  `packages/platform/nros-platform-freertos/nros-platform.toml` is softfp
  (`-mfloat-abi=softfp`, `target_exclude = "eabihf"`). It was made for the
  NVIDIA Orin SPE. There is no hard-float R5 profile and no R5 toolchain file
  in `cmake/toolchain/` (only `arm-freertos-armcm3` and `arm-freertos-armcr52`
  on Arm FreeRTOS).
- The only inter-core zenoh-pico link is `zpico-link-ivc` (NVIDIA IVC).
  Nothing speaks RPMsg or OpenAMP.
- FreeRTOS sched tasks carry `budget_us`, `deadline_us` and
  `deadline_policy` (`packages/boards/nros-board-freertos/c/freertos_run_sched_tasks.c`),
  and nothing enforces them. The sporadic server exists only on two
  bare-metal platforms (`nros-platform-stm32f4`, `nros-platform-mps2-an385`,
  `src/sporadic_timer.rs`).
- A vendor SDK the build needs and must never fetch has a pattern:
  `[gated.nxp-freertos-s32ze]` and `[gated.nxp-rtd-s32ze]` in
  `nros-sdk-index.toml`, used by `cmake/board/nano-ros-board-s32z270-freertos-rtd.cmake`.
- A board brings its own Ethernet through weak
  `nros_board_register_netif` / `nros_board_poll_netif`, which a vendor
  driver's strong definitions override.
- Every board builds with GCC. Nothing links nano-ros's objects into an image
  that another compiler (TI's `tiarmclang`) builds.

## Work items

### W1 — a hard-float Cortex-R5 arch profile and toolchain file

- **Status:** OPEN
- **Owner:** nros 118, after its current work

An `[arch.*]` profile for `armv7r-none-eabihf` with
`-mcpu=cortex-r5 -mfpu=vfpv3-d16 -mfloat-abi=hard`, beside the softfp
`cortex-r5` one, which stays as the SPE's. A `cmake/toolchain/` file for it,
on the R52 file's pattern (`NanoRosCrossToolchain.cmake`, the pinned
arm-none-eabi-gcc). The Rust side uses the `armv7r-none-eabihf` target.

Acceptance: a FreeRTOS C, C++ and Rust image builds for the new profile in
CI (build only; there is no R5F emulator), and the softfp SPE profile's
builds are unchanged.

### W2 — an out-of-tree board with a vendor SDK, end to end

- **Status:** OPEN
- **Owner:** nros 118, after its current work

Prove, with a fixture board kept outside `packages/boards/`, that a board
package in a user's repository works through `NROS_EXTRA_BOARD_PATH`:
`nros setup`, `nros build`, `nros run --dry-run`, and a vendor SDK it names
through its own variable and never fetches. Fix what that finds missing.
Document the result in the book's "Add a board" page. Whether a board
outside nano-ros can declare a `[gated.*]` SDK, or must name the SDK through
its own variable, is this item's question to settle.

Acceptance: the fixture board builds a FreeRTOS image from a released SDK
root with no nano-ros source patched, and the "Add a board" page's
out-of-tree section is a probe-fenced walk-through of it.

### W3 — linking nano-ros into an image another compiler builds

- **Status:** OPEN, and needed (settled 2026-11-18)
- **Owner:** nros 118, after its current work

nano-ros's Rust staticlib and C objects must link into an image that TI ARM
Clang (`tiarmclang`) builds (the same AAPCS).

The condition is settled. The downstream project uses TI's Processor SDK
RTOS for J784S4 (user answer, 2026-11-18). Its published release
(11_00_00_06) builds the R5F with TI ARM Clang only: GCC ships for the A72
alone, and its concerto build has no GCC R5F rule (nros 118's reading of
TI's documentation). TI's AM64x MCU+ SDK offers GCC for R5, but no J784S4
MCU+ SDK was found.

Acceptance: a minimal FreeRTOS image linked by a non-GCC toolchain in CI,
with nano-ros's objects built by its own toolchain.

### W4 — `zpico-link-rpmsg`, zenoh-pico over RPMsg

- **Status:** OPEN
- **Owner:** nros 241, paired with its RPMsg vision link

A zenoh-pico link over RPMsg between images on different cores of one SoC,
modelled on `zpico-link-ivc`. It is generic: RPMsg/OpenAMP runs on many
heterogeneous SoCs, and the link's platform half sits behind a small
interface the board fills. The alternative shape, an lwIP netif over RPMsg,
is weighed in the item's design note; pick by latency and footprint.

Settled with the downstream project (user answers, 2026-11-17):

- **The first user's R5F runs FreeRTOS from TI's Processor SDK RTOS for
  the J784S4**, with the SDK's IPC RPMessage API (user answers, 2026-11-17
  and 2026-11-18; first recorded as "MCU+ SDK", which TI does not publish for
  the J784S4). The port assumptions and the backend binding follow that.
- **R5F to R5F first.** The downstream design routes RPMsg only between R5F
  images on different cores. No Linux end is in its path: the A72 cluster
  has no role, and the external host is reached over Ethernet. A Linux end
  (TI exposes `rpmsg_char`, not a tty, so a small generic
  `rpmsg_char`-to-TCP bridge into the router, with a tty plus the router's
  serial locator where one exists) is generic nano-ros work that this
  user does not need first.
- **The split.** nano-ros owns the link glue, the framing (a fixed 496 B
  MTU), the RX ring, the wake and the backend contract. The vendor
  RPMessage binding stays in the board package, which lives in the
  downstream repository.

Acceptance: a talker and listener on two images exchange messages over the
link on an emulated or simulated pair, with p50/p99 hop latency recorded.

### W5 — budget enforcement and deadline monitors on FreeRTOS

- **Status:** OPEN
- **Owner:** nros 142, after its RTIC-priorities work item

Enforce the `budget_us` a FreeRTOS sched task already carries: a sporadic
server, on the pattern of the bare-metal `sporadic_timer.rs`, with the
overrun policy the contract names. Detect a missed `deadline_us` and run the
declared `deadline_policy`.

Acceptance: on the FreeRTOS QEMU board, an injected overrun is throttled at
its budget and logged, a deadline miss runs its policy, and the other sched
tasks keep their deadlines.

### W6 — a time source hook for timestamps

- **Status:** OPEN
- **Owner:** nros 139, with issue 0758 (SNTP)

A board can supply the clock that node timestamps read (a gPTP-disciplined
clock on a TSN board), instead of the platform's monotonic tick. Generic
across boards with a hardware PTP clock.

Acceptance: a board-supplied clock is what message timestamps carry, in a
test with a stub clock; boards that supply none are unchanged.

### W7 — the book says what is generic and what is the board's

- **Status:** OPEN
- **Owner:** nros 139 AUO book, after W1, W2 and W5 land

When W1, W2 and W5 land: the book's "Add a board" page names the boundary
this phase draws (what a board package in the user's repository owns, and
what nano-ros provides), and links the out-of-tree walk-through.

Acceptance: the page states the split, and every step it shows is probed.
