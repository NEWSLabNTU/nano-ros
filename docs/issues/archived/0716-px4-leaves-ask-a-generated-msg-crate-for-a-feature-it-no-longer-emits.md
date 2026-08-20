---
id: 716
title: "Three px4 leaves ask `px4_msgs` for a `std` feature the generator stopped
  emitting — and the gitignored `generated/` tree hides it until someone regenerates"
status: resolved
type: bug
area: examples, codegen
related: [phase-316, phase-361, phase-359]
---

## Symptom

```
error: failed to select a version for `px4_msgs`.
package `px4-probe` depends on `px4_msgs` with feature `std`
but `px4_msgs` does not have that feature.
help: available features: default
```

Same for `px4-stub` and `offboard-companion`. Every cargo command in those
leaves fails at MANIFEST RESOLUTION — build, check, `cargo metadata`, all of
it.

## Why it lay dormant

The two halves drifted years apart and nothing joined them:

* `examples/px4/rust/companion/*/Cargo.toml` has carried
  `px4_msgs = { …, features = ["std"] }` since phase-316 (`b324bd496`), when the
  generator emitted a msg crate with `std = ["nros-core/std",
  "nros-serdes/std"]`. A tree generated back then still has it — e.g. the copy
  at `examples/px4/rust/xrce/offboard-companion/generated/px4_msgs/Cargo.toml`
  on this host.
* phase-361 W3.b settled the generated manifest on `[features] default = []`
  and nothing else. `rosidl_bindgen::generator::generate_cargo_toml` is what
  `nros generate-px4-msgs` actually writes, and it emits no `std`.

**`generated/` is gitignored** (repo rule: it is USER-side, regenerated per
host), so a checkout that never regenerated kept resolving against the older
crate and stayed green. The failure appears only when someone runs the
generator — which is exactly when they are least expecting a manifest error.

That is the general shape worth remembering: **a consumer pinned to a feature
of a GENERATED crate is an assertion about a generator's output, checked
nowhere.** The tracked half and the emitted half can drift indefinitely because
the artifact that would disagree is not in the repo.

## Fix

Drop `features = ["std"]` from the three leaves; keep `default-features =
false`. Nothing is lost — the feature does not exist, so nothing could have
been enabled by it, and the generated code is `no_std`-capable by construction.
`px4-probe`'s own manifest comment already said it "needs no `env` (and hence
no `std`) from the core"; the `px4_msgs` line contradicted its neighbour.

Verified by reconstructing what the generator emits (`[features] default = []`,
`nros-core`/`nros-serdes` with `default-features = false`, `heapless`) as
`generated/px4_msgs` and resolving the leaf both ways:

```
cargo metadata --format-version 1 --offline
# with    features = ["std"] -> exit 101, the error above
# without                    -> exit 0
```

## Not fixed here

Nothing gates this class. A check that every tracked manifest naming a feature
of a generated crate names one the emitter still writes would have caught it
the day W3.b landed — the emitter's feature list is a constant in
`generator.rs`, so the comparison is cheap. Worth doing if a second instance
appears; recorded now so the second instance is recognised as a repeat.
