---
id: 1697
title: "The e2e harness buffers a guest's console with no bound — one flooding Zephyr image grew the test process to 91 GB"
status: resolved
type: bug
area: [testing]
severity: high
found: 2026-10-05
related: [1674, 1696]
---

## Summary

Split from issue 1674 ([archived](1674-zephyr-native-sim-cyclonedds-delivers-nothing-and-floods-select-failed.md)).

`nros_tests::zephyr`'s `spawn_stream_reader` (`packages/testing/nros-tests/src/zephyr.rs`)
reads the guest console in a loop and calls `push_str` on a shared
`Arc<Mutex<String>>` for every chunk, with no limit. A guest that loops on a
log line (issue 1696) grows that buffer until the host runs out of memory.
Measured during 1674: one Cyclone cell reached 91 GB before the kernel killed
it, and a separate run's nextest log reached 55 GB. A test failure became a
host-wide out-of-memory event that also hit other sessions.

The other process wrappers with `wait_for_output` (`process.rs`, `qemu.rs`,
`ros2.rs`, `ros_env.rs`) have not been audited for the same shape. Fix the
class, not this one site.

## Fix direction

One shared capture type with a byte cap. It keeps the head, where boot and the
first error are, plus a rolling tail, and records how many bytes were dropped,
so a failure message can say "console truncated, N bytes dropped". Every
reader uses it. A gate refuses a new unbounded `push_str` into a capture buffer.

## Acceptance

* A guest that writes without limit leaves the test process's memory bounded.
  Show this with a fixture or a synthetic writer, not by reading the code.
* Failure text still includes the guest's first error line and says that
  output was truncated.

## Resolution (2026-10-06)

There is now one bounded spelling, `nros_tests::capture::append(&mut out, &buf[..n])`
(`packages/testing/nros-tests/src/capture.rs`), and all 15 read loops under
`packages/testing` use it:

- `zephyr.rs`'s `spawn_stream_reader`;
- 7 in `process.rs`;
- 3 in `ros2.rs`;
- `qemu.rs`'s `drain_into`;
- `lib.rs`'s `collect_output`;
- the two in `tests/logging_smoke.rs`.

The capture stays a plain `String`, so no caller's pattern search changed.

It keeps the first 1 MiB, where the boot and the first error are. Then comes a
marker line, `[nros-tests: console truncated, N bytes dropped]`, which every
failure text that quotes the capture carries. Then a rolling tail of the latest
output. It compacts once it passes 10 MiB, back to 8 MiB.

Measured with a real child: `a_flooding_child_leaves_the_capture_bounded` runs
`yes` through `ManagedProcess::wait_for_output` for 4 s. With the cap the
capture stays bounded, keeps the child's first line and counts what it dropped.
With the cap disabled, the same test captured 3 625 103 539 B in those 4 s.

The unit tests check that every byte is either kept or counted, over 64 MiB of
flood, and that multi-byte characters are never cut.

Gate: `check-test-capture-bounded` (`just check test-capture-bounded`) refuses
the raw `push_str(&String::from_utf8_lossy(&buf[..n]))` anywhere under
`packages/testing`, except in `capture.rs` itself. On `main` before this fix it
reports 7 sites in `process.rs` alone.

Not bounded: `wait_for_pattern`'s `read_line`. It clears its buffer on every
line, so only a single line with no newline could grow it.
