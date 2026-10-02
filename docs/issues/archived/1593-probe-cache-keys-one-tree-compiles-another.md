---
id: 1593
title: "A metadata probe's `.unprobeable` marker is keyed on the nano-ros tree the CLI asked for, while its cached CMake build compiles whichever tree it was first configured against"
status: resolved
type: bug
area: [cli, build, metadata]
severity: medium
found: 2026-10-01
related: [issue-1280, issue-1469, issue-0627, issue-1575]
resolved_in: "branch fix/build-correctness-1593-1596-1599-1605"
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

## Resolution

Took the issue's preferred direction (pin, so the second tree is removed rather
than detected) and kept the other two as belts.

- **Pinned on every probe** (`metadata_probe_cmake.rs::configure_project`):
  the configure passes `-Dnano_ros_DIR=<nano_ros>` and
  `-D_NANO_ROS_CODEGEN_TOOL=<this nros>` (the documented caller pre-set,
  `NanoRosBootstrapCodegen` rung 1), and every probe cmake invocation —
  configure AND build, since a build can re-run cmake itself — carries
  `NROS_WORKSPACE` and `NROS_CLI` (`pin_probe_env`). `-D` rewrites the cached
  entries in place; nothing is wiped.
- **Read back** (`check_probe_root`): after configure, the cache's
  `nano_ros_DIR` is compared with `nano_ros`; a mismatch is a typed
  `ProbeRootMismatch` naming both roots and the `cmake … -Dnano_ros_DIR=` that
  repairs it, and the refresh writes NO marker for it (the failure is about the
  cache, not a component).
- **The marker records its compile root** (`root: <path>`, second line, read
  from the probe cache after the run; the cargo path uses `nano_ros` itself,
  which it path-depends on). `is_known_unprobeable` honours a marker only when
  key AND root match; a marker with no root line (every pre-fix one) is not
  honoured — one re-probe, once. A marker cannot be written without a root.

Measured on `examples/workspaces/realtime-c` (2 C components), A = this
worktree, B = the main checkout (`tmp/sync1593.sh`, `tmp/poison1593.sh`):

1. After `cmake <probe>/build -Dnano_ros_DIR=$B -D_NANO_ROS_CODEGEN_TOOL=$B/…/nros`,
   the PRE-FIX configure command (`cmake -S … -B … -DCMAKE_PREFIX_PATH=$A`)
   left `nano_ros_DIR=$B` and the B CLI cached, and its output named
   `$B/cmake/NanoRosMessageBounds.cmake` — the defect reproduced.
2. With the sidecars removed, `nros sync` from A with the fixed CLI: 16 s,
   `2 rebuilt`, cache back to `nano_ros_DIR=$A` and A's CLI; the generated
   Makefiles reference A 37 times and B's `cmake/`/`packages/` zero times.
3. An immediate second sync: `0 rebuilt, 2 already current`, 0 s, CMakeCache
   mtime unchanged — no reconfigure treadmill.

Acceptance 2 (B's marker never honoured from A, a rootless legacy marker not
honoured, the root line never read as the reason) and the mismatch refusal
naming both roots are unit tests:
`a_marker_from_another_compile_root_is_never_honoured`,
`a_build_dir_caching_another_tree_is_refused_naming_both`.

Not measured: a live sync hitting `ProbeRootMismatch` — with the `-D` pin it is
unreachable short of a cmake module overriding `nano_ros_DIR`, so only the unit
test exercises that arm. Not changed: the POSITIVE sidecar stamp carries no
root. After the pin, a successful probe compiled the key's tree by
construction, and a sidecar keyed on B's closure content equal to A's describes
identical sources, so trusting it is correct.
