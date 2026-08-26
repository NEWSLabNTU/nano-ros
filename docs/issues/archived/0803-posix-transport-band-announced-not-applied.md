---
id: 803
title: "POSIX transport band is announced but not applied — zenoh read/lease land on SCHED_FIFO 1, below every tier"
status: resolved
type: bug
area: platform
related: [issue-0765, issue-0623, issue-0506, rfc-0079]
---

## Problem

On POSIX, `nros-board-linux` states a reserved transport band of `90..99`
(RFC-0079, issue 0765) and prints it at boot:

    nros: transport tasks at SCHED_FIFO 90 (reserved band floor; most urgent tier is 80)

The kernel does not agree. With `cap_sys_nice+ep` granted, the zenoh read
and lease threads run on `SCHED_FIFO` at priority **1** — `sched_get_priority_min`,
the bottom of the range — while the application tiers correctly occupy 10 and 80.

So the band is inverted in exactly the way it was introduced to prevent: the
app preempts the link it publishes over. Issue 0623 for POSIX, one layer under
issue 0765's fix.

## Evidence

Four arms varying only `NROS_TMP_TRANSPORT_PRIO` (a temporary runtime override,
so one `setcap`'d binary measures every arm — a rebuild drops the file
capability). Entry confined to one CPU with `taskset`; three reps each; kernel
state read from `ps -L -o tid,cls,rtprio`.

| arm (requested) | main | transport ×2 | tier-high | ctrl msgs | telem msgs |
| --- | --- | --- | --- | --- | --- |
| 90 (the RFC floor) | FF 10 | **FF 1** | FF 80 | 1997–1998 | 198–199 |
| 40 | FF 10 | **FF 1** | FF 80 | 1997–1999 | 199 |
| 5 | FF 10 | **FF 1** | FF 80 | 1998–1999 | 197–199 |
| 0 (call skipped) | FF 10 | **TS** | FF 80 | 1998–1999 | 198–199 |

Two facts pin it down:

- **The requested value is discarded.** 90, 40 and 5 produce byte-identical
  kernel tables. The number never reaches the thread.
- **The code path is live.** Arm 0 skips `zpico_set_task_config` entirely and
  the threads stay `SCHED_OTHER`, so it is that call which puts them on
  `SCHED_FIFO` — at the floor's minimum rather than the requested priority.

`applied=2 eperm=0` in every arm: the capability is effective and both tiers'
`sched_setscheduler` calls succeed. This is not a privilege failure.

## Why it stayed hidden

The threads *are* `SCHED_FIFO`, so any check that asks "did the band apply?"
by looking at the POLICY says yes. Only the priority is wrong, and the boot
line reports the requested value rather than the achieved one — the process
tells you what it asked for, never what it got.

## Notes for the fix

`zpico_posix_fifo_set_priority` clamps into `[sched_get_priority_min,
sched_get_priority_max]` and sets policy, `PTHREAD_EXPLICIT_SCHED`, and the
param on the attr; `zpico_open` copies the attr into the session and
`zp_start_read_task` forwards it to `pthread_create`. Reading that chain does
not reveal where the value is lost — priority 1 is exactly what
`zpico_posix_rt_permitted()` puts on the *calling* thread while probing, which
is what an inheriting spawn would pick up. Instrumentation (`TEMP-0079-DIAG`)
is in the tree to report what the attribute actually holds and what policy the
creating thread has at spawn time; confirm the mechanism before fixing.

Whatever the mechanism, the fix must also make the boot line report the
**achieved** priority, not the requested one, so this class cannot recur
silently.

## Related measurement

The same sweep found no delivery difference between any arm, including the
`SCHED_OTHER` control — at ~2000 msgs / 20 s over loopback the tiers sleep
most of their period, so priority ordering never binds. Justifying the value
of the floor needs a saturating workload and a latency measure, not a message
count. That question is RFC-0079's open one and stays open.

## Narrowing, 2026-08-26 — the attr is NOT where the value is lost

Two things are now established without privileges, and together they eliminate
the hypothesis this issue proposed.

