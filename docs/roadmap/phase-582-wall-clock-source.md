# Phase 582 — Wall-clock source

**Status (2026-11-19). OPEN: design in review; no item started. Owner: nros
139.** Implements [RFC-0141](../design/0141-wall-clock-source.md): one selected
wall-clock source per image, behind the one wall-clock symbol. A board can
supply its clock, every source that can fail falls under one stated policy,
and the libc shims answer from the same symbol.

Each item is one PR, claimed with `just claim phase-582-wN` before it starts.
No item starts before the user accepts RFC-0141.

## Why

Wall-clock time has one symbol and several unchosen answers:
- POSIX reads `CLOCK_REALTIME`.
- Zephyr reads an offset set once by SNTP or repeatedly by the zenoh HLC.
- FreeRTOS and ThreadX say "none", while FreeRTOS's libc shims say "ticks
  since boot" (issue 2691).

Also, no board can supply a clock (issue 0758 D4, phase-478 gap 5.4), and
SNTP never re-acquires. A consumer's board hook papers over that with
`std::exit(1)` (issue 0758, "Consumer requirement").

## Work items

### W1 — The seam: source knob, board hook, one offset store

- **Status:** OPEN
- **Owner:** nros 139
- Landed in four PRs, in order:
  - **W1a:** the one offset store, `nros/clock_epoch.h` (RFC-0141 D3), and
    Zephyr's per-port store retired onto it. Gate `check clock-epoch-store`.
  - **W1b:** the state query: `nros_platform_wall_clock_state()`, a
    platform-ABI symbol every port DEFINES (no default, so a missing port is a
    link error), and its faces `rclc_wall_clock_state()` (C),
    `rclcpp::Clock::wall_clock_state()` (C++) and
    `nros_core::wall_clock_state()` (Rust). The store keeps which source
    installed the offset and counts source changes. The `unstamped` default
    holds already (an unsynced clock reads 0, and the query says so).
    Re-split 2026-11-20 (manager ruling): the `halt` policy moved to W1c,
    because it needs W1c's `epoch_failure` knob and its end-of-boot
    resolution point.
  - **W1c:** `[knobs.clock]`: one source or an ordered fallback, boot-time
    resolution and latch, the opt-in monotonic upgrade, and a test per source.
    Also D4's `epoch_failure`: at the end of resolution, `unstamped` (the
    default) logs once and runs on; `halt` stops the image with one message
    naming the source(s). The halt test lands here.
    Split 2026-11-20 (manager ruling) into **W1c-1**: the registry's `enum`
    and `enum_list` kinds and the `[knobs.clock]` rows, the retired Zephyr
    switches, `nros/clock_resolve.h` (resolution, latch, the store's
    admission rule and monotonic floor, `epoch_failure`), host-tested with a
    fake clock (`check clock-resolve`); and **W1c-2**: the Zephyr wiring, the
    resolve point moved after registration, and native_sim cells (sntp,
    `["sntp", "platform"]` with no server, zenoh-hlc). W1c-2 must show the HLC
    sample is observable at the resolve point without the executor
    dispatching; if zenoh-pico surfaces it only through a dispatched
    callback, the wait gets a bounded pump that runs no user callbacks, and
    the doc says so. The shape refinements are recorded in RFC-0141 D1.
    W1c-2 as built: the clock gate (both runners, one primitive in
    `zephyr/nros_platform_zephyr_shims.c`), the single-executor entries
    resolving after registration (C, C++, and both Rust ones, `nros::main!`'s
    generated entry and `zephyr_component_main!`, which never resolved at
    all before), `Executor::restart_timing` (and
    `nros_cpp_executor_restart_timing`) when the gate opens. The native_sim
    cells are the configured `(ZephyrNativeSim, Rust, Zenoh, Pubsub, Example)`
    cells `+ [ClockHlc]` and `+ [ClockSntp]`, answered by
    `tests/zephyr_clock_resolve.rs`. W1c-2 also shipped a pump for
    ZPICO_DRIVE_CALLER builds, which issue 2791 found unreachable: the Zephyr
    port defines no `ZPICO_SERIAL`, so a serial image keeps its read thread,
    and a single-thread build boots into an executor refusal (no readiness
    form). The pump is deleted, and `CONFIG_NROS_ZENOH_MULTI_THREAD=n` is
    refused at configure time with that reason.
  - **W1d:** the board hook, its link rule and the selection-named TU, and
    the linux stub-board stamp test in Rust, C and C++.
- RFC-0141 D1–D4:
  - the `[knobs.clock]` group: `source` (one, or an ordered fallback),
    `fallback_settle_ms`, `upgrade`, `sntp_attempts`, `epoch_failure`
    (default `unstamped`);
  - the state query `rclc_wall_clock_state()` in C, C++ and Rust;
  - the board descriptor's default;
  - `bool nros_board_wall_clock_ns(uint64_t *)`, referenced only with
    `source = "board"`, and with NO default compiled then;
  - the board archive inside nano-ros's link group, with a `LINK_DEPENDS`
    edge;
  - one shared offset store and setter.
