---
id: 2393
title: "A generated Zephyr Rust west application's backend dependency carries no ROS edition feature"
status: open
type: bug
area: rmw, zephyr, build, cli
related: [2372, 1547, 1893, phase-545, rfc-0071]
found: 2026-11-10
---

**Found while fixing [issue 2372](archived/2372-out-of-tree-rmw-native-sim-west-app.md).
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
