---
id: 398
title: Recipes that hard-set a relative target dir escape the distrobox's
  CARGO_TARGET_DIR, so host and box share build-script binaries
status: open
type: bug
area: build
related: [0375, 0383]
---

## Problem

`scripts/dev/ros2-box-env.sh` redirects `CARGO_TARGET_DIR` to
`$HOME/.cargo-target-box` for one stated reason: cargo re-runs cached
build-script EXECUTABLES, and a host-built one cannot run in the box —

```
build-script-build: /lib/x86_64-linux-gnu/libc.so.6: version `GLIBC_2.39' not found
```

Recipes that set their own target dir escape that override. `check-workspace-embedded`
does `CARGO_TARGET_DIR=target-embedded cargo clippy …` (justfile:2264) — a
RELATIVE path, resolved against the shared checkout, so host and box write the
same tree. A host `just ci` followed by a box `just ci` dies in
`check-workspace-embedded` on a build script the host compiled, naming
`nros-rmw-cffi` — a crate that has nothing to do with the failure.

The recipe prints a hint for a DIFFERENT cause at that point ("a NEW host-only
member is leaking `std`… declare the new crate host-only"), which is what makes
this expensive: the hint is confident, unrelated, and points at whatever crate
was added most recently.

Same shape for the other explicit dirs: `target-zenoh` (4 sites),
`target-zenoh-fixture-posix`, `target-xrce`, `target-tls`,
`target-ros-edition-<distro>-<rmw>`.

## Repro

```sh
just ci                                              # host (glibc 2.44)
DBX_CONTAINER_MANAGER=docker distrobox enter ros2 -- \
    bash -c '. scripts/dev/ros2-box-env.sh && just ci'   # box (glibc 2.35)
```

Second run fails in `check-workspace-embedded`. `rm -rf target-embedded` clears
it until the next host run.

## Fix sketch

Make the dedicated dirs nest under the active `CARGO_TARGET_DIR` when one is
set, so the box gets its own copy and the host is unchanged:

```sh
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:+$CARGO_TARGET_DIR/}target-embedded"
```

Unset (host) → `target-embedded`, exactly as today. Set (box) →
`$HOME/.cargo-target-box/target-embedded`. Apply to every explicit target dir,
not just the one that surfaced (`grep -n 'target-dir\|CARGO_TARGET_DIR=' justfile just/*.just`).

Worth checking whether the `check-workspace-embedded` hint can distinguish the
two causes — a `GLIBC_.* not found` in the build-script output is unambiguous
and should print a different remedy.

## Notes

Found while running tier 1 in the box for the issue-0383 `-Werror` work
(2026-08-03). Not caused by that change; it predates it and reproduces on a
clean tree.
