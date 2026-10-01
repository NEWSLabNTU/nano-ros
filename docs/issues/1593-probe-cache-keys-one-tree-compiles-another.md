---
id: 1593
title: "A metadata probe's `.unprobeable` marker is keyed on the nano-ros tree the CLI asked for, while its cached CMake build compiles whichever tree it was first configured against"
status: open
type: bug
area: [cli, build, metadata]
severity: medium
found: 2026-10-01
related: [issue-1280, issue-1469, issue-0627, issue-1575]
---

## What happened (reported by the issue-1575 agent; not re-measured)

While landing issue 1575 in a nested worktree, the agent's first `nros sync`
ran with the PARENT checkout's `nano_ros_ROOT` and PATH: issue 1280's
inherited environment. Two things followed:

- The C/C++ metadata probe's build directory cached `nano_ros_DIR` and
  `_NROS_CLI_RESOLVED` pointing at the parent checkout.
- The four components' probes failed to compile, with
  `expected template-name 'NodeWithTimers'`, because they compiled against
  the parent tree's headers. Each was marked `<component>.json.unprobeable`.

After that, every later sync, run correctly from the worktree, reported the
four as unprobeable, until the agent reconfigured the probe build directory
with explicit overrides and deleted its own markers by hand.

## Why the cache did not notice

`metadata_refresh.rs::probe_inputs_key(package_root, nano_ros)` keys the
marker on three things:

- the component's source digest;
- the CLI's own source stamp;
- `probe_closure_digest(nano_ros)`, which is every source the probe compiles
  in the **`nano_ros` root the CLI passes**.

That is the right key, but for the tree the CLI *meant*. The probe's CMake
build directory persists across syncs, and its CMakeCache keeps whatever
`nano_ros_DIR` / `_NROS_CLI_RESOLVED` it was first configured with. So the
key can describe tree A while the compile ran against tree B:

- A failure caused by B is recorded under A's key.
- A sync from A finds A's key, trusts the marker and never re-probes.

The marker is "fresh" for a probe that never ran against A. This is issue
0627's rule ("a stamp over a different closure is never fresh") broken at
the seam between the key and the compile.

## Direction

The compile's real inputs have to agree with the key. Any of:

- **Pin the probe's configure to the key's root on every probe.** Pass
  `-Dnano_ros_DIR=<nano_ros>/…` and the CLI path explicitly so a cached value
  can never win, and treat a CMakeCache naming another root as a reconfigure.
  `cmake <dir>` with the new `-D` values is enough; nothing is wiped.
- **Or key on what the cache says.** Read the probe build directory's cached
  root back and refuse to write a marker when it is not `nano_ros`.
- **And record the compile root in the marker**, so a mismatch reads as
  stale rather than authoritative.

Prefer the first: it removes the second tree rather than detecting it.

## Acceptance

1. Configure a probe build directory against checkout B and run a sync whose
   `nano_ros` is checkout A. The probe either compiles against A, or refuses
   to write an `.unprobeable` marker and names both roots.
2. A marker written from B's failure is never honoured by a sync from A.
3. A no-op sync from A stays a no-op (no reconfigure treadmill).
