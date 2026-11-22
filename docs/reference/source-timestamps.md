# Source timestamps per RMW backend

A sample's **source timestamp** is the instant its publisher stamped it. A
subscriber reads it from `MessageInfo::source_timestamp()` in Rust,
`rmw_message_info_t.source_timestamp` in C, and the same field in C++.

nano-ros selects one wall clock per image with `[knobs.clock] source`
(RFC-0141). Every nano-ros reader of wall-clock time asks that clock:
`Clock::system()`, `rclcpp::Clock::now()` and the executor. The RMW backend
stamps a sample itself, though, so the stamp follows the selected clock only
where the backend asks nano-ros for the time (RFC-0141 D7). This table
records, per backend and platform, where the stamp comes from.

## How a row is measured

A row is measured on a built image. The publisher's wall clock is made
unmistakable: a stub board whose `nros_board_wall_clock_ns` answers
2023-11-14T22:13:20Z (`clock_board_pkg`, `[knobs.clock] source = "board"`).
A subscriber built for the same backend then prints each sample's source
stamp. The talker prints its own wall clock in the same run, which shows the
board clock is the image's clock. A stamp outside the board's window is the
backend's own choice of clock.

Each row's evidence is the test function that read the stamp back. A row
that has no such test says `not measured` and why. Its "follows" column then
says `unknown`, however clear the code looks: a table written from reading
code is what this page replaces.
`just check source-timestamp-table` enforces both rules, and also that every
RMW backend has at least one measured row.

## The table

- **follows**: whether the stamp is the image's selected wall clock.
  - `yes`: it is.
  - `no`: the backend reads another clock.
  - `no stamp`: the backend delivers 0.
  - `unknown`: not measured.
- **evidence**: `<test binary>::<test function>` under
  `packages/testing/nros-tests/tests/`.

<!-- source-timestamp-table:begin -->
| backend | platform | source stamp from | follows | evidence |
| --- | --- | --- | --- | --- |
| zenoh | linux | the nano-ros zenoh shim: `nros_platform_time_now_ns`, the selected wall clock | yes | `linux_board_clock::board_clock_reaches_the_wall_clock_and_the_source_stamp` |
| cyclonedds | linux | Cyclone's own `ddsrt_time_wallclock`: the host's `CLOCK_REALTIME` | no | `linux_board_clock::board_clock_source_stamp_per_backend` |
| xrce | linux | nothing: the XRCE client delivers no source stamp, so every sample reports 0 | no stamp | `linux_board_clock::board_clock_source_stamp_per_backend` |
| zenoh | zephyr | the nano-ros zenoh shim: the image's wall clock, here an SNTP source the test steps | yes | `zephyr_clock_resolve::zephyr_sntp_resync_follows_a_step_of_the_server_time` |
| zenoh | freertos | expected: the shim, as on linux | unknown | not measured: no FreeRTOS image has a board-clock or SNTP configuration with a stamp observer |
| zenoh | threadx | expected: the shim, as on linux | unknown | not measured: no ThreadX image has a board-clock configuration with a stamp observer |
| zenoh | nuttx | expected: the shim, as on linux | unknown | not measured: no NuttX image has a board-clock configuration with a stamp observer |
| cyclonedds | zephyr | expected: Cyclone's `ddsrt_time_wallclock` | unknown | not measured: no Zephyr Cyclone image has a configured clock with a stamp observer |
| cyclonedds | freertos | expected: Cyclone's FreeRTOS `ddsrt_time_wallclock` | unknown | not measured: no FreeRTOS Cyclone image has a configured clock with a stamp observer |
| cyclonedds | threadx | expected: Cyclone's `ddsrt_time_wallclock` | unknown | not measured: no threadx-linux Cyclone image has a configured clock with a stamp observer |
| xrce | zephyr | expected: no stamp, as on linux | unknown | not measured: no Zephyr XRCE image has a stamp observer |
| xrce | freertos | expected: no stamp, as on linux | unknown | not measured: no FreeRTOS XRCE image has a stamp observer |
| uorb | px4 | expected: none: the backend has no take-with-info entry, so no `MessageInfo` reaches a subscriber | unknown | not measured: the backend runs only inside PX4, and no PX4 SITL cell reads a subscriber's `MessageInfo` |
<!-- source-timestamp-table:end -->

## What the measurements found

The board's instant was 1 700 000 000 000 000 000 ns and the host clock read
about 1 795 356 135 000 000 000 ns (2026-11-22), so each reading's clock is
plain.

- **zenoh, linux.** Every stamp was in the board's window. This holds in
  Rust and in C++. The ordinary talker, which selects no source, stamps host
  time.
- **cyclonedds, linux.** The talker's wall clock read
  1 700 000 000 000 004 000 (the board). Its samples' stamps were
  1 795 356 135 300 244 223, 1 795 356 136 300 245 743 and
  1 795 356 137 300 270 276. Those are host time, inside the two host reads
  that bracket the run. So Cyclone stamps from `CLOCK_REALTIME`, not from
  the selected clock.
- **xrce, linux.** The talker's wall clock read 1 700 000 000 000 005 000 (the
  board). Every sample's source stamp was 0. The XRCE client receives no
  writer stamp from the Agent, so the subscriber's `MessageInfo` carries
  none, whatever clock the publisher selected.

Aligning Cyclone with the selected clock, or giving XRCE a stamp, is a
separate decision per backend (RFC-0141 D7).
