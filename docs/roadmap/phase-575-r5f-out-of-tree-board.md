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

- **Status:** OPEN (the profile, toolchain file and ABI check landed
  2026-11-18; the image acceptance moved to W2)
- **Owner:** nros 118

An `[arch.*]` profile for `armv7r-none-eabihf` with
`-mcpu=cortex-r5 -mfpu=vfpv3-d16 -mfloat-abi=hard`, beside the softfp
`cortex-r5` one, which stays as the SPE's. A `cmake/toolchain/` file for it,
on the R52 file's pattern (`NanoRosCrossToolchain.cmake`, the pinned
arm-none-eabi-gcc). The Rust side uses the `armv7r-none-eabihf` target.

Acceptance: a FreeRTOS C, C++ and Rust image builds for the new profile in
CI (build only; there is no R5F emulator), and the softfp SPE profile's
builds are unchanged.

**Split, ruled 2026-11-18.** A FreeRTOS image needs a board, and the real
board stays out of tree, so the image half of this acceptance moved to W2,
whose fixture board builds those images. W2 checks `Tag_ABI_VFP_args: VFP
registers` on each. What W1 landed:

- `[arch.cortex-r5f]` in `nros-platform-freertos/nros-platform.toml`,
  claiming `armv7r-none-eabihf` only, beside the softfp `[arch.cortex-r5]`,
  which still claims `armv7r-none-eabi` only (`check-arch-profile-resolution`:
  no triple has two answers, and each ABI agrees with its triple);
- `cmake/toolchain/arm-freertos-armcr5f.cmake`, on the R52 file's pattern;
- `nros_arm_r_fpu_cflags_env` (was `nros_armv8r_cflags_env`), which now also
  hands cargo's cc-rs builds the R5F's FPU flags;
- `check-r5f-hard-float-abi` (build tier): C, C++ and Rust objects built for
  the profile all carry `Tag_ABI_VFP_args: VFP registers` and link together,
  and its selftest requires ld to refuse a softfp C half against them.
  Making the toolchain file `softfp` turns it red (measured).

W1 closes when W2's images build.

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

Built as RFC-0135's first board package (phase-577 W8). Found on the way:
an outside FreeRTOS board must copy the family's three config headers, because
the cargo road's C builds see only `FREERTOS_CONFIG_DIR` (issue 2687, not a
precondition).

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

Acceptance (scope settled 2026-11-18, nros 241 manager, on nros 118's
measurements below): a minimal FreeRTOS image linked by a non-GCC toolchain,
checked by a local opt-in check (`just check tiarmclang-image`), and in CI
only if a TI board joins nano-ros's tests (see Provisioning). nano-ros's C (zenoh-pico, lwIP, the platform and board glue) is built
by the IMAGE's C compiler, and only the Rust staticlib by nano-ros's own
toolchain (rustc). In that image a board's strong definition of a weak board
hook overrides the weak default nano-ros's objects carry, and with no strong
definition the weak default links. One check covers every weak board hook:
`nros_board_wall_clock_ns` (W6's clock seam), `nros_board_register_netif` and
`nros_board_network_wait` (asked by nros 139 for W6, 2026-11-18).

Why the image's compiler builds nano-ros's C. Measured with TI Arm Clang
5.1.1.LTS, relinking the GCC-built `fixture-r5f-freertos` C image with
tiarmlnk:
- nano-ros's GCC objects, compiled against newlib's headers, named newlib's
  ABI: `__errno`, `_impure_ptr`, `_ctype_`, `__assert_func`. TI's C library
  has another (`errno`, `_ftable`, `_assert`), and an image links one libc.
- GCC against TI's headers is no way out: they assume clang (`stdlib.h` pulls
  `alloca.h`, which is `#error` for any other compiler), and TI's libc has no
  `<sys/time.h>` or `<unistd.h>`.
- Shimming newlib's symbols cannot reconcile stdio: `_impure_ptr` is newlib's
  FILE layout.

Weak hooks under tiarmlnk, measured: the same as GNU ld in all four cases.
A weak default plus a strong definition in an archive links the default (no
linker pulls a member to replace a weak definition). With no weak default the
archived strong definition is pulled. With neither the link fails naming the
hook. A strong definition in an object replaces the default. So a board's
strong hook goes in an object on the link line, or the build omits the weak
default (W6's `source = "board"`).

Delivered: `cmake/toolchain/arm-freertos-tiarmclang-cr5f.cmake`, the fixture
package's second board `fixture-r5f-tiarmclang` (a TI linker command file
beside the GNU script), and `just check tiarmclang-image`. nano-ros's C no
longer assumes newlib: `__dso_handle` is weak, the FreeRTOS family's lwIP
`cc.h` takes `struct timeval` from `<sys/select.h>` and fills in the socket
errnos a libc lacks, newlib's time shims sit under `__NEWLIB__`, and
zenoh-pico's FreeRTOS wall clock is `nros_platform_time_now_ns`
(`Z_FEATURE_NROS_PLATFORM_TIME`, a fork patch). Found on the way:
issue 2692 (some cc-rs builds take their CPU from cc-rs's compiler defaults, not
the arch profile) and issue 2693 (the stack-floor check is off on this road).

