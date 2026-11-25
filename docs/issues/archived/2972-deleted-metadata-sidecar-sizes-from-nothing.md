---
id: 2972
title: "A deleted metadata sidecar left the configure current, so the build sized the image from nothing"
status: resolved
resolved: 2026-11-25
resolved_in: "the change that adds this issue"
type: bug
severity: high
area: [build, sizing, metadata]
related: [issue-2964, issue-0445, phase-530]
found: 2026-11-25
---

## What happened

Found while measuring issue 2964's Rust fix on host 118. A component's probe
writes its source-metadata sidecar into the source tree
(`src/<pkg>/metadata/<component>.json`). After the sidecar was deleted,
`nros build` did not re-run the probe, even with the component's source
touched. It printed

```text
nros build:   `zephyr`: nothing declared to derive sizes from (...)
```

and built the image on its configured pool knobs. The rpi-pico-w-zephyr
listener, 194,392 B with its sidecar, overflowed RAM by 94,832 B without it.
Only removing `build/` made it re-probe. Nothing said "stale": the missing input
read as "this image declares nothing", issue 0445's absorbing shape. The same
blindness could as easily under-size an image whose components declare more
than the configured pools hold.

## Cause

`nros build` configures first, unless the configure stamp is current
(`configure_stamp::currency`). The stamp's metadata check,
`metadata_refresh::stale_sidecar_components`, read each declared component's
sidecar and, on a read error, `continue`d. A deleted sidecar was therefore not
stale, the configure was skipped, and the probe (which runs inside the
configure's sync) never ran. The refresh itself was right all along:
`sidecar_is_fresh` treats a missing sidecar as stale. It just was never reached.

## Fix

The refresh's per-component decision is lifted into ONE function,
`metadata_refresh::plan_component`, returning a `ComponentPlan`:
- `Unsupported`: a host blocker, a C/C++ declaration the probe refuses, or no
  nano-ros path;
- fresh, for C/C++ or Rust;
- a failure recorded at THIS source, for C/C++ or Rust;
- probe, for C/C++ or Rust.

`refresh_stale_sidecars` acts on the plan. `stale_sidecar_components` asks the
same function, and calls a missing sidecar stale exactly when the plan writes
one. The configure's freshness and the refresh therefore cannot disagree about
which absence is expected. A component the probe cannot cover keeps no sidecar
on purpose and stays current, so there is no reconfigure loop.

A failure marker counts only while it is current, through the refresh's own
keyed check. A marker left from an older source no longer hides a deleted
sidecar.

The review that shaped this caught a first version that RE-STATED the
refresh's rules in a second predicate. It had already drifted in two places:
it looked only at whether a marker existed, not whether it was current, and it
never asked the C/C++ freshness question.

Every other reader of the sidecar directories (`leaf_entity_env`,
`west_leaf_sizing`, the planner) runs after `ensure_current`, so it now sees a
re-probed sidecar rather than an empty directory.

## Verified

- `configure_stamp`:
  - `stale_when_a_metadata_sidecar_is_deleted` fails with the rule disabled
    and passes with it;
  - `a_stale_failure_marker_does_not_hide_a_deleted_sidecar`.
- `metadata_refresh`, per arm: a missing C/C++ or Rust sidecar plans a probe;
  a failure recorded at this source holds it off (the negative control); a
  marker from an older source does not; no nano-ros path is `Unsupported`.
- End to end, `nros new hl --board linux --template listener`: build, delete
  `hl/src/hl_node/metadata/hl_node.json`, build again. The second build
  reports the sidecar missing, re-probes (`1 rebuilt`) and writes it back. A
  rebuild with the sidecar present reports nothing missing.
