---
id: 759
title: "The box env's SHARED-tree mode can build but never rebuild: it redirects
  CARGO_TARGET_DIR and leaves the RFC-0070 cache root and the leaf
  target-fixtures dirs pointing at the host's artifacts"
status: resolved
type: bug
area: build, testing
related: [issue-0400, issue-0401, rfc-0070]
---

## Problem

`ros2-box-sync.sh`'s header already establishes that host and box cannot share
one checkout, and the fix is the box's OWN tree — that is not this issue.
`ros2-box-env.sh` nevertheless still supports the shared mode, and says why the
redirect makes it work: "Sharing the host tree is still supported … and keeps
the redirect, because there the alternative is host-built build scripts dying
on GLIBC."

The redirect does not cover the paths the FIXTURE builds actually use:

1. `<repo>/build/cargo-fixtures/<family>/` — the RFC-0070 cache root. The
   fixture builders pass their own `--target-dir`, so `CARGO_TARGET_DIR` never
   applies.
2. `examples/**/target-fixtures/<plat>/` — leaf-relative BY CONTRACT, which
   issue 0401 established must NOT be redirected.

So in shared mode the box builds fine as long as every cargo unit is a
fingerprint HIT — a cached unit is reused without being executed — and fails
the moment one source changes and a host binary has to run:

    build-script-build: /lib/x86_64-linux-gnu/libc.so.6: version `GLIBC_2.39' not found

Delete those dirs and the next layer is the proc-macro `.so`, which the
compiler dlopens and reports AT A SOURCE LINE, so it reads as a compile error
in code that is fine:

    error: .../deps/libnros_macros-….so: … GLIBC_2.39 not found
     --> packages/boards/nros-board-freertos/src/entry.rs:187:16

Measured 2026-08-22 (shared tree, FreeRTOS lane): a full box build reported 12
fixtures built over a host-populated tree; a one-line edit to
`packages/boards/nros-board-freertos/c/freertos_run_tiers.c` then failed in
`build/cargo-fixtures/freertos`, and after wiping that, again in two leaves'
`target-fixtures/freertos`.

## Why the successful build is the harmful part

It is what makes the mode look usable. During #0636 verification a mutation
test rebuilt NOTHING for this reason, the build error scrolled past under
`tail -2`, and the unchanged museum binary PASSED — i.e. removing the seam
appeared not to matter, the one conclusion a mutation test exists to prevent.

## Direction

Either of these, not both:

- **Cover the remaining paths.** `NROS_BUILD_ROOT` exists for exactly this
  (RFC-0070) and `nros_tests::build_root` mirrors it, so build and resolver
  would move together. Deliberately not done here: phase-334 W2.b counts 236
  unmigrated path literals, so flipping the root today trades a loud GLIBC
  error for a quiet STALE split-brain. The leaf dirs cannot move at all (0401).
- **Refuse the mode loudly.** Given the leaf dirs are unfixable by
  construction, the honest option may be for `ros2-box-env.sh` to WARN (or
  refuse) when it lands on a tree with no `.nros-box-tree` marker and existing
  host fixture artifacts, naming `ros2-box-sync.sh`. Today the mode announces
  nothing and the first failure is four frames deep.

Interim: use the box tree (`scripts/dev/ros2-box-sync.sh`); if you must share,
`rm -rf build/cargo-fixtures/<family> examples/**/target-fixtures/<plat>`
before building on the other side.

## Resolved 2026-08-23 — the shared mode is refused, not repaired

Decided by the maintainer, and it settles the "Direction" section above by
rejecting its first option outright:

> The pitfall is that the build on host and distrobox share the build
> artifacts, while they have distinct compiler and libc. It'll be a source of
> issues because there is no guarantee to ensure both toolchains are
> compatible. In the case the distrobox is employed, do all jobs in the box.

That is a stronger statement than this issue was making. I had been treating
the collisions as a set of paths to cover — redirect the cache root, exempt the
leaf dirs, keep the mode working. But the two sides do not merely write to the
same directories: they produce objects with different compilers against a
different libc, and **nothing in the build system checks that those toolchains
agree**. The GLIBC errors are the visible half; a silently-accepted mixture is
the half that has no error at all. Covering paths would have made the mode fail
less often, which is worse than making it fail always.

`NROS_BUILD_ROOT` is therefore NOT the fix and phase-334 W2.b's 236 unmigrated
literals stop being a blocker for this issue.

**What landed.** `ros2-box-env.sh` refuses to configure at all when it is
sourced against a tree with no `.nros-box-tree` marker:

```
box: REFUSING to run against the host's own checkout (issue 0759).
       tree: /mnt/wd/data/projects/nano-ros  (no .nros-box-tree marker)

  Host and box have different compilers and a different libc, and they
  would share build artifacts here — nothing guarantees the two
  toolchains are compatible. A build may even SUCCEED, by reusing
  cached units it never executes, and fail only on the next edit.

  Give the box its own tree, then work there:
      scripts/dev/ros2-box-sync.sh
      cd /mnt/wd/data/projects/nano-ros-box

  Deliberate exception: NROS_ALLOW_SHARED_BOX_TREE=1, and say why.
```

`return 1`, so a `. ros2-box-env.sh && <cmd>` chain stops instead of running
the command in a half-configured environment. The escape hatch keeps the old
behaviour and now says what it is buying, matching
`NROS_ALLOW_SUBMODULE_REWIND`'s shape — the deliberate act stays possible and
stops being the default.

Verified all three paths: the shared tree refuses (rc=1), the box's own tree
configures with `CARGO_TARGET_DIR` correctly unset, and the hatch warns and
proceeds.

**Why a refusal and not a warning.** A warning is read once and then lives in
scrollback. This mode's whole hazard is that it looks like it worked, and the
session that filed this issue had the warning available — `ros2-box-sync.sh`'s
header documents every one of these failures — and still spent hours in the
shared tree, because each individual command succeeded.

