---
id: 2393
title: "A generated Zephyr Rust west application's backend dependency carries no ROS edition feature"
status: resolved
type: bug
area: rmw, zephyr, build, cli
related: [2372, 1547, 1893, phase-545, rfc-0071]
found: 2026-11-10
resolved_in: "4225459111"
---

**Found while fixing [issue 2372](../2372-out-of-tree-rmw-native-sim-west-app.md).
Not reproduced on a wire; found by reading the generated manifests side by
side.**

## What is wrong

`builder/west_app.rs::resolve_in` writes the backend dependency of a generated
Zephyr Rust west application as

```toml
<crate> = { path = "…", default-features = false, features = ["<platform_feature>"], optional = true }
```

It forwards the board's `platform_feature` when the crate declares it, and
nothing else. The cargo road's twin, `orchestration/backend_link.rs`
(`image_backend`), forwards the image's ROS edition (`ros-<edition>`, the
descriptor's `[rmw.link] edition_feature`) and the backend's capability
features. RFC-0071 D9 says the west entry gets both ("the edition and backend
capabilities on a direct backend dependency (the Zephyr west entry)").

Measured on the out-of-tree fixture (`fixtures/out-of-tree-rmw`), same
workspace, two images:

- `linux` (cargo road): `acme-rmw = { …, features = ["ros-humble", "platform-posix"] }`
- `zephyr` (west road, native_sim): `acme-rmw = { …, features = ["platform-zephyr"], optional = true }`

The in-tree zenoh backend's west dependency has the same shape.

## Why it matters

`nros-rmw-zenoh`'s data key expression is cfg-gated on the edition
(`keyexpr.rs`: RIHS01 type hash on iron/jazzy, `TypeHashNotSupported`
otherwise, issue 1893). The `nros` dependency of the same application does get
`ros-<edition>`, and `zephyr/CMakeLists.txt` resolves the edition for the C/C++
road (issue 1547). So a Rust Zephyr image with `ros_edition = "jazzy"` would
build `nros` for jazzy and the backend for humble, and publish humble keys
under a jazzy type hash. This builds cleanly and fails on the wire. Every
Rust Zephyr image in the tree is humble today, so nothing has shown it.

Capability features on the same dependency (`safety` → `safety-e2e`) are
not forwarded either. Whether the `zephyr/rust/safety` image gets the feature
some other way was not checked.

## Fix direction

Have `west_app::resolve_in` build the backend's feature list the way
`backend_link::image_backend` does: the edition (`[rmw.link]
edition_feature`, `{edition}` substituted), the capability features the image
declares, and the platform feature. Prefer one shared function over a second
spelling. Add a unit test beside
`an_out_of_tree_backend_resolves_through_the_scan`, and a jazzy Rust Zephyr
image if a wire test is wanted.

## Resolution (2026-11-10, commit 4225459111)

One derivation now serves every line that names a backend crate directly,
in `orchestration/backend_link.rs`:

- `derived_features(r, sys, edition)`: the image's ROS edition
  (`ros-<edition>`) and each capability the image declares that the backend
  implements, under the descriptor's `[rmw.capabilities]` name. Sorted.
- `dependency_features(r, dir, sys, edition, platform)`: that list, cut to
  what the crate at `dir` declares, followed by the board's platform
  feature(s) (`with_platform_features`).

`west_app::resolve_in` writes its backend line from `dependency_features`
and records the edition and capability part as
`WestApp::backend_derived_features`. The entry writer marks those derived
(RFC-0071 D13, so a materialised entry does not freeze them). It no longer
patches west lines whose key starts `nros-rmw-`. The cargo road's
`image_backend`, its line writer `entry::image_backend_lines`, the owned
adapter in `facade`, and `facade::derive`'s `backend_features` call the same
functions.

**Two corrections to the report above.**

- The in-tree zenoh backend's final west line was **not** missing the
  edition. `west_app` wrote `["platform-zephyr"]`, and then
  `entry::with_backend_features` added the selection's edition and
  capability features to any line whose key starts `nros-rmw-`. So
  `examples/workspaces/rust` `zephyr` already had
  `["ros-humble", "platform-zephyr"]`, and `examples/workspaces/safety`
  `zephyr_rust_safety` already had `["ros-humble", "safety-e2e",
  "platform-zephyr"]`. That is how the safety Zephyr Rust image got its
  capability. The defect was the key prefix: an out-of-tree crate
  (`acme-rmw`) got nothing.
- The ruling is RFC-0071 **D13** ("Features": "the edition and backend
  capabilities on a direct backend dependency (the Zephyr west entry)"), not
  D9. D9 is the C/C++ umbrella, which composes its backend line in CMake from
  `nros ws rmw-facts` (`NROS_RMW_EDITION_FEATURE`,
  `NROS_RMW_CAPABILITY_FEATURES`, `NROS_RMW_PLATFORM_FEATURES`). A Rust
  function cannot serve that line.

**Measured** with `nros build --dry-run`. The "before" binary was a CLI built
from a tree whose builder sources match `main`:

| image | before | after |
| --- | --- | --- |
| out-of-tree fixture + `[image.zephyr_jazzy] ros_edition = "jazzy"` | `acme-rmw … features = ["platform-zephyr"]` | `features = ["ros-jazzy", "platform-zephyr"]  # nros-derived: ros-jazzy` |
| the same with no edition stated (`zephyr`) | `["platform-zephyr"]` | `["ros-humble", "platform-zephyr"]` |
| `workspaces/rust` `zephyr`, `workspaces/safety` `zephyr_rust_safety` + `linux_rust_safety_{talker,listener}`, out-of-tree `linux` | | byte-identical |

One value moves that no manifest line reads: for an out-of-tree backend, the
selection's `backend_features` now carry the descriptor's capability name
(`acme-safety`), not the registry's `safety-e2e`. That field reaches only
`nros-rmw-*` lines, and every in-tree backend uses the registry's name.

Tests: `west_app::tests::the_west_backend_dependency_carries_the_edition_and_capabilities`
(jazzy, humble, a system-wide edition, and `safety` → `acme-safety`) and
`backend_link::tests::the_cargo_road_backend_dependency_features_are_pinned`.
No wire test was added. Every in-tree Rust Zephyr image is still humble.
