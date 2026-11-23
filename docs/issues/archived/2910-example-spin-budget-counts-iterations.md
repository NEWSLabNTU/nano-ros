---
id: 2910
title: "`NROS_SPIN_MS` in five Linux C/C++ examples counted spin calls as 100 ms each, so a busy image exited long before its budget"
status: resolved
type: bug
severity: medium
area: [examples, testing]
related: [phase-554, issue-2904]
found: 2026-11-24
resolved: 2026-11-24
---

## What happened

Five Linux examples bound their run with `NROS_SPIN_MS`, a budget in
milliseconds. Each loop counted iterations instead of measuring time:

```c
for (unsigned waited = 0; budget == 0u || waited < budget; waited += 100) {
    (void)rclc_executor_spin_some(&app.executor, 100ULL * 1000ULL * 1000ULL);
}
```

`spin_some` / `spin_once` take 100 ms as an UPPER bound and return as soon as
they have done work. An image with a timer and live sessions has work most of
the time, so each pass took far less than 100 ms and the budget ran out early.
The image then exited normally, printing nothing.

Measured on `examples/linux/c/lifecycle-pair` over XRCE, with the
`NROS_SPIN_MS=180000` that `lifecycle_pair_interop` passes: the image was gone
by about 100 s. Every later `ros2` call in the test failed against a peer that
no longer existed:

- with a 2 s `--spin-time`, `ros2 lifecycle set /lc_beta activate` answered
  `Node not found` at about 112 s;
- with a 4 s `--spin-time`, `ros2 lifecycle set /lc_alpha deactivate` printed
  nothing and was killed by its 20 s deadline at about 122 s.

Both look like discovery or service faults, and neither is one. A hand replay
of the test's exact steps showed the cause: `kill` reported the image's pid as
already gone. The zenoh and Cyclone cases of the same test passed, because
they reach their last step before the shortened budget runs out.

## Sites

The same idiom at all five (`git grep -n 'waited += 100' -- examples`):

- `examples/linux/c/lifecycle-pair/src/c_lifecycle_pair/src/main.c`
- `examples/linux/cpp/lifecycle-pair/src/cpp_lifecycle_pair/src/main.cpp`
- `examples/linux/c/lifecycle-node/src/c_lifecycle_node/src/main.c`
- `examples/linux/c/parameters/src/c_parameters/src/main.c`
- `examples/linux/cpp/parameters/src/cpp_parameters/src/main.cpp`

## Resolution

Every budget is now wall time: C reads `clock_gettime(CLOCK_MONOTONIC)` and C++
reads `std::chrono::steady_clock`. These examples are Linux-only, so a POSIX
clock is in reach. `git grep -n 'waited += 100' -- examples` is empty.

While diagnosing this, `lifecycle_pair_interop` gained a trailer on every
`ros2` call that does not exit 0, giving its exit status and elapsed time. An
empty failure now says it hit the deadline instead of saying nothing.
