# Running the real-time tests

> The model these tests measure is
> [The Real-Time Execution Model](../../realtime/execution-model.html) in
> the nano-ros Book. A level's budget has exactly one enforcer (issue 2018);
> S3 was measured on the S32K344 safety island in phase-509.

Three tests need the kernel's real-time scheduling, and live in
`packages/testing/nros-tests/tests/rt_sched_levels_e2e.rs`:

- the preemption order of `levels-cpp`'s three levels on one CPU;
- a spawned level past its `SCHED_DEADLINE` budget, throttled by the kernel;
- S1 (1 kHz critical loop beside 5 ms telemetry; see the scenario table on
  the user page).

Only host-tests' `rt-sched` job claims the `rt-sched` capability, so
everywhere else each is a `[SKIPPED:lane]`. None of them needs `sudo`. By
default each image runs in a throwaway docker container granted `SYS_NICE`
and an RT priority limit, so a member of the `docker` group can run them on
any Linux host:

```sh
NROS_FIXTURE_ID=workspace-cpp-native-levels-s1 scripts/build/workspace-fixtures-build.sh linux cpp
# ... and -levels, -levels-s1-flat, -levels-budget
NROS_TEST_UNCLAIMED= cargo nextest run -p nros-tests --test rt_sched_levels_e2e
```

The container's image is the host's own distribution and release, so the
binary finds the C++ runtime it was linked against; `NROS_RT_SCHED_IMAGE`
names another. `NROS_RT_SCHED_LAUNCHER=direct` runs the binaries as they are,
for a process that already holds the capability: a CI job container started
with `--cap-add SYS_NICE`, or fixtures given `setcap cap_sys_nice+ep` (bound
to the file's contents, so again after every rebuild).
`NROS_RT_SCHED_CPU` moves the pinned runs off CPU 0.

S1 asserts zero misses only on a `PREEMPT_RT` kernel. A generic kernel misses
the odd release for any program: a bare C loop at the same priority missed 2
in 60 s. There the test asserts a miss rate at least 50 times below the
one-level control's, which is what the levels change.

A run that claims `rt-sched` on a host without the capability fails with
`[UNMET PRECONDITION]` and names the binary to grant it to.

## WCET benchmark harness

The DWT cycle-count harness described in
[Real-Time Analysis](../../guide/realtime/analyze-timing.html) is the in-tree
`rs-wcet-bench` QEMU fixture:

```bash
# Build all QEMU examples (includes rs-wcet-bench)
just qemu build

# Run the WCET benchmark
just qemu test-wcet

# Extract the harness's artifact from the run log
just qemu wcet-artifact
```

`test-wcet` is not part of the QEMU test group: under QEMU the DWT counter
reads zero, so the bench refuses to emit numbers. Run it on a hardware lane
for real cycle counts.

