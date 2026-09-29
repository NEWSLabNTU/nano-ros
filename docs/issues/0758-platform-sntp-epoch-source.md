---
id: 758
title: "No platform wall-clock epoch source — embedded consumers hand-roll
  SNTP before boot, and stamped messages are wrong until they do"
status: open
type: enhancement
area: core, boards
related: [rfc-0052]
---

## Problem

An embedded image has no wall-clock epoch at boot. Messages it stamps
(`now()` from a monotonic source) carry boot-relative time, and any peer
that validates stamps rejects them. The concrete consumer case
(autoware-safety-island, phase-3 driving demo): the island stamped
control commands from its boot epoch and Autoware's `vehicle_cmd_gate` /
monitors rejected them as stale — autonomous mode could never actuate
until ASI added its own SNTP step:

- a `platform_init_clock_via_sntp()` platform hook (Zephyr SNTP lib
  underneath, `CONFIG_SNTP_SERVER_ADDRESS` Kconfig),
- an unprivileged host-side `scripts/sntp-server.py` for tap/FVP setups,
- a call wired into its board network hook before `nros::init`.

That is ROS-infra living consumer-side. Every embedded nano-ros consumer
that talks to a stamped-message peer will need the same thing, and
timer/age machinery (RFC-0052 age monitors, `ExecutorConfig`-style epoch
knobs) wants the same epoch fact internally.

## Direction

A platform epoch source in nano-ros:

- Platform ABI: an optional `epoch_us()` / `set_epoch_us()` pair (or a
  one-shot `acquire_epoch()` hook) in the platform vtable — optional
  slot, `Option<fn>` like the rest (RFC-0054 rules; bindgen regen).
- A small SNTP client as the first provider (Zephyr has one in-tree;
  lwIP ships SNTP for the FreeRTOS family; POSIX boards just read
  `CLOCK_REALTIME`).
- Config: server address/port as a board/deploy fact (system.toml rung),
  not a consumer #define.
- Boot ordering: acquired after netif-up, before components construct —
  the board bring-up already has exactly that seam (network hook).

ASI then deletes its `platform_init_clock_via_sntp` copy and the
board-hook call; its `sntp-server.py` host helper can move to nano-ros
tooling or stay consumer-side.

## Non-goals

Full time sync (PTP, continuous discipline). One epoch acquisition at
boot is what stamped-message interop needs; drift handling can layer
later.

## Mechanism correction (2026-08-23) — the platform ABI has no vtable to add a slot to

The Direction says the hook goes "in the platform vtable — optional slot,
`Option<fn>` like the rest (RFC-0054 rules; bindgen regen)". That describes the
RMW ABI, not the platform one, and the difference changes the first work item.

Measured in `packages/platform/nros-platform-cffi/src/generated.rs`:

* **94** `pub fn nros_platform_*` declarations — link-time free functions;
* **2** `Option<unsafe extern "C" fn>` types, and both are CALLBACK typedefs
  (`nros_platform_log_flush_fn_t`, the timer callback), not dispatch slots.

`platform.h`'s own preamble states the rule: "links exactly one platform
implementation; resolution is at link time — **no runtime registration**". So
there is no table to append to and nothing to make `Option<fn>`.

### What the shape actually is

A free function in the SSoT header, per RFC-0054 — header first, then
`scripts/gen-abi-bindings.sh`, then commit both, with `check-abi-bindings`
gating staleness:

```c
/* nros_platform_epoch_us — wall-clock microseconds since the Unix epoch, or 0
 * when this platform has no epoch source. */
uint64_t nros_platform_epoch_us(void);
```

Optionality is expressed the way this header ALREADY expresses it for clocks,
which is why no new convention is needed — `platform.h` line 34:

> `clock_*` / `time_*` returns are absolute / monotonic counters and never
> error. If the platform has no clock, return `0`.

A returns-0 sentinel also fits the consumer: a caller that gets 0 knows the
image has no wall clock and can keep stamping boot-relative time rather than
publishing a wrong absolute one.

### Why this is not merely pedantic