### 1. The attribute carries the requested priority, through the copy

`zpico_set_task_config`'s exact POSIX sequence, replayed standalone — `memset`,
`pthread_attr_init`, then `zpico_posix_fifo_set_priority`'s three setters, then
the session's `s->read_task_attr = g_default_read_task_attr` struct assignment:

```
FIFO range: [1, 99]
after init                   policy=OTHER inherit=INHERIT prio=0
setters rc: policy=0 inherit=0 param=0
after set_priority(90)       policy=FIFO inherit=EXPLICIT prio=90
after struct copy            policy=FIFO inherit=EXPLICIT prio=90
```

All three setters return 0, and the byte-copy of a glibc `pthread_attr_t`
preserves policy, inherit-mode and param. This needs no capability: setting
fields on an attribute object never does, which is why it is checkable on a host
where the real path cannot run at all.

So neither the `memset`-before-`pthread_attr_init` nor the struct assignment
loses the value. (The copy does duplicate glibc's internal extension pointer,
which is a latent double-free hazard on `pthread_attr_destroy` — worth its own
look, but not this bug.)

### 2. The forwarding chain does not drop it either

Read at the pinned zenoh-pico commit:

```
zpico_open:   read_opts = s->read_task_configured ? &s->read_task_opts : NULL
zp_start_read_task(api.c:2152)      -> _zp_start_read_task(.., opt.task_attributes)
_zp_start_read_task(session.c:450)  -> _zp_unicast_start_read_task(&zn->_tp, attr, task)
_zp_unicast_start_read_task(read.c) -> _z_task_init(task, attr, ..)
_z_task_init(unix/system.c:131)     -> pthread_create(task, attr, fun, arg)
```

Nothing nulls, rewrites or re-inits the attribute along it.

### 3. Therefore the stated hypothesis is out

The Notes say priority 1 "is exactly what `zpico_posix_rt_permitted()` puts on
the calling thread while probing, which is what an inheriting spawn would pick
up". A spawn inherits only when the attr is NULL or carries
`PTHREAD_INHERIT_SCHED`. Section 1 shows it carries `PTHREAD_EXPLICIT_SCHED`,
and section 2 shows it arrives.

**Unless `read_task_configured` is false** — then `read_opts` is NULL, the
attribute never reaches `pthread_create`, and the thread inherits exactly as
described. That is the one branch left standing, and it is what the next
instrumented run should print first: `g_default_read_task_configured` and
whether `read_opts` is NULL at the `zp_start_read_task` call.

Two other candidates eliminated while here: the weak `zpico_set_task_config`
stub is FreeRTOS-only (`nros-board-freertos/c/freertos_c_entry.c`, not linked on
Linux — the phase-386 weak-body class does not apply), and `nros-board-linux`
passes a literal `TRANSPORT_BAND_FLOOR = 90`, so nothing normalises it on the
way in.

### Why it was not confirmed here

This host cannot run the path at all: `RLIMIT_RTPRIO` is 0 soft / 0 hard and
there is no `CAP_SYS_NICE`, so `zpico_posix_rt_permitted()` returns 0 and
`zpico_posix_fifo_set_priority` early-returns. Confirming needs
`setcap cap_sys_nice+ep` on the entry binary, which needs root.

### Correction to this issue's Notes

`TEMP-0079-DIAG` is **not in the tree** — `git grep` finds neither it nor
`NROS_TMP_TRANSPORT_PRIO`, the runtime override the four measured arms varied.
Whoever picks this up cannot re-run the table as written without restoring both.

## Resolved, 2026-08-27 — a `pthread_attr_t` read as an `nros_platform_task_attr_t`

Measured, not inferred. `read`/`lease` now run at **SCHED_FIFO 90**, above every
tier, which is what the band exists to state:

```
TID      CLS RTPRIO
294056   FF   10     main / tier `low`
294058   FF   90     zenoh read      <- was 1
294060   FF   90     zenoh lease     <- was 1
294061   FF   80     tier `high`
```

### Mechanism

