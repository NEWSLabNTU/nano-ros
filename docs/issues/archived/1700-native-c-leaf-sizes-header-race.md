---
id: 1700
title: "Two leaves differing only in their entity facts (sizing descriptor, declared counts) shared one Corrosion cargo dir, so a C leaf compiled against a C++ leaf's sizes header"
status: resolved
type: bug
area: [build, testing]
severity: medium
found: 2026-10-05
related: [1100, 1382, 1684, 0945, 0616]
resolved: 2026-10-06
---

## What was measured

While measuring `just build-test-fixtures lane=tier1-host` for issue 1684
(fresh worktree, 32 cores, tree = `origin/main` 2026-10-05 plus CI-lane
changes only — no core crate touched), the `native` module failed twice in
a row, each time on a DIFFERENT native C leaf:

| run | leaf | 
| --- | --- |
| 1 | `examples/native/c/service-server/build-zenoh` |
| 2 | `examples/native/c/talker/build-xrce` |

Both with the same panic from `nros-build-helpers/src/shared.rs:722`:

    nros-cpp: …/nros-c-generated/nros/nros_config_generated.h was written by
    another crate with DIFFERENT probed sizes.
      EXECUTOR_OPAQUE_U64S: on-disk=2583 vs would-write=11331
      NROS_EXECUTOR_SIZE: on-disk=20664 vs would-write=90648

`ninja -C examples/native/c/talker/build-xrce` run on its own afterwards
succeeds; the fixture build fails on the same leaf again.

## Diagnosis — a missing KEY input, not a feature split

The first reading (that `nros-c` gets `rmw-xrce` and `nros-cpp`
`rmw-xrce-cffi`) was wrong: the `-cffi` names are aliases that turn on the
same `nros/rmw-cffi`. Each sizes-probe directory records what it was keyed
on (`nros-probe-key-inputs.txt`), and diffing the two directories in the panic
settles it:

    < knob NROS_DECLARED_INFRA_QUERYABLES=none
    < knob NROS_DECLARED_NODES=1
    < knob NROS_SIZING_DESCRIPTOR=…/examples/native/cpp/talker/build-xrce/nros/sizing/cpp_talker.toml

The header in the **C** talker's build dir was written by a probe run with the
**C++** talker's sizing descriptor. The path that carries it is the shared
Corrosion cargo directory (issue 0945): every leaf's `<build>/cargo` is a
symlink to `build/corrosion-cargo/<platform>/<key>`, and `nros-c-generated/`
lives inside it. `nros-c`'s key is features + rmw + board + caps + profile +
target + knobs, all computed when `nros-c` is configured. The entity facts
(`NROS_SIZING_DESCRIPTOR`, `NROS_DECLARED_*`, the budget and depth tables)
reach the same cargo command later, in `_nros_entity_facts_flush`, because they
exist only once every entry has registered. So they were never in the key.
`cpp/talker` (which has a descriptor) and `c/talker` (which has none) hashed to
one directory. Whichever leaf built last owned the header, and the other
leaf's `nros-cpp` refused the build. Had the values agreed by luck, a leaf
would have linked the other leaf's `libnros_c.a`, which is the 0500/0616
failure.

The NuttX lane had the same gap: its key was computed before the entity
facts it then hands cargo. The Zephyr west lane did not, because there
`NROS_SIZING_DESCRIPTOR` is a resolved knob and so is in
`nros_knob_key_fields()`.

## Fix

- `_nros_entity_facts_flush` re-keys first. When there are facts, it re-points
  `<build>/cargo` at the directory keyed on `nros-c`'s base key + knobs +
  facts. Re-pointing is the supported move: `nros_share_corrosion_cargo_dir`
  keeps the old directory for whoever still uses it. No facts means no change.
- The issue-0945 witness (`nros_assert_shared_cargo_dir_used`) reads the
  directory from a target property at generate time, so it follows the re-key.
- NuttX composes `nros_entity_facts_env` before its key and includes it.

Sweep: `git grep -n 'nros_shared_cargo_dir(\|nros_share_corrosion_cargo_dir('`.
That gives 4 call sites: `nros-c` (fixed), NuttX (fixed), Zephyr (descriptor
already in its knobs), and the key probe (exempt).

## Proof (2026-10-06)

The tree that had failed on this panic on three consecutive runs ran
`just build-test-fixtures lane=tier1` with the fix applied, incrementally
(not a fresh worktree). The `native` module (781 s), `zephyr` (826 s) and
`threadx_linux` (108 s) were all `OK`, and `DIFFERENT probed sizes` appeared
0 times. `examples/native/c/talker/build-xrce/cargo` and
`examples/native/cpp/talker/build-xrce/cargo` now point at different keyed
directories (`…/9e90282fa1fd`, `…/8a8134a04deb`). The whole recipe exits 0.
That needed one more fix on the same road:

- `px4_bridge_ffi` (compile-check) syntax-checks a generated message header
  that includes `<nros/nros_config_generated.h>`. It had no `-I` for one, so
  it failed on every host that has PX4. It now takes the per-build config
  headers the way `cxx_syntax_check` does.

A fresh-tree build is still to be confirmed. The first `run-matrix` tier-1
run is that confirmation.

## Acceptance

A native C leaf's two crates either agree on one sizes probe, or write to
separate headers; `build-test-fixtures lane=tier1-host` completes on a fresh
tree in one run.
