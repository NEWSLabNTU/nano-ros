---
rfc: 0141
title: "Wall-clock source: one selected source behind the one wall-clock symbol"
status: Draft
area: platform-abi
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-582]
amends: []
supersedes: []
superseded-by: null
---

# RFC-0141 — Wall-clock source

**Status:** Draft (2026-11-19). The shape was agreed with the manager on
2026-11-18 as phase-575 W6, then moved here because it is a new seam on every
platform, not a board's own work. Decisions D1–D7 await the user's review.
Tracked by phase-582.

## Summary

Every reader of wall-clock time in nano-ros asks one symbol,
`nros_platform_time_now_ns()`. That covers:
- `Clock::system()` in nros-core;
- `get_system_time_ns` in nros-c;
- `rclcpp::Clock::now()` in nros-cpp;
- the executor's `epoch_us`.

What answers it differs by platform and is chosen by no one:

| platform | today's answer |
| --- | --- |
| POSIX | `CLOCK_REALTIME` |
| Zephyr | a stored offset plus the monotonic clock, set once by SNTP at boot or repeatedly by the zenoh HLC |
| FreeRTOS, ThreadX | 0 ("no wall clock") |

Two gaps follow from that:
- **No board can supply a clock.** A board with a gPTP-disciplined hardware
  clock on a TSN board, for example, has no seam to plug it in.
- **Ticks pass as time of day on FreeRTOS.** Two libc shims there answer
  wall-clock calls with ticks since boot, so time of day comes from somewhere
  other than the symbol (issue 2691).

This RFC makes the source a stated choice per image:
- ONE source is selected by a knob, never a cascade.
- A board supplies its clock through a hook that says whether it is synced.
- Every source that can fail falls under one stated failure policy.
- The libc shims answer from the same symbol.

## Motivation

