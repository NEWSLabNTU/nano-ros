---
id: 590
title: "Zephyr + Cyclone: `ddsrt` posix `environ.c` calls setenv/unsetenv, which
  Zephyr's libc does not declare — the cell cannot compile"
status: resolved
type: bug
area: rmw/zephyr
related: [issue-0557, issue-0566, issue-0371, rfc-0005]
---

## Symptom

On a freshly provisioned Zephyr workspace (3.7 line, SDK 0.16.8), every
cyclonedds cell of `just zephyr build-fixtures` fails to compile:

```
FAILED: modules/nros/CMakeFiles/nros.dir/…/cyclonedds/src/ddsrt/src/environ/posix/environ.c.obj
…/ddsrt/src/environ/posix/environ.c:53:7: error: implicit declaration of
    function 'setenv'; did you mean 'getenv'? [-Wimplicit-function-declaration]
…/ddsrt/src/environ/posix/environ.c:75:7: error: implicit declaration of
    function 'unsetenv'; did you mean 'getenv'?
```

The zenoh and xrce cells on the same workspace build: 12 `zephyr.elf` images
produced, 4 failures, all of them cyclonedds.

## Reading

Cyclone's ddsrt picks its **posix** environ backend for Zephyr, and that file
calls `setenv`/`unsetenv`. Zephyr's libc declares them only behind its POSIX
options (`CONFIG_POSIX_API` / the `_POSIX_C_SOURCE` surface); without that they
are implicitly declared, which gcc ≥ 14 — the compiler the Zephyr SDK 0.16.8
ships — treats as an error rather than a warning.

Same *class* as the two zenoh-pico fixes already on that fork
(`int-conversion`, `implicit decl is fatal on gcc >= 14`) and as issue 0566,
which found the Zephyr port reaching for `CONFIG_POSIX_API` and giving up when
it is absent. The pattern to check for is a port assuming the POSIX surface on
a kernel that gates it.

## What is NOT established

* Whether this is new. The cyclone-on-Zephyr cells are known to RUN on the
  maintainer's host (issue 0557 reports a RUNTIME failure of the ACTION images,
  which presupposes they built), so this may be specific to the Zephyr version /
  SDK on this host rather than a regression.
* Which fix is right, and it is deliberately not chosen here: enable the POSIX
  env option in the Zephyr conf; give ddsrt a Zephyr environ backend (the shape
  issues 0371/0557 took for the sync backend — `DDSRT_WITH_ZEPHYR` already picks
  Zephyr-native types); or stub the two calls. **The Zephyr ddsrt seam is under
  active change by another session** (0557's native `k_mutex`/`k_condvar`
  backend), so whoever owns that work should pick, not a drive-by.

## It now fails TIER 1, not just the Zephyr lane (2026-08-15)

Provisioning the Zephyr workspace turned previously-SKIPPED cells into required
ones: `sched_dims_applied` reports

```
sched_dims: 4 of 12 cell(s) FAILED:
  CorePin/zephyr/rust: Test fixture binary MISSING for an in-lane coordinate:
    zephyr-workspace/build-ws-rs-realtime-entry-zenoh/zephyr/zephyr.exe
```

That entry is `zephyr-fixture-65` in the lane's make driver; the cyclonedds
cells are 13–18, and the driver has no `-k`, so the build stops long before 65
and the workspace entries are never produced. On a tier-1 run at HEAD this is
the ONE real failure — the other 39 are capability skips (no ROS 2 on this
host).

So the cost of this issue is no longer "the cyclone cells do not build": while
it stands, no Zephyr fixture after the cyclone group can be built at all, and
tier 1 is red on any host with a provisioned Zephyr workspace.

## Reproduce

```sh
just zephyr setup           # workspace + SDK (PEP 668 hosts: see e3225404c)
just zephyr build-fixtures  # zenoh + xrce build; the 4 cyclonedds cells fail
```


## RESOLVED 2026-08-16 — a Zephyr `environ` backend, plus four unrelated walls behind it

The compile error itself was one line's worth of cause: `ddsrt` picks its POSIX
environ backend on Zephyr, that TU calls `setenv`/`unsetenv`, and Zephyr's libc
declares them only behind its POSIX options — an implicit declaration, which the
gcc 14 the SDK 0.16.8 ships treats as an error.

Fixed with a Zephyr backend (`zephyr/cyclonedds-zephyr/environ_zephyr.c`) and
the TU swap that already exists for its siblings, rather than by enabling
`CONFIG_POSIX_API`. This issue deliberately declined to choose among three
options; the reasoning for this one:

* **not `CONFIG_POSIX_API=y`** — turning the whole POSIX surface on to obtain
  two functions drags in what #0566 is about, and Zephyr's POSIX objects come
  from fixed static pools (#0371 / #0496). That pooling is precisely what the
  native `k_mutex`/`k_condvar` sync backend exists to escape; adding a second
  reason to depend on it would undo that work.
* **not a bare stub** — same edit, less honest about why.

A Zephyr image has no process environment (nros bakes locator/domain/node at
build time via `option_env!` for exactly that reason), so `getenv` reports
NOT_FOUND and the mutators return OK without storing. OK rather than an error
because cyclone calls `ddsrt_setenv` on paths that must not fail; nothing can
observe the dropped write, since the only reader is `ddsrt_getenv` in the same
file. Argument validation matches the POSIX TU exactly.

### The issue's real cost was not this bug

The "It now fails TIER 1" section was right that the damage was the make driver
stopping at the cyclone group. What it could not know is that FOUR more walls
stood behind it, each invisible until the previous cleared:

| leaf | wall | origin |
| --- | --- | --- |
| 13-18 | `setenv` implicit declaration | this issue |
| 10 | task storage probes named `pthread_t`, undeclared without CONFIG_POSIX_API | #0566 vs phase-359 W10 |
| 10 | `msg_to_cyclone_idl.py` returned a script that exists and cannot import | the rosidl ladder's dead path |
| 56 | `BackendDynamic` arm gated on the CONSUMER's `alloc`, not the provider's | phase-360 W2.a |
| 70 | two `nros_platform` units sharing one corrosion target dir | #0616 |

Three of the five are collisions between commits that were each fine alone.
Nobody had run the lane end to end since they landed, so they stacked.

### Acceptance — met

`just zephyr build-fixtures` completes: **0 failures across all 70 leaves**,
`Zephyr test fixtures built successfully`, leaf 70 producing a 13,299,200-byte
`zephyr.exe`. That is the first complete Zephyr fixture build on this host since
the breakage began, and it also unblocks phase-353 W2's full-lane measurement
and turns phase-363 W5's two realtime-entry staleness tests green.