Provisioning (settled 2026-11-18): TI Arm Clang is a `[gated.ti-arm-clang]`
row, never fetched by `nros setup`, because its licence (TI's "TI Text File"
terms, plus a click-through in the installer) limits use to TI devices and is
the user's to accept. No public image or mirror may carry it.

CI scope (user ruling, 2026-11-18): the CI runners install TI Arm Clang only
when a TI board is included in nano-ros and is part of its tests. A TI board
kept on the user side is the user's responsibility. The one TI board in-tree
today is `fixture-r5f-tiarmclang`, a fixture standing in for a user's
out-of-tree board, so no runner carries the toolchain: the runner tooling
(PR #4202) was closed, and `just check tiarmclang-image` runs where a
developer has installed TI Arm Clang (`TI_ARM_CLANG_ROOT`).

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

**No router between two R5F images (decided 2026-11-18,
[RFC-0134](../design/0134-p2p-link-peering.md)).** nano-ros compiles
zenoh-pico's unicast peer mode out, and zenoh-pico 1.10.1's peer mode is
socket-based (its accept path is TCP/TLS only and its peer read loop waits on
sockets), so two images cannot peer over RPMsg as shipped. The user approved:

- unicast peer mode as an opt-in, per image, only for images that carry the
  RPMsg (or IVC) link; the default build stays peer-free, with a gate that a
  non-RPMsg image compiles `Z_FEATURE_UNICAST_PEER 0`;
- a patch to our zenoh-pico fork for peering on a single-peer symmetric link,
  committed on the fork's patch-line branch (rebased, linear, not squashed);
  the user pushes the fork, then the superproject pin moves;
- the cost measured before the link is built.

Steps, in order:

0. **Design and cost.** RFC-0134 with its RFC-0109 and RFC-0110 amendments,
   plus the measurement: unicast peer over TCP on FreeRTOS MPS2-AN385 under
   QEMU against the same image built peer-free, giving static RAM, heap at
   open, live executor tasks against `Z_RUNTIME_MAX_TASKS`, executor stack
   peak, and threads added (expected none). The numbers go to the user
   before step 1.
1. **The fork patch, then the link.** The RFC-0134 D2 patch, then
   `zpico-link-rpmsg` on the `zpico-link-ivc` pattern, with the vendor
   binding in the board package.
   - **Patch: done (2026-11-18).** Three fork commits on `nano-ros-1.10.1`:
     the D2 peering patch, and two IVC fixes the first end-to-end run found
     (a client on IVC dereferenced NULL for its peer socket; a session on
     IVC could neither wait in its handshake nor read). The nano-ros half is
     the opt-in and the guard. The opt-in is by link set (RFC-0134 D1,
     amended; ruled by the user on 2026-11-18): every image that compiles a
     point-to-point link compiles peer mode, +5,188 B of code, no RAM or
     heap, with no per-image knob. The user chose to push
     once a runtime test existed: `nros-rmw-zenoh`'s `point_to_point_peer`
     runs two processes over the IVC host mock, with no router.
   - **Link: done on the host (2026-11-18).** `zpico-link-rpmsg` owns the
     endpoint table, a receive ring per endpoint and the board contract
     (`include/nros/rpmsg_link.h`: the board registers `open`/`close`/`send`
     and calls `nros_rpmsg_deliver` from its receive callback or task). The
     fork's RPMsg link shares one framing with IVC (`framed.c`), 496-byte
     frames. `tests/point_to_point_peer.rs` runs two peers over each link's
     host mock with no router, with a 1000-byte message that spans several
     frames. The TI RPMessage binding is the board package's, downstream.
2. **Acceptance on dual-core QEMU.** Two nano-ros peers, one listening and
   one connecting, exchange messages over RPMsg with no router, on an
   emulated dual-core target (Zephyr's OpenAMP samples on a dual-core QEMU
   machine), with p50/p99 hop latency recorded. The same over IVC if that
   comes cheap, since it is the same seam.

The Linux end (the `rpmsg_char` bridge, or the router's serial locator on an
RPMsg tty) is generic follow-up for the vision island, not this item's
acceptance. A native zenoh-rs RPMsg link, which removes the bridge, is
phase-576.

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

Also in W6 (manager, 2026-11-18): retire the FreeRTOS libc shims that answer
wall-clock calls with ticks since the scheduler started (`_gettimeofday` in
`nros-board-freertos`, `clock_gettime(CLOCK_REALTIME)` in
`nros-platform-freertos`'s Cyclone compat), or route them through the selected
source. Without this, the seam would not be the only answer to "what time is
it". The detail is in issue 2691, including why `_gettimeofday` also feeds
libstdc++'s `steady_clock` there.

### W7 — the book says what is generic and what is the board's

- **Status:** OPEN
- **Owner:** nros 139 AUO book, after W1, W2 and W5 land

When W1, W2 and W5 land: the book's "Add a board" page names the boundary
this phase draws (what a board package in the user's repository owns, and
what nano-ros provides), and links the out-of-tree walk-through.

Acceptance: the page states the split, and every step it shows is probed.
