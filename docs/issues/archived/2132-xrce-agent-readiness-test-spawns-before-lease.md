---
id: 2132
title: "The XRCE Agent readiness unit test spawns its `sleep 5` stand-in before leasing a port, so a slow lease turns the never-bound case into the exited case"
status: resolved
type: bug
area: [testing, xrce]
severity: low
found: 2026-10-29
related: [1932]
resolved: 2026-10-29
---

## What

`fixtures::xrce_agent::readiness_tests::a_process_that_never_binds_times_out`
(`packages/testing/nros-tests/src/fixtures/xrce_agent.rs`) spawns `sleep 5` as
an agent that runs and never binds. Only then does it call
`port_lease::lease_port(Udp)`, and only after that does it start
`wait_until_listening(300 ms)`.

If the lease takes longer than 5 s, the stand-in has already exited when the
probe starts. The probe then correctly reports the EXITED branch, and the
test's `did not bind` assertion fails:

```
the timeout must say the port was never bound, got: Other(ProcessFailed("XRCE Agent exited before binding udp4 port 36533 (exit status: 0)"))
```

## Measured

- Seen once, during `test-lane-contracts` in a combined `just ci gate`. The
  test took 11.5 s there. In the same run,
  `port_lease::tests::a_dead_owners_lease_is_reclaimed` took 11.1 s, so
  leasing was slow on that loaded host.
- Solo, 3/3 pass in 0.31 s.

## Fix

Lease the port BEFORE spawning the stand-in. A longer `sleep` would also work
(the process-group Drop reaps it at once), but it only moves the race. The
ordering fix removes it.

## Resolution

The test now leases the port before it spawns the stand-in, and nothing
slow sits between the spawn and the probe. The lease's duration no longer
eats into the stand-in's 5 s life.

Measured (`cargo nextest run --lib readiness_tests`, nros-tests):

- Negative control: a 6 s `thread::sleep` placed where a slow lease lands.
  With the old order (spawn, lease, probe) the test fails with the exact
  message above (`XRCE Agent exited before binding udp4 port … (exit
  status: 0)`). With the new order (lease, spawn, probe) it passes in 6.3 s.
- Solo: 3/3 runs pass, 0.32 s.
- Under load (2x nproc `yes` burners): 10/10 runs of all three readiness
  tests pass. CPU load alone did not slow the lease on this host, so the
  slow-lease control above is the real evidence.