The existing clock is `nros_platform_clock_ns`, documented at line 150 as
"monotonic nanoseconds since a platform-defined epoch (boot, program …)" and
introduced by RFC-0073 to REPLACE the `clock_ms`/`clock_us` pair. Adding an
`epoch_us` beside a monotonic `clock_ns` puts two clocks in one ABI whose names
differ by one word and whose meanings differ by "is it comparable with a peer's
timestamp". Whatever it is called, the doc comment has to say which one a caller
wants, or the next stamped-message bug is someone reaching for the wrong one.

### Revised first work item

1. `nros_platform_epoch_us()` in `platform.h`, returns-0 sentinel, doc comment
   contrasting it with `clock_ns`;
2. `scripts/gen-abi-bindings.sh` + commit `generated.rs`;
3. POSIX port implements it from `CLOCK_REALTIME` (the one port where it is
   free), every other port returns 0;
4. only then the SNTP provider, its config rung and the boot ordering.

Steps 1–3 are small, land green on their own, and give ASI something to call
before any SNTP code exists. The rest of the Direction stands unchanged.

## FreeRTOS / lwIP checked (2026-08-24) — feasible, but a DIFFERENT SHAPE

Checked rather than assumed, because the Direction lists lwIP beside Zephyr as
though the two were symmetric ports of one design. They are not, and someone
implementing the FreeRTOS half by analogy with the Zephyr one would build the
wrong thing.

### Available: yes

`third-party/freertos/lwip/src/apps/sntp/sntp.c` is vendored. Our FreeRTOS build
does not compile it — `nros-board-freertos/build.rs` carries an EXPLICIT lwIP
source list (core, ipv4+igmp, api/sockets, netif, the FreeRTOS `sys_arch`) and
no `src/apps/*`. Adding it is one line there plus `SNTP_*` defines in
`lwipopts.h`.

### The API is asynchronous, and lwIP has no synchronous one-shot

Zephyr's `sntp_simple(server, timeout, &ts)` BLOCKS and returns the time, which
is why W2 could acquire an offset inline at boot. lwIP offers no such call
(`grep -c 'sntp_simple\|sntp_request_sync' sntp.c` = 0). It is a background
daemon:

```c
void sntp_setservername(u8_t idx, const char *server);
void sntp_init(void);          /* starts polling; returns immediately */
```

and the time arrives through a COMPILE-TIME macro the port defines —
`SNTP_SET_SYSTEM_TIME(sec)`, or `SNTP_SET_SYSTEM_TIME_US(sec, us)` for the
precision this ABI wants.

### Why that is fine, and what it changes

The returns-0 sentinel absorbs it natively: `epoch_us()` answers 0 until the
first callback lands, then non-zero. No new convention is needed, and arguably
it is the BETTER fit — boot does not stall on a network round trip.

But one guarantee W4 makes on Zephyr does NOT carry over. There, the epoch is
acquired between netif-up and the first component, so no message is ever stamped
boot-relative. With lwIP the time arrives whenever the daemon gets a reply, so
early messages carry boot-relative stamps and the clock flips mid-run. A peer
that validates stamps will reject that opening window. Any FreeRTOS consumer has
to either tolerate it or wait for `epoch_us() != 0` before publishing — a
consumer-visible difference that belongs in the port's documentation, not
discovered at a `vehicle_cmd_gate`.

### Not implemented, for the reason #0750 was closed

No FreeRTOS consumer has asked. The named demander is a Zephyr island
(`nano_ros_use_board(fvp-aemv8r-smp)`); the plausible-but-unasked FreeRTOS case
is S32Z270 (phase-372, Cortex-R52 automotive), which stamps nothing today. The
port stays at `0` — correct, not unfinished — until someone needs it, and this
section is what they should read first.

## Re-acquisition is a REQUIREMENT, not "drift handling later" (2026-08-24)

The Non-goals section says one epoch acquisition at boot is what stamped-message
interop needs, and that drift can layer later. Measured evidence from the ASI
consumer says the opposite for any consumer whose tick is not real-time-paced:
a one-shot epoch is unbounded-error by construction, because the epoch is
advanced afterwards by the platform's own tick.

Measured on the ASI Zephyr FVP island (one-shot SNTP in its boot network hook,
then the FVP tick):

    island stamp 1787579546  vs  host wall 1787575852   offset  3694 s
    +16 s of wall clock later                            offset  3854 s

