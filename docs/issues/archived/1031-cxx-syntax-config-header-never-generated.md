---
id: 1031
title: "the cxx-syntax lane's config-header generation selects no RMW backend, so the probe returns 0, both build scripts decline to write, and three snippets compile against the committed stub — passing locally only on residue"
status: resolved
area: build, testing
severity: medium
found: 2026-09-04
resolved: 2026-09-04
related: [0834, 0464, 0475, 0088, 1030]
---

# A build that exits 0 having generated nothing

## Symptom

Every scheduled `gate.yml` run, three `cxx-syntax` compile-check fixtures failed:

```
== generating nros-cpp / nros-c config headers for cxx-syntax ==
== cxx-syntax: rclcpp_node_options ==
.../nros_config_generated.h:37:2: error: #error "nros_config_generated.h must be supplied per-build by the build system; see this stub for guidance."
.../nros_cpp_config_generated.h:59:2: error: #error "nros_cpp_config_generated.h must be supplied per-build ..."
.../polling_action_server.hpp:231:44: error: 'NROS_CPP_RAW_ACTION_SERVER_OPAQUE_U64S' was not declared in this scope
   cxx-syntax FAILED for rclcpp_node_options (no stamp; consuming test will report)
```

Same for `subscription_with_info` and `spin_until_future_complete` — the three
snippets that reach `nros.hpp`. The step itself exited 0.

## Root cause

`scripts/build/compile-check-fixtures.sh` generated the per-build headers with

```
cargo build -q -p nros-cpp -p nros-c --features nros-cpp/std,nros-c/std,nros-cpp/ros-humble
```

which selects **no RMW backend**. Both build scripts take their sizes from
`probe_nros_sizes`, which builds `nros` with the features this command resolves;
with no backend the probe returns `EXECUTOR_SIZE = 0`, and both scripts then
take the documented early return — "no RMW backend means no executor sizes to
ship" — writing neither header. The build succeeds, so the script's
`|| echo "...generation build failed"` never fires.

Adding `nros-cpp/rmw-zenoh-cffi` makes the probe yield real sizes and both
headers appear.

## Why it looked like a CI-only fault

It is not. Reproduced on a developer machine — but only after deleting
`target/nros-{c,cpp}-generated/`. Those directories are normally left over from
some other build that DID select a backend, so the lane passes on **residue**.
Before the second fix below, deleting them did not even restore them: the
headers are a build-script byproduct that nothing declared as an input, so
cargo considered the crates fresh and never re-ran the scripts. The
`drop_stamp_without_header` doc comment already described that state — "cargo
considers the crate up to date, so the byproduct is not re-emitted" — and noted
that `write_header_if_absent_or_verify` "already self-heals an absent header
when it RUNS". The missing half was making it run.

## Fix

1. `nros-cpp/rmw-zenoh-cffi` added to the generation command, with the reason
   at the call site.
2. `cargo:rerun-if-changed` on the generated header and its stamp, from both
   the writer and the declining path. Cargo now says
   `Dirty nros-cpp: the file target/nros-c-generated/nros/nros_config_generated.h is missing`
   and regenerates. Verified no rebuild loop: three consecutive builds report
   1, 0, 0 dirty units. Same discipline as the `rerun-if-changed` on the
   cbindgen output one function over, and issue 0475's shape one layer down.
3. A build that exits 0 having written no header now says so, instead of
   letting the failure surface twenty frames into a header as the stub's
   `#error`.

Verified: from a cleared `target/nros-{c,cpp}-generated/`, all three snippets
stamp `.compile-ok`.

## What stays open

The step exits 0 while three of its fixtures fail — by design ("no stamp;
consuming test will report"), but the consuming test does not run in that lane,
so nothing reported it for as long as it was broken. Changing that exit
semantics affects every compile-check lane and is not done here. It is the same
lane-visibility problem as issue 1030.
