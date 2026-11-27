---
id: 2973
title: "A process that aborts while the deadline is pending reads as `timeout`'s 124, so a check that treats 124 as \"still running\" passes a crash"
status: resolved
resolved: 2026-11-27
resolved_in: "the change that archives this issue (446c8e8d8d fixed the first site)"
type: bug
severity: high
area: [testing, gates, scripts]
related: [issue-1723, issue-1741, phase-488, phase-590]
found: 2026-11-26
---

## What happened

`build::check::cpp-runtime` went red in a `ci gate` on a loaded host, in
`scripts/check-rclcpp-examples-boot.sh`'s own negative control:

```
selftest: an aborting example was NOT reported
check-rclcpp-examples-boot: selftest FAILED -- the classifier cannot be trusted
```

The control's fake example is `kill -ABRT $$`. It took the signal at
00:38:54; systemd-coredump recorded the dump at 00:39:00 (`coredumpctl`,
`core_pattern` is a pipe to `systemd-coredump`), about 6 s for a 26 KiB core,
while the host ran a `ci gate` beside another project's cargo build. The
example runs under the shared deadline, `"${NROS_DEADLINE[@]}" 3`, and GNU
`timeout` exits **124** whenever its timer fired, whatever the child then died
of. The 3 s timer fired during the dump, `timeout` said 124, and the classifier
reads 124 as "still spinning at the deadline", i.e. BOOTED.

So the selftest was right to fail: the classifier really cannot tell an abort
from a boot once a dump outlasts the deadline. A real ported rclcpp example
whose create fails (`detail::require_created` aborts, the regression this check
exists for) would have passed green on the same host.

Reproduced deterministically with a child that dies by SIGABRT only after the
deadline's SIGTERM (`trap "kill -ABRT \$\$" TERM`):

| child | `timeout -k3 1` | `timeout -k3 --preserve-status 1` |
|---|---|---|
| aborts after the deadline fired | **124** | 134 |
| `exec sleep 30` | 124 | 143 |
| ignores SIGTERM | 137 | 137 |

## Fix (this check)

`check-rclcpp-examples-boot.sh` runs the deadline with `--preserve-status`, so
the status judged is the example's own: 0 or 143 (our SIGTERM) or 137 (our
SIGKILL) booted, anything else failed. The selftest gains two cases: a
TERM-ignoring example (137) must boot, and an example that aborts AFTER the
deadline fired must be reported. The pre-fix classifier fails the new case and
passes every old one, which is the mutation check.

## The class: every "124 means still alive" reading

Swept with `git grep -nE '\b124\b' -- scripts just` plus every
`"${NROS_DEADLINE[@]}"` caller. A deadline whose 124 means FAILURE (a hang)
is not affected: a crash that reads as a hang is still red. Only a 124 read as
SUCCESS is. Those sites:

- `scripts/check-rclcpp-examples-boot.sh`: fixed here.
- `scripts/ci/lib/image-context.sh` (the ROS image's build-time router check:
  `timeout 3 … nros_router_exec …; test "$rc" -eq 124`). A router that aborts
  in its first 3 s with a dump that outlasts the remainder would pass the image
  BUILD. Not changed here: the router may handle SIGTERM and exit 0, so
  `--preserve-status` alone changes what "alive" reads as and needs its own
  measurement inside the image. Fixed 2026-11-27, see the resolution below.
- `scripts/check-shared-build-dir.sh`'s negative control (`[ "$rc" -eq 124 ]`,
  "the raw cargo deadlocked"). A cargo that crashed mid-dump would satisfy the
  control. The control's subject is a lock deadlock, not a crash, and its
  positive case still has to finish; left as is, recorded here.

Whoever touches one of the open sites should use `--preserve-status` and judge
143/137 as "alive at the deadline", as this check now does.

## Resolution (2026-11-27): the router check

The measurement the open site needed, with ROS humble's `rmw_zenohd` (the
package the image's ROS layer installs) on host 118:

| subject | `timeout 3` | `timeout --preserve-status 3` |
| --- | --- | --- |
| a live router | 124 | **0** (it exits cleanly on SIGTERM) |
| a command that exits 0 before the deadline | 0 | **0** |
| a command that aborts | 124 if still dumping | 134 |

So `--preserve-status` alone cannot be the router's rule: a live router and
one that exited cleanly on its own read the same. The router check needs two
facts, and `nros_router_alive_after <secs> <cmd>` (`scripts/dev/zenohd.sh`,
already copied into the image) asks both:
- the process is present, and not a zombie, at the deadline;
- it then exits 0 or 143 on our SIGTERM. A process still dumping core is
  present, then exits with its signal status (134, 139).

The ROS layer's build-time `RUN` uses it; the real router passes it (`state=Sl,
exit on SIGTERM rc=0`). `just check router-alive-probe` holds it with
stand-ins for every verdict, plus the negative control that `timeout`'s 124
passes the crashing one.

`scripts/check-shared-build-dir.sh`'s control stays as recorded above: its 124
means a deadlock, not life.

