---
id: 1696
title: "Cyclone's receive thread retries a failed `select` with no back-off, so any persistent failure floods the Zephyr console"
status: open
type: bug
area: [rmw-cyclonedds, zephyr]
severity: medium
found: 2026-10-05
related: [1674, 0507]
---

## Summary

Split from issue 1674 ([archived](archived/1674-zephyr-native-sim-cyclonedds-delivers-nothing-and-floods-select-failed.md)), which recorded this as a leftover.

When `select()` fails, Cyclone's receive thread (the sockwaitset loop in the
vendored fork) retries at once and logs every failure. Under 1674's cause,
`ENOMEM` from `zsock_select` past `CONFIG_NET_SOCKETS_POLL_MAX`, that loop
printed `select failed` without limit. One e2e cell's console reached 91 GB
before the kernel killed the test (issue 1697 covers the harness side).

1674 removed that cause. The loop is unchanged, so the next persistent
`select` failure, whatever causes it, will flood the console the same way.
The flood also hides the one line that names the cause.

## Fix direction

Back off in the receive loop after consecutive failures: log the first failure
and a periodic count, not every retry. This is a change to the Cyclone fork
(`third-party/dds/cyclonedds`, patch branch). It needs a row in
`docs/reference/cyclonedds-fork-delta.md`, and the maintainer pushes it, since
the agent does not push fork remotes.

## Acceptance

* With `CONFIG_NET_SOCKETS_POLL_MAX=3` forced (1674's reproduction), the
  console shows the failure and a rate-limited count, not an unbounded stream.
* The fork-delta table lists the commit.