`_z_task_init` on this build is NOT zenoh-pico's unix one.
`c/zpico/platform_aliases.c` supplies it for every platform except ThreadX
(`NROS_PLATFORM_ALIASES_SKIP_TASK` is set only there) and forwards
`task_attributes` straight to `nros_platform_task_init`, which reads it as
`nros_platform_task_attr_t *`.

But `ZENOH_LINUX` makes zenoh-pico's `unix.h` typedef `z_task_attr_t` to
`pthread_attr_t`, and the POSIX arm of `zpico_set_task_config` filled exactly
that. So the platform layer read 56 bytes of a `pthread_attr_t` as an nros
struct. The `priority` offset reads **0**, which the ABI documents as band value
0 — *least urgent* — so `nros_posix_native_priority` returned
`sched_get_priority_min(SCHED_FIFO)` = **1**.

That is why the requested value never mattered: pthread keeps its `schedparam`
elsewhere, so that offset is 0 for 90, for 40 and for 5. The issue's own table
("90, 40 and 5 produce byte-identical kernel tables") is this fact.

`nros_zenoh_generic_platform.h` predicted it in writing:

> `platform_aliases.c` forwards that value straight to
> `nros_platform_task_init`, which reads it as a `nros_platform_task_attr_t *`
> — correct only while the value is always NULL, which it was.

Issue 0765 made it non-NULL. Two headers disagreeing about one type is issue
0135's shape, named there and reached here.

### Fix

The POSIX arm fills an `nros_platform_task_attr_t` —
`NROS_PLATFORM_PRIORITY_RAW(n)`, because the reserved band is RAW SCHED_FIFO,
the vocabulary issue 0623 settled on — and `task_attributes` points at it. The
pointer width is unchanged, so `zp_task_read_options_t`'s layout is untouched.

**Both halves were needed.** Fixing only the process-wide default left the bug
alive: `zpico_open` re-points `task_attributes` at the session's own
`pthread_attr_t` copy, so the session needs storage of the right type too. The
first attempt changed the global, measured, and was still 1.

### The boot line now reports what it GOT

Required by this issue, and general rather than transport-specific, because
"reports the request, never the result" is the class. `nros_platform_task_init`
reads the priority back off the running thread and warns when the kernel
disagrees. Silent on a correct run; it is what would have caught this in a day.

### How it was found, and what misled me

Reading the chain proved nothing four times: the attribute is correct at every
point our code can see it, and every hypothesis from reading — inheritance, the
struct copy, `RLIMIT_RTPRIO`, an environmental clamp, `_zp_start_read_task`'s
"already running" early-return — was eliminated by measurement.

**The instrumentation itself lied, twice.** Dumping the attr with
`pthread_attr_get*` reads it through the same lens that wrote it, so it reported
a healthy `FIFO/EXPLICIT/90` while the consumer saw zeros; and a probe thread
spawned from our code with that same pointer really did come out at 90, because
`pthread_create` is the reader it was written for. Two self-consistent views of
one buffer, and the bug lived in the third.

What ended it was printing from the CONSUMER — one line in
`nros_platform_task_init` showing `priority=0 native=1` — which is the same
lesson issue 0801 recorded three days earlier: a value read where it is USED
proves what is happening; a value read where it is WRITTEN proves only that you
wrote it.

### Corrections to this issue

* `TEMP-0079-DIAG` is **not** in the tree, and neither is
  `NROS_TMP_TRANSPORT_PRIO`. The four measured arms cannot be re-run as written.
* The Notes' hypothesis — priority 1 inherited from the probe in
  `zpico_posix_rt_permitted()` — is wrong. That function no longer runs on this
  path at all and the threads were still 1.

### Reproducing without a host sudo

`tmp/setcap-via-docker.sh` grants `cap_sys_nice+ep` through a container
(`--cap-add SETFCAP`, `--network none`, the host's own `setcap` invoked via the
host loader, since the only local image lacks it and docker networking is broken
on this host). File capabilities live in an xattr on the file, so it persists —
and is dropped by every rebuild, so re-run it after each one.
