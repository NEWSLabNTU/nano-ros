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
- RFC-0141 D1–D4:
  - the `[knobs.clock]` group: `source` (one, or an ordered fallback),
    `fallback_settle_ms`, `upgrade`, `sntp_attempts`, `epoch_failure`
    (default `unstamped`);
  - the state query `nros_wall_clock_state()` in C, C++ and Rust;
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

### W2 — The FreeRTOS libc shims answer from the symbol

- **Status:** OPEN
- **Owner:** nros 139
- RFC-0141 D6, issue 2691:
  - `_gettimeofday` answers from `nros_platform_time_now_ns`;
  - `clock_gettime` honours `clock_id` (`CLOCK_REALTIME` from the symbol,
    `CLOCK_MONOTONIC` from the monotonic clock).
- **Acceptance:** on the MPS2 FreeRTOS QEMU image:
  - with no source synced, `gettimeofday` reads 0 s and `CLOCK_MONOTONIC`
    still advances;
  - with a stub board clock, both wall-clock paths read it.

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
- [Issue 2691](../issues/2691-freertos-tick-count-answers-as-wall-clock.md):
  W2.
- phase-478 gap 5.4 (no board time source over serial).
- phase-575 W6: the shape was first agreed there; the design moved here.
