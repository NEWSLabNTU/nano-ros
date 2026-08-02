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

### Second instance, FIXED (2026-08-03)

`scripts/build/link-determinism-fixture.sh` built with cargo (honouring
`CARGO_TARGET_DIR`) but copied from a HARDCODED `$repo_root/target/debug/libnros_c.a`.
In the box that copies the HOST's archive — built by some other lane with
different features — so `build/link-determinism/libnros_c.a` contained no
`nros_rmw_zenoh_register` at all and `staticlib_duplicate_symbols` failed with

    `-u nros_rmw_zenoh_register` did not pull the backend register entry into the image

i.e. a link-MODEL error message for what was a stale file from another machine
image. Fixed by resolving the archive from `${CARGO_TARGET_DIR:-$repo_root/target}`
and failing loudly when it is absent. The justfile recipes above are unfixed.

Worth sweeping for the rest of the class: any script or recipe that pairs a
`cargo build` with a hand-built `target/…` path.

### Third instance, and this one has no one-sided fix

`nros-launch-resolve` builds into its own `packages/cli/nros-launch-resolve/target/`,
outside the redirect, and `nros sync` invokes it by absolute path (issue 0285).
A host build links `libpython3.14.so` (Arch); in the box that is

    error while loading shared libraries: libpython3.14.so.1.0: cannot open shared object file

and a box build links `libpython3.10.so`, which the HOST then cannot load.
Unlike the CLI — where glibc's backward compatibility makes the box build
usable on both sides, which is why `nros_box_publish` works — the Python
soname is not compatible in either direction, so ONE binary cannot serve both.
It needs either a per-side path (a target dir that honours CARGO_TARGET_DIR,
as everything else here) or abi3 linkage.

Today the loser is whoever ran second: `just build-test-fixtures lane=native`
dies in `generate-bindings` mid-sync.

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