- Zephyr's per-port offset code is deleted in the same PR, as its own commit.
- **Acceptance:**
  - A linux stub board whose hook returns a fixed instant: `header.stamp`
    carries it through `Clock::system()` in Rust, C and C++.
  - `source = "board"` on a board without the hook fails to LINK, naming the
    symbol and the knob.
  - A board that does not select `board` is byte-for-byte unchanged in what
    it links.
  - The existing Zephyr SNTP and zenoh-HLC tests pass on the shared store.
  - `epoch_failure`: the default `unstamped` publishes a zero stamp and the
    state query says unsynced; `halt` stops the image. Each has a test.
  - A fallback `["sntp", "platform"]` with no SNTP server resolves to
    `platform` before the first stamp, and with `upgrade = "never"` stays
    there when the server appears.
  - With `upgrade = "monotonic"`, a source that arrives behind the current
    clock never makes a reading go backwards, and the step is counted in the
    state query.
  - Every clock test asserts a LOWER bound as well as an upper one. The
    latched and the monotonic-step cases must also show the clock ADVANCING
    (a later reading is at least the elapsed monotonic time past an earlier
    one), so a clock that never moves fails "never goes backwards" instead
    of passing it. Each such test is mutation-checked against a frozen
    clock.

### W2 — The FreeRTOS libc shims answer from the symbol

- **Status:** DONE (2026-11-22, the issue 2691 PR)
- **Owner:** sai (claim `issue-2691`), assigned by nros 139
- RFC-0141 D6, issue 2691:
  - `_gettimeofday` answers from `nros_platform_time_now_ns`;
  - `clock_gettime` honours `clock_id` (`CLOCK_REALTIME` from the symbol,
    `CLOCK_MONOTONIC` from the monotonic clock).
- **Acceptance:** on the MPS2 FreeRTOS QEMU image:
  - with no source synced, `gettimeofday` reads 0 s and `CLOCK_MONOTONIC`
    still advances;
  - with a stub board clock, both wall-clock paths read it.
- **As built:**
  - The class: every definition of a libc time entry point in the boards
    and platform ports, found by the sweep below (and its Rust twin,
    `rg -n -e 'fn (_gettimeofday|gettimeofday|clock_gettime|_times|time)\b' -g '*.rs' packages/boards packages/platform`,
    which finds none). It finds two, both FreeRTOS, both fixed.

    ```
    rg -n -e '^\s*(__attribute__\(\(weak\)\)\s*)?[A-Za-z_][A-Za-z0-9_ ]*\**\s*\b(_gettimeofday|gettimeofday|_gettimeofday_r|clock_gettime|clock_getres|clock_settime|settimeofday|_times|_times_r|time|clock|timespec_get)\s*\(' \
       -g '*.c' -g '*.cpp' -g '*.h' packages/boards packages/platform
    ```

    `_gettimeofday` (`nros-board-freertos/c/freertos_c_entry.c`) reads
    `nros_platform_time_now_ns`; newlib's `gettimeofday` and `time()` reach it.
    `clock_gettime` (`nros-platform-freertos/src/cyclonedds_compat.c`) reads
    that symbol for `CLOCK_REALTIME`, `nros_platform_clock_ns` for
    `CLOCK_MONOTONIC`, and refuses any other clock with `EINVAL`. `_times` is
    libnosys's failing stub, so `clock()` reports -1 rather than a time. The
    Rust lane links neither shim; zenoh-pico already reads the symbol through
    `zpico-sys`'s platform aliases.
  - `steady_clock` keeps a monotonic clock. This libstdc++ builds both
    `steady_clock::now` and `system_clock::now` over `gettimeofday`, so a
    wall `gettimeofday` alone would have frozen `steady_clock` at 0 and broken
    every ported node's rate limit and liveness age (issue 2691, "Why it is
    not a one-line fix"). The board defines both libstdc++ clocks beside the
    shim, `steady_clock` from `nros_platform_clock_ns` and `system_clock` from
    `nros_platform_time_now_ns`; both, because they share one archive member.
  - The stub board clock in the test is a link-time `--wrap` of
    `nros_platform_time_now_ns` in the probe image, since W1d's FreeRTOS hook
    is not built yet; the shims under test are the shipped ones.
  - Measured on the MPS2-AN385 QEMU image (`tests/freertos_libc_clock.rs`,
    fixture `freertos-cpp-zenoh-libc-clock`): unsynced, `gettimeofday`,
    `CLOCK_REALTIME`, `time()` and `system_clock` read 0, and
    `CLOCK_MONOTONIC`, `steady_clock` and the platform clock each advance at
    least the probe's 1.5 s sleep; with the stub, all four wall paths read the
    stub's instant to its last kept digit, and the monotonic clocks run on
    unstepped. On main the test fails at its first wall assertion
    (`gettimeofday` read ticks). Not measured: mps3-an536 and s32z270 build
    the same two shims, and freertos-posix has neither (glibc answers, and
    there `gettimeofday` and the platform symbol still disagree).

### W3 — SNTP re-acquires

- **Status:** OPEN
- **Owner:** nros 139
- RFC-0141 D5: `sntp_resync_interval_ms` (default 0, one-shot), re-setting
  the offset through W1's setter. It is issue 0758's consumer requirement.
- **Acceptance:** on Zephyr native_sim with a local SNTP server whose time
  is stepped mid-run, stamps follow the step within one interval, and with 0
  they do not.

### W4 — Board guidance and the per-backend stamp table

- **Status:** OPEN
- **Owner:** nros 139
- The board-package guidance states:
  - board hooks are linked inside nano-ros's link group, after its library;
  - a board archive placed elsewhere is the documented failure;
  - how to select `board`.

  It also gets the table RFC-0141 D7 asks for: where each RMW backend stamps
  its samples from, per platform.
- **Acceptance:** the table is checked against a built image per backend
  (a stamp read back), not written from reading code.

## Links

- [Issue 0758](../issues/0758-platform-sntp-epoch-source.md): SNTP, re-acquire, failure
  policy, and the board-time section (D4). W1, W3.
- [Issue 2691](../issues/archived/2691-freertos-tick-count-answers-as-wall-clock.md):
  W2.
- phase-478 gap 5.4 (no board time source over serial).
- phase-575 W6: the shape was first agreed there; the design moved here.