- **Stale and jumping stamps come from a cascade.** A chain that tries SNTP,
  then the HLC, then the platform clock switches clocks mid-run when one
  source drops out. The stamps then jump. One consumer (Autoware behind an
  island) rejects commands as stale after such a step (issue 0758, "Consumer
  requirement").
- **A failure policy hard-coded in a board hook.** The same consumer's board
  hook retries SNTP 10 times and then calls `std::exit(1)`, because nano-ros
  has no stated policy for "no epoch".
- **A board clock has no seam** (issue 0758 D4; phase-478 gap 5.4). The board
  knows its PTP hardware clock; the platform port does not.
- **Two answers on FreeRTOS** (issue 2691). There:
  - the symbol says 0;
  - newlib's `gettimeofday` (and so `std::chrono::system_clock`) says ticks
    since the scheduler started;
  - Cyclone's `clock_gettime(CLOCK_REALTIME)` says the same ticks.

  A Cyclone image stamps its samples with boot time.

## Design

### D1 — Select one source; never cascade

A new knob group, `[knobs.clock]`, in the knob registry
(`packages/tooling/nros-zephyr-build/knobs.toml`), with one key:

```toml
[knobs.clock]
source = "board" | "sntp" | "zenoh-hlc" | "platform"
```

- `platform`: the port's own clock. POSIX `CLOCK_REALTIME`. On FreeRTOS and
  ThreadX it means "none", the honest answer.
- `sntp`: an SNTP exchange sets the offset (Zephyr today; FreeRTOS + lwIP
  per issue 0758's feasibility notes).
- `zenoh-hlc`: the zenoh session's HLC timestamps set the offset (Zephyr
  today, `CONFIG_NROS_EPOCH_FROM_ZENOH`).
- `board`: the board's hook (D2) answers.

The board descriptor states the default for its board; an image may override
it with the usual knob rungs. Exactly one source runs in an image. If it is
not synced, the answer is "no wall clock" under D4, never another source's
answer.

### D2 — The board hook says whether it is synced

```c
/* true: *out_ns is ns since the Unix epoch. false: not synced (no sentinel). */
bool nros_board_wall_clock_ns(uint64_t *out_ns);
```

- **With `source = "board"`, nano-ros compiles NO default.** The reference
  from the platform's clock TU then pulls the board's definition from its
  archive, and a board that does not define it fails to LINK. It does not
  silently get a default that always answers false.
- **Why no weak default.** Measured on GNU ld and on TI's tiarmlnk 5.1.1.LTS:
  a strong definition inside an archive never replaces a weak default that is
  already linked, whatever the archive order. With no default compiled, the
  member is pulled on both linkers, and a missing definition is "undefined
  reference" or "unresolved symbols remain", naming the symbol.
- **For other sources the symbol is not referenced.** A board that does not
  supply it is unchanged.
- **Build rule.** The board archive goes inside nano-ros's existing link group
  (the one `nano_ros_link_rmw` whole-archives the RMW backends into), after
  nano-ros's library. It gets a `LINK_DEPENDS` edge, so a hook edit relinks
  (issue 0475's class). A separate `target_link_libraries()` on the archive
  would reorder ld's single pass, so it is not used.
- **Readable error.** The generated TU that references the hook is named for
  the selection (`…set_by__knobs_clock_source`), and configure prints the
  board key and the knob, so the link error says which choice demanded it.

### D3 — One offset store, one setter

`sntp` and `zenoh-hlc` both produce an offset from the monotonic clock. Today
each port keeps that offset itself; Zephyr's is a spinlock-guarded pair in
`nros-platform-zephyr/src/platform.c`. It moves to ONE shared C file with one
setter (`nros_clock_set_epoch_offset`) and one reader, used by every port.
The per-port offset code is deleted in the same PR, as its own commit, guarded
by the existing SNTP and zenoh-HLC tests.

`epoch_us` is `time_now_ns / 1000`. The monotonic `clock_ns` is untouched.

### D4 — One failure policy for every source that can fail

```toml
[knobs.clock]
sntp_attempts = 3                   # sntp only
epoch_failure = "halt" | "unstamped"
```

When the selected source has not answered by the end of boot (`board` returns
false, SNTP has used its attempts, no HLC timestamp has arrived):
- `halt` stops the image with one message naming the source.
- `unstamped` runs with "no wall clock": `time_now_ns` returns 0, as
  `platform.h` already defines, and it says so once.

The image chooses; the board hook no longer decides by calling `exit`.

### D5 — SNTP re-acquires

`sntp_resync_interval_ms` (default 0, one-shot as today). Above 0, the SNTP
exchange repeats on that interval and re-sets the offset through D3's setter,
as the zenoh HLC already does. This is the consumer requirement in issue 0758:
after the server's time steps, stamps follow it.

### D6 — The libc shims answer from the same symbol

On FreeRTOS (issue 2691):
- `_gettimeofday` (newlib's backend for `gettimeofday`) answers from
  `nros_platform_time_now_ns`.
- `clock_gettime` honours `clock_id`: `CLOCK_REALTIME` from the same symbol,
  `CLOCK_MONOTONIC` from the monotonic clock.

With no source synced, `gettimeofday` reads 0 seconds since the epoch, which
is true, instead of boot time posing as time of day. Zenoh-pico's FreeRTOS
port and Cyclone then stamp from the selected source.

### D7 — Per-backend source timestamps are out of scope, and listed

Each RMW backend stamps its own samples:
- zenoh-pico through `gettimeofday` or the HLC;
- Cyclone through `clock_gettime(CLOCK_REALTIME)`;
- XRCE through the agent's time sync.

After D6, the first two read the selected source on FreeRTOS. Phase-582 lists
where each backend stamps from on each platform. Aligning the rest is a
separate decision per backend.

## Alternatives considered

- **A cascade** (board, else SNTP, else HLC, else platform). Rejected: a
  source dropping out changes the clock mid-run, which is the stale-stamp
  failure in Motivation.
- **A weak default board hook returning 0.** Rejected: 0 is a valid instant
  as well as a sentinel. Also, the measured linker rule means an archived
  board hook silently loses to the weak default.
- **Keeping the offset per port.** Rejected: three copies of one invariant
  (one setter, readers never tear). Zephyr's copy already needed a fix when
  a second writer (the HLC) arrived (issue 0758, 2026-09-29).

## Non-goals

- A PTP servo, or any clock discipline. A board's hook reads a clock the
  board keeps.
- Leap seconds and time zones. The value is ns since the Unix epoch, as
  `platform.h` defines it.
- Changing the monotonic clock or anything scheduled on it.

## Related

- Issue 0758 (SNTP, re-acquire, failure policy; its D4 board-time section).
- Issue 2691 (FreeRTOS libc shims).
- phase-478 gap 5.4 (no board time source over serial).
- phase-575 W6 (where this shape was first agreed; the design moved here).

## Changelog

- 2026-11-19: Draft, from the shape agreed for phase-575 W6.
