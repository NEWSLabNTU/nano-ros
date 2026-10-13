---
id: 1885
title: "`check-action-client-arena-budget` failed `action-client` on a config another leaf wrote: the single-package road never got issue 1869's per-configuration build dir"
status: resolved
type: bug
area: [build, gates]
severity: medium
found: 2026-10-14
resolved: 2026-10-14
resolved_in: PR #2158
related: [900, 616, 1869, phase-489, RFC-0100]
---

## Measured

```
check-action-client-arena-budget: 2 image(s) will fail at REGISTRATION:
  FAIL build/cargo-fixtures/linux-1511459464/nros-relwithdebinfo/action-client
      ARENA_SIZE=8192 B — NROS_EXECUTOR_ACTION_CLIENTS=0 but the image LINKS action client creation
  FAIL .../action-server  (same)
```

`linux-1511459464` is the cyclonedds fixture group: eleven single-package leaves
(`examples/native/rust/*`, nros-tests bins) share it as one `--target-dir`.
The leaf's own settings are correct: `action-client/build/native/nros-cargo.toml`
carries `NROS_EXECUTOR_ACTION_CLIENTS = "1"`, `MAX_CBS = "1"`.

The profile dir holds exactly ONE `build/nros-node-<hash>/out/nros_node_config.rs`,
and it says `ARENA_ACTION_CLIENTS = 0`. That is `listener`'s: reproduced with
the current CLI by building `action-client` then `listener` into one
`--target-dir` — one `nros-node` unit, one `out/`, last build script wins, and
the file read `ARENA_ACTION_CLIENTS = 0` after `listener`.

## Cause

Not a sizing bug (the binaries were each linked after their own build script
ran) but the same class as issue 0616/1869: one unit identity, several
configurations. Issue 1869 gave `nros build`'s WORKSPACE road a build dir per
sizing configuration (`<target-dir>/nros-class/<key>/build`) and taught the gate
to read a class's `deps/`. The SINGLE-PACKAGE road (`plan_leaf_image`,
every fixture under `examples/native/rust/`) was not touched, so those leaves
still thrash one `out/` (and re-run each other's build scripts, issue 0491's
shape). The gate attributes a profile dir's one config to every image in it, so
it condemned `action-client` and `action-server` on `listener`'s numbers — and
would equally have PASSED a real under-size the same way.

## Resolution

1. `leaf_settings::write_shared` applies `shared_target::apply` when the caller
   names a `--target-dir`; `nros build` on a leaf passes
   `--config build.build-dir=<target-dir>/nros-class/<key>/build`, as the
   workspace road does. `write` is unchanged for every other caller.
2. The gate refuses to attribute: an ordinary (non-class) profile dir holding
   more than one distinct program is reported `[not examined]` with the reason,
   instead of judged on whichever config was written last. A class dir is
   attributable by construction. Self-test cases pin all three shapes.

Sibling sweep: `grep -rn "knob_of\|nros_node_config.rs" scripts just` finds only
this gate reading the config by profile dir; `shared_target::apply` callers are
now the workspace and leaf roads (`grep -rn "shared_target::apply\|write_shared"
packages/cli`). The cmake road keys by content already (`nros_shared_cargo_dir`).
Old-layout fixture dirs stay on disk until rebuilt; they now read `[not
examined]` rather than red.
