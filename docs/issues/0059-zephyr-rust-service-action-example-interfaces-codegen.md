---
id: 59
title: Zephyr rust service/action examples fail — generated `example_interfaces` crate not resolvable at cargo build
status: open
type: bug
area: codegen
related: [phase-244]
---

`zephyr-dual-line.yml` cells `rust/service-server`, `rust/service-client`,
`rust/action-server`, `rust/action-client` fail on **both** 3.7 and 4.4 lines
(8 jobs). Root cause:

```
Zephyr: generating Rust interface crates for rust/service-server
ws sync: refreshed [patch.crates-io] block in .../service-server/Cargo.toml
ws sync: done.
...
error: no matching package named `example_interfaces` found
FAILED: .../librustapp.a  (recipe `build-one` failed with exit code 1)
```

## What's known

- The companion pubsub cells (`rust/talker`, `rust/listener`) — which depend on
  the builtin `std_msgs` — **pass**. So the per-example codegen + `ws sync`
  `[patch.crates-io]` rewrite mechanism works in general.
- service/action cells differ only in depending on the **external**
  `example_interfaces` package (srv `AddTwoInts`, action types +
  `action_msgs`/`unique_identifier_msgs`).
- The build log shows the Zephyr build step ran the interface-crate generation
  and `ws sync` *did* refresh the patch block:
  `example_interfaces = { path = "generated/example_interfaces" }`.
- Yet cargo resolves dep `example_interfaces = "*"` to nothing →
  `no matching package named example_interfaces found`. In a no-index no_std
  build this message means the `[patch]` did **not** bind: either
  `generated/example_interfaces/` was never materialized, or its `Cargo.toml`
  `package.name` ≠ `example_interfaces` so the patch fails to match the
  crates-io name.

## Likely cause (to confirm)

The "generating Rust interface crates" step refreshed the patch block but did
not actually emit the `generated/example_interfaces` crate dir — only
builtin/std crates are generated, and external `example_interfaces` (srv +
action types) is skipped/failing silently. Needs local repro of
`just zephyr build-one rust/service-server zenoh` (or the underlying
`nros generate-rust` + `nros ws sync`) to confirm whether the crate dir is
missing vs misnamed.

Note: the **native** rust action/service examples were fixed in phase-244 E3
(example_interfaces feature forwarding + `rmw-cyclonedds` wiring). The Zephyr
variants were not part of that change and regress here independently.

## Fix direction

Make the Zephyr per-example interface-crate generation emit
`generated/example_interfaces` (and its action-side transitive deps
`action_msgs`, `unique_identifier_msgs`) before `ws sync` rewrites the patch
block, with the crate `package.name` matching the crates-io name the patch
targets. Fail loudly if a declared `<depend>`/`build_depend` interface pkg
produces no generated crate (silent skip is what hid this).

Cannot validate locally yet (no Zephyr SDK in sandbox). Found 2026-06-13 while
triaging the chronically-red zephyr-dual-line lane.
