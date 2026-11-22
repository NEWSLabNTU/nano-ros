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
    the linux stub-board stamp test in Rust, C and C++. Built under the user's
    2026-11-22 ruling, which amends RFC-0141 D2. The hook is an object of the
    image or an archive linked after nano-ros (no link group exists for
    zenoh or XRCE), and `board` is a resolution source through the one
    setter.
    - The POSIX port resolves lazily on the first wall-clock read.
    - Host tests (`check clock-resolve`): the stub clock, the fallback, an
      unanswered `board` reading 0, `halt`, the link error naming the hook,
      and an unselected build referencing nothing.
    - Image cells `(Linux, Rust|Cpp, Zenoh, Pubsub, Example) + [ClockBoard]`
      (`tests/linux_board_clock.rs`): the board image's public wall clock and
      its zenoh source stamps sit in the stub instant's window, and its state
      reads synced, `board`. The ordinary sibling stamps host time and links
      no hook.
    - **C is not covered:** a C image's configure also builds every
      component's executable against the board-selected runtime (issue
      2841). Until that is fixed, such an executable is refused at configure
      with a clear message (manager ruling), and a verdict cell covers it.
    - **The settle window (manager ruling, 2026-11-22):** `board` behaves
      like `sntp`. While nothing is latched it is asked every 10 ms within
      `clock.fallback_settle_ms`. The first answer latches `board`; a window
      that runs out moves on to the next source, then `epoch_failure`. The
      hook is never called after resolution. The poll is the one
      selection-named TU (`nros-platform-api/c/`), shared by every port, and
      the policy defaults are `nros/clock_resolve.h`'s
      (`nros_clock_policy_from_build`). A test holds the C settle default
      equal to the registry's. Host tests: a hook that answers after 200 ms
      is waited for, an unanswered one costs the window then reads 0 or
      halts, and later reads ask nothing. Mutation controls: a TU that asks
      once fails, and a port that resolves on every read fails.
    - **FreeRTOS resolves `board`** the same lazy way, once, from the first
      wall-clock read in a running task. The once is the port's critical
      section and a phase flag, and a read before the scheduler runs reads 0.
      The verdict goes through the board's log writer. Bare-metal `platform`
      does not answer. On the freertos-posix board (the FreeRTOS POSIX port)
      `platform` is the host's `CLOCK_REALTIME`, with no list stated too, so
      that board's default moves from 0 to host time. Every copy of the port's
      `platform.c` gets the same selection: the family and freertos-posix
      board crates from `clock_selection`, and the CMake library from
      `nros_clock_selection_apply` (`cmake/NanoRosClockSelection.cmake`, the
      CMake mirror of the same rules, reading the value the image's cargo
      commands see). Host test `tests/clock-board-freertos-tests.sh`: the real
      kernel on its POSIX port and the real port, built through its
      CMakeLists.
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
  - With the wall clock reading 0, the DDS and zenoh stacks still discover and
    deliver:
    - Cyclone: `freertos_qemu`'s
      `an536_cyclonedds_cpp_entry_delivers_within_the_dds_heap_default` (the
      `(FreertosMps2, Cpp, Cyclonedds, EntryPubsub, Workspace)` cell, booted
      on mps3-an536 and linking this `clock_gettime`) passes.
    - zenoh: `rtos_e2e`'s FreeRTOS C++ pubsub cell passes.
    - XRCE has no FreeRTOS cell (carved out). By reading, it is unaffected:
      an embedded build compiles neither of upstream's `util/time.c` arms,
      and `uxr_millis`/`uxr_nanos` come from the backend's
      `platform_aliases.c` over `nros_platform_clock_ns`.
  - No FreeRTOS-reachable code waits on the wall clock (no
    `std::chrono::system_clock` deadline, no `wait_until`/`sleep_until` on
    it), so RFC-0141 D6 now states that wall-clock deadlines are not
    supported before sync.

### W3 — SNTP re-acquires

- **Status:** DONE (2026-11-22, #4609)
- **Owner:** nros 139
- RFC-0141 D5: `sntp_resync_interval_ms` (default 0, one-shot), re-setting
  the offset through W1's setter. It is issue 0758's consumer requirement.
- **Acceptance:** on Zephyr native_sim with a local SNTP server whose time
  is stepped mid-run, stamps follow the step within one interval, and with 0
  they do not.
- **Done:**
  - **Knob.** `clock.sntp_resync_interval_ms` (image, default 0, range 0 to
    one day) in the knob registry, generated into Kconfig as
    `CONFIG_NROS_CLOCK_SNTP_RESYNC_INTERVAL_MS`.
  - **Thread.** `zephyr/nros_platform_zephyr_shims.c` defines one static,
    not-yet-started, lowest-application-priority thread, built only when
    `CONFIG_SNTP` is set and the interval is above 0. Resolution starts it when
    the image lists `sntp` among its sources. Every interval it re-runs the
    boot exchange and installs through the store's one setter, so the image's
    `upgrade` policy decides whether the install is admitted, with no second
    rule. A failed query keeps the installed offset and is reported once per
    failing streak. It runs on its own thread because `sntp_simple` blocks
    for up to `sntp.timeout_ms`.
  - **Cells.** `+ [ClockSntpResync]` and `+ [ClockSntpOnce]` on
    `(ZephyrNativeSim, Rust, Zenoh, Pubsub, Example)`. They are the Rust
    workspace's `zephyr_talker_clock_sntp_{resync,once}` images, each with its
    own loopback port, answered by two new cases in
    `tests/zephyr_clock_resolve.rs`:
    - the test runs an SNTP server and a message-info observer;
    - it checks the source stamps start at the server's time;
    - it steps the server +1000 s;
    - with 500 ms the stamps follow within one interval plus one publish
      period, and with 0 they stay unstepped over eight intervals.
  - **Acceptance met** (host 241, at #4609's gated head): both cases pass on Zephyr
    3.7 (10.4 s and 12.0 s) and 4.4 (8.4 s and 12.1 s).
  - **Fix found on the way.** The workspace image parser took the FIRST
    config token prefixing an image name, so `clock_sntp` swallowed
    `clock_sntp_resync`. It now takes the longest.

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
- [Issue 2841](../issues/2841-c-configure-builds-component-exes-against-image-runtime.md):
  W1d's C stub-board image cannot build until a C image's configure stops
  building every component's executable against the image's runtime.

- phase-575 W6: the shape was first agreed there; the design moved here.