i.e. the island's wall-clock estimate advances ~10.5x real time whenever the
model is idle (the FVP fast-forwards an idle guest; its rate limiter lives in
the visualisation component and does not pace idle time). The error has no
bound — it grows for as long as the image runs.

What that cost downstream: Autoware's `mrm_emergency_stop_operator` seeds its
braking ramp over `dt = now - input.stamp` with no sanity clamp, so a
future-stamped command turned an emergency STOP into `a = +7127 m/s^2`
(measured: `a0 = -1.5 + 3.0 * 2376`, where 2376 s was the offset at that
moment) and drove a simulated vehicle to its speed clamp. The peer's missing
clamp is its own bug — but the input that triggered it was a one-shot epoch
left to drift.

Consequences for this design:

- `acquire_epoch()` must be RE-callable, and the port should re-acquire on an
  interval (or slew), not once at boot. The returns-0 sentinel already models
  "not yet"; nothing in the ABI needs to change to allow a second call, but the
  contract has to SAY it is allowed and the providers have to do it.
- The interval belongs with the other deploy facts (alongside server
  address/port), because the tolerable error is a property of the deployment:
  ppm-level on real silicon, ~10x on a free-running simulator.
- Worth stating in the port documentation: consumers that stamp messages for a
  validating peer should treat a monotonic-only clock as unsafe to stamp with,
  the same way the lwIP note above tells them to wait for `epoch_us() != 0`.

Full investigation record (probe method, per-sample numbers, the two halves of
the chain) lives in the ASI consumer's phase-4 doc, section "MRM divergence —
investigated and root-caused".

## The epoch never reached the wall clock on Zephyr (2026-09-29)

W2/W4 installed the epoch behind `nros_platform_epoch_us`, but every reader of
the WALL clock asks a different symbol, `nros_platform_time_now_ns`:
`nros_clock_get_now_ns(NROS_CLOCK_SYSTEM_TIME)` (nros-c `get_system_time_ns`),
`rclcpp::Clock().now()` in nros-cpp, `nros_core::Clock::system()` and the
executor's `default_epoch_us`. The Zephyr port hard-coded that one to `return 0`,
so an image with `CONFIG_NROS_SNTP_EPOCH` and an acquired epoch still had no
wall clock anywhere a node could reach it; the only live consumer of
`epoch_us` was a test bin.

Found by the ASI consumer (phase8-W18): its island stamps
`/system/emergency/hazard_lights_cmd` from the monotonic clock, and Autoware's
`vehicle_cmd_gate::getContinuousTopic` keeps its previous hazard, turn and gear
command whenever the new one is stamped EARLIER. A boot-relative stamp is
always earlier than the host-stamped command it replaces, so the gate held
DISABLE for the whole MRM, logging "The operation mode is changed, but the
HazardLightsCommand is not received yet". Moving the island to
`rclcpp::Clock(NROS_CLOCK_SYSTEM_TIME)` needed the platform to answer.

A second gap sat in front of the first: W4 wired the acquisition into the
TIERED entries only. The generated C++ single-executor entry
(`ZephyrBoard::run_components`, which ASI's island uses) and the C one
(`nros_board_rtos_run_components`) went straight from `nros_board_network_wait()`
to `nros::init`, so `nros_zephyr_epoch_acquire_configured` was not even linked
into such an image (measured: absent from `nm zephyr.exe` with
CONFIG_NROS_SNTP_EPOCH=y, and no "epoch acquired" line at boot).

Fix, both halves:
* both single-executor entries call `nros_zephyr_epoch_acquire_configured()`
  (under `__ZEPHYR__`) right after `nros_board_network_wait()`, the window the
  tiered entries use;
* the Zephyr port's `nros_platform_time_now_ns` returns
  `nros_platform_epoch_us() * 1000` -- 0 until an epoch is installed, exactly
  the sentinel it returned before.

FreeRTOS and ThreadX still answer 0 from both symbols, so they stay consistent
without a change. Re-acquisition (the section above) is still open. Measured
on ASI's native_sim island after the fix: "nros: wall-clock epoch acquired from
127.0.0.1:12323" at boot, and its hazard/MRM stamps compare with host time.
